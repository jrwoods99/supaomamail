import QtQuick
import "../account/Model.js" as Model
import "../providers/Registry.js" as Provider
import "When.js" as When

// What is snoozed in one mailbox and until when, and the calls that change it.
// The backend keeps the times and wakes the mail (`src/snooze/`); this asks,
// remembers the answer for the rows that draw it, and says when mail is back.
QtObject {
  id: client
  required property var account
  readonly property var backend: account ? account.backend : null
  // API 6, and asked for by name as well: a backend from another line that
  // reached API 6 without snoozing must never be sent one.
  readonly property bool available: !!backend && backend.ready === true
    && Number(backend.apiVersion) >= 6 && !!backend.protocolInfo
    && Array.isArray(backend.protocolInfo.methods)
    && backend.protocolInfo.methods.indexOf("snooze.set") >= 0
  // Whether this mailbox is one the backend snoozes for at all. Nothing is
  // asked on behalf of any other, so a backend with no Gmail mailbox never
  // starts its snooze worker.
  readonly property bool serves: available && !!account
    && Provider.can(account.providerId, "snooze", account.capabilityRefusals)
  // Whose snoozes these are. A new mailbox can learn its address after this
  // object exists, from its first profile read.
  readonly property string owner: account ? String(account.accountId || "").toLowerCase() : ""
  // { messageId: wakeAt }, epoch milliseconds, for this mailbox only.
  property var wakeTimes: ({})
  property double revision: -1
  // What the last snooze from this window sent away, for its Undo:
  // { ids, at }, or null once the moment for it has passed.
  property var latest: null
  property bool retired: false

  function owns(accountId) {
    return owner !== "" && String(accountId || "").toLowerCase() === owner
  }

  // A backend that restarted counts its revisions from nothing again, and a
  // different mailbox's answers are not this one's.
  function reset() {
    revision = -1
    wakeTimes = ({})
    supersede()
    sync()
  }

  // A newer snooze is on its way, so the last one's Undo has had its moment:
  // Undo must never take back an older snooze while a newer one is pending.
  function supersede() {
    latest = null
    undoWindow.stop()
  }

  function wakeAt(id) {
    return Number(wakeTimes[String(id || "")] || 0)
  }

  // An answer can outlive this object -- a mailbox removed, or switched to
  // another provider, with a call in flight -- so each one checks, the way
  // SendQueue does, that the object is still there before touching it.
  function request(method, params, callback) {
    if (!available) {
      Qt.callLater(function() {
        if (client && typeof client.apply === "function" && !client.retired)
          callback(null, "Mail backend unavailable")
      })
      return false
    }
    params.accountId = String(account.accountId || "")
    backend.call(method, params, function(result, error) {
      if (client && typeof client.apply === "function" && !client.retired)
        callback(result, error ? String(error.message || "Mail backend request failed") : "")
    })
    return true
  }

  // A snapshot older than one already applied says less than it: answers can
  // land out of order, and the revision is the backend's own count.
  function apply(snapshot) {
    if (!snapshot || !Array.isArray(snapshot.entries)) return
    var at = Number(snapshot.revision)
    if (isFinite(at) && at < revision) return
    if (isFinite(at)) revision = at
    var times = ({})
    for (var i = 0; i < snapshot.entries.length; i++) {
      var entry = snapshot.entries[i]
      if (entry && owns(entry.accountId)) times[String(entry.messageId)] = Number(entry.wakeAt) || 0
    }
    wakeTimes = times
  }

  function sync() {
    if (!serves || owner === "") return
    request("snooze.snapshot", {}, function(result, error) { if (!error) client.apply(result) })
  }

  // The dispatch for a snooze verb, called where the account sends every
  // other action, with the same `done`: a failure puts the rows back.
  function send(action, ids, done) {
    var at = Model.snoozeUntil(action)
    var list = ids.slice()
    if (at > 0) supersede()
    request(at > 0 ? "snooze.set" : "snooze.cancel",
      at > 0 ? { ids: list, wakeAt: at } : { ids: list }, function(result, error) {
        if (!error && at > 0) {
          client.apply(result)
          client.latest = { ids: list, at: at }
          client.undoWindow.restart()
        }
        done(result, error)
      })
  }

  // The snooze taken back: the messages are in the Inbox again now, read or
  // unread as they were, with no notification. The rows return with the
  // reload this asks for.
  function undo() {
    var last = latest
    if (!last) return false
    latest = null
    request("snooze.cancel", { ids: last.ids }, function(result, error) {
      if (error) { client.account.fail(error); return }
      client.account.note("Snooze undone")
      client.account.loadMessages(false, true, "")
    })
    return true
  }

  function label(action) {
    var at = Model.snoozeUntil(action)
    return at > 0 ? "Snoozed until " + When.describe(at, Date.now()) : "Back in the Inbox"
  }

  readonly property Timer undoWindow: Timer {
    interval: 8000
    onTriggered: client.latest = null
  }

  onServesChanged: reset()
  onOwnerChanged: reset()

  readonly property Connections backendEvents: Connections {
    target: client.backend
    function onNotification(method, params) {
      if (method !== "snooze.changed" || !params || !client.owns(params.accountId)) return
      client.sync()
      var notice = Model.snoozeNotice(params.woken)
      if (!notice) return
      client.account.refresh()
      client.account.announce(notice.title, notice.body, notice.messageId)
    }
  }

  Component.onCompleted: sync()
  Component.onDestruction: retired = true
}
