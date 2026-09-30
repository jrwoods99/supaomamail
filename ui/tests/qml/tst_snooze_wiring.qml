import QtQuick 2.15
import QtTest 1.3
import "../.." as Omamail
import "BackendFixture.js" as BackendFixture
import "NativeIntentFixture.js" as NativeIntentFixture
import "../../snooze/When.js" as When

// Snoozing through the real objects: the key, the row menu and the reader's
// button open one picker; the answer goes through the same optimistic action
// every verb does and reaches the backend as `snooze.set`; Alt+Z takes it back.
// Unit tests own each rule, and this catches a forwarding step dropped between.
Item {
  width: 900
  height: 600

  QtObject {
    id: fakeShell
    function hide(_id) {}
  }

  Omamail.Service {
    id: mailService
    shell: fakeShell
    manifest: ({ id: "omamail", __sourceDir: "" })
  }

  Omamail.App {
    id: app
    service: mailService
    shell: fakeShell
  }

  TestCase {
    id: suite
    name: "SnoozeWiring"
    when: windowShown

    property var fixture: null
    readonly property var snoozing: ["snooze.snapshot", "snooze.set", "snooze.cancel", "snooze.wake"]
    // What the fake backend holds snoozed, { messageId: wakeAt }, and its
    // revision count, which only rises: the client ignores an older answer.
    property var waiting: ({})
    property int revision: 0

    function snapshot(accountId) {
      var entries = []
      for (var id in suite.waiting)
        entries.push({ accountId: accountId, messageId: id, wakeAt: suite.waiting[id], state: "pending" })
      suite.revision++
      return { revision: String(suite.revision), entries: entries }
    }

    function initTestCase() {
      fixture = BackendFixture.markReady(mailService, 6)
      NativeIntentFixture.install(mailService)
      fixture.answers = ({
        "snooze.snapshot": function(params) { return suite.snapshot(params.accountId) },
        "snooze.set": function(params) {
          var next = Object.assign({}, suite.waiting)
          for (var i = 0; i < params.ids.length; i++) next[params.ids[i]] = params.wakeAt
          suite.waiting = next
          return suite.snapshot(params.accountId)
        },
        "snooze.cancel": function(params) {
          var next = Object.assign({}, suite.waiting)
          for (var i = 0; i < params.ids.length; i++) delete next[params.ids[i]]
          suite.waiting = next
          return { cancelled: params.ids }
        }
      })
      advertise(snoozing)
    }

    // Snoozed as far as the backend knows, and the client has read it back.
    function snoozeInBackend(account, id, at) {
      var next = Object.assign({}, suite.waiting)
      next[id] = at
      suite.waiting = next
      account.snoozes.sync()
      tryVerify(function() { return account.snoozes.wakeAt(id) === at }, 2000)
    }

    // What the connected backend says it can be asked. Snoozing is refused by
    // name as well as by API revision.
    function advertise(methods) {
      mailService.backend.protocolInfo = { apiVersion: 6, protocol: 1, version: "0.0.0", methods: methods }
    }

    function named(item, objectName) {
      if (!item) return null
      if (item.objectName === objectName) return item
      var values = item.children || []
      for (var i = 0; i < values.length; i++) {
        var found = named(values[i], objectName)
        if (found) return found
      }
      return null
    }

    function sent(method) {
      return fixture.requests.filter(function(request) { return request.method === method })
    }

    function row(id) {
      return { id: id, labelIds: ["INBOX"], unread: false, starred: false, inInbox: true,
        subject: id, time: "now", from: { display: "Sender" }, snippet: "" }
    }

    function installAccount(provider, id, email) {
      mailService.accountsLoaded = true
      mailService.accountList = ({
        version: 1,
        accounts: [{ id: id, email: email, provider: provider }],
        activeId: id
      })
      tryCompare(mailService, "accountCount", 1)
      tryVerify(function() { return mailService.current !== null }, 1000)
      wait(0)
      return mailService.current
    }

    // Signed in, with the transport held below the action pipeline: these
    // tests want the optimistic result and the backend call, not Google.
    function gmail() {
      var account = installAccount("gmail", "me@example.com", "me@example.com")
      account.auth.credentials = ({
        clientId: "123-test.apps.googleusercontent.com",
        clientSecret: "test",
        projectId: "test"
      })
      account.auth.toolsChecked = true
      account.auth.missingTools = []
      account.auth.loggedIn = true
      account.auth.refreshBusy = true
      tryCompare(account, "ready", true)
      tryCompare(account, "canSnooze", true)
      account.messages = [row("one"), row("two")]
      return account
    }

    function init() {
      advertise(snoozing)
      suite.waiting = ({})
      fixture.requests = []
      app.opened = true
      app.cursorId = ""
      app.checkedIds = []
      mailService.accountList = ({ version: 1, accounts: [], activeId: "" })
      wait(0)
    }

    function cleanup() {
      var picker = named(app, "snooze-picker")
      if (picker && picker.opened) picker.close()
      mailService.accountList = ({ version: 1, accounts: [], activeId: "" })
      wait(0)
    }

    function test_h_snoozes_the_cursor_row_and_alt_z_takes_it_back() {
      var account = gmail()
      var picker = named(app, "snooze-picker")
      app.cursorId = "one"
      app.runShortcut("snooze", "h")
      tryCompare(picker, "opened", true)
      picker.searchQuery = "2m"
      picker.cursorIndex = 0
      var at = picker.rows[0].at
      picker.chooseCursor()
      tryCompare(picker, "opened", false)

      tryVerify(function() { return sent("snooze.set").length === 1 }, 2000)
      var request = sent("snooze.set")[0].params
      compare(JSON.stringify(request.ids), JSON.stringify(["one"]))
      compare(request.wakeAt, at)
      compare(request.accountId, "me@example.com")
      tryCompare(account, "actionPreparations", 0)
      compare(account.messages.length, 1, "the row leaves the Inbox at once")
      compare(account.messages[0].id, "two")
      compare(app.cursorId, "two", "and the cursor moves on as it does for an archive")
      tryVerify(function() { return !!mailService.snoozes && !!mailService.snoozes.latest }, 2000)
      tryCompare(mailService, "actionStatus", "Snoozed until " + When.describe(at, Date.now()))
      verify(named(app, "snoozed-toast").visible, "the snooze says so, with its undo")

      app.runShortcut("undoSend", "Alt+Z")
      tryVerify(function() { return sent("snooze.cancel").length === 1 }, 2000)
      compare(JSON.stringify(sent("snooze.cancel")[0].params.ids), JSON.stringify(["one"]))
      tryCompare(mailService, "actionStatus", "Snooze undone")
      compare(mailService.snoozes.latest, null)
    }

    function test_mail_already_snoozed_is_offered_back_now() {
      var account = gmail()
      snoozeInBackend(account, "one", Date.now() + 3600000)
      var picker = named(app, "snooze-picker")
      app.cursorId = "one"
      app.runShortcut("snooze", "h")
      tryCompare(picker, "opened", true)
      compare(picker.rows[0].label, "Unsnooze now")
      picker.cursorIndex = 0
      picker.chooseCursor()
      tryVerify(function() { return sent("snooze.cancel").length === 1 }, 2000)
      compare(JSON.stringify(sent("snooze.cancel")[0].params.ids), JSON.stringify(["one"]))
      compare(sent("snooze.set").length, 0)
    }

    function test_a_mailbox_that_cannot_snooze_refuses_before_the_picker() {
      installAccount("imap", "imap:me@example.com", "me@example.com")
      app.cursorId = "message-1"
      compare(app.openSnoozePicker(), false)
      compare(named(app, "snooze-picker").opened, false)
      compare(mailService.actionStatus, "IMAP has no snooze")
    }

    function test_gmail_on_a_backend_that_cannot_snooze_says_why() {
      advertise([])
      var account = installAccount("gmail", "me@example.com", "me@example.com")
      compare(account.canSnooze, false)
      app.cursorId = "one"
      compare(app.openSnoozePicker(), false)
      compare(named(app, "snooze-picker").opened, false)
      compare(mailService.actionStatus, "This needs an updated backend")
    }

    function test_the_row_menu_and_the_readers_button_open_it_for_their_message() {
      var account = gmail()
      var picker = named(app, "snooze-picker")
      var menu = named(app, "rowMenu")
      verify(menu !== null)
      menu.actionRequested("snooze", "two")
      tryCompare(picker, "opened", true)
      compare(app.cursorId, "two", "the picker is about the row the menu was over")
      picker.close()
      tryCompare(picker, "opened", false)

      account.selectedId = "one"
      account.selectedMessage = row("one")
      var button = named(app, "reader-snooze-button")
      verify(button !== null, "the reader has a snooze button")
      button.clicked()
      tryCompare(picker, "opened", true)
      compare(app.cursorId, "one")
    }

    function test_the_snoozed_mailbox_says_when_each_comes_back() {
      var account = gmail()
      var at = Date.now() + 26 * 3600000
      account.mailboxKey = "snoozed"
      snoozeInBackend(account, "one", at)
      account.messages = [row("one")]
      tryVerify(function() {
        var mark = named(app, "message-snoozed")
        return !!mark && mark.visible
      }, 1000, "a snoozed row carries the snooze glyph")
      compare(named(app, "message-time").text, When.compact(at, Date.now()))
    }
  }
}
