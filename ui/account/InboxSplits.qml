import QtQuick
import "../providers/Registry.js" as Provider
import "Model.js" as Model

// The Inbox's tabs -- Primary and Other, on Gmail -- where the provider splits
// its Inbox and the account has asked for them. A tab is a native query and
// nothing more, so the list, its cache and its paging follow from it the way
// they follow from any other query, and nothing here names a provider.
//
// The mailbox stays "inbox" under either tab. That is what keeps every rule
// the Inbox already has -- archiving removes the row, a star keeps it -- true
// on both tabs without restating one of them.
QtObject {
  id: splits
  required property var account

  // The tab chosen, or "" for the first. Kept when the Inbox is left, so going
  // back lands where it was.
  property string selected: ""
  // Tabs holding unread mail that no count covers, by key.
  property var unreadDots: ({})
  property int dotSerial: 0

  readonly property var available: account ? Provider.splitsFor(account.providerId, "inbox") : []
  // A custom default search is the Inbox resolved in Rust, which a tab would
  // quietly override; a search or a label is not the Inbox at all.
  readonly property bool customInbox: !!account && account.defaultQuery.trim() !== ""
    && account.defaultQuery.trim() !== Provider.get(account.providerId).inheritedDefault
  readonly property bool viewingInbox: !!account && account.mailboxKey === "inbox"
    && account.searchQuery === "" && account.searchRaw === "" && account.rawQuery === ""
  readonly property bool enabledHere: !!account && account.inboxSplits === true
  readonly property bool shown: enabledHere && available.length > 1 && !customInbox && viewingInbox
  readonly property string current: shown ? available[Math.max(0, indexOf(selected))].key : ""
  readonly property string query: shown ? available[indexOf(current)].query : ""
  // What the strip draws. The tab whose unread mail is exactly what the
  // Unread mailbox counts carries that count; any other tab says only that
  // something is there, because a number for it would be Gmail's estimate.
  readonly property var tabs: {
    var out = []
    var counted = account ? Provider.unreadQuery(account.providerId) : ""
    for (var i = 0; i < available.length; i++) {
      var tab = available[i]
      var counts = tab.unreadQuery !== "" && tab.unreadQuery === counted
      out.push({ key: tab.key, label: tab.label,
        count: counts ? Math.max(0, Number(account.inboxUnread) || 0) : 0,
        dot: !counts && unreadDots[tab.key] === true })
    }
    return out
  }

  // Turning the tabs on or off while the Inbox is on screen changes what it
  // lists, and nothing else would ask for the list again.
  onEnabledHereChanged: {
    if (viewingInbox && account.ready) reload()
  }

  function indexOf(key) {
    for (var i = 0; i < available.length; i++) {
      if (available[i].key === key) return i
    }
    return -1
  }

  function reload() {
    account.clearSelection()
    account.messages = []
    account.loadedDepth = 0
    account.listLoaded = false
    account.loadMessages(false)
    refreshUnread()
  }

  function select(key) {
    if (!shown || indexOf(key) < 0 || key === current) return false
    selected = key
    reload()
    return true
  }

  // Tab and Shift+Tab walk the tabs and wrap, the way the account switcher's
  // rows do: there are two of them, and the far one is always one press away.
  function step(delta) {
    if (!shown) return false
    return select(available[Model.wrappedIndex(indexOf(current), delta, available.length)].key)
  }

  // One page of one per tab whose unread mail no count covers, so the tab can
  // say that something is there. Only for the mailbox on screen: a dot on a
  // tab nobody can see is a request nobody needed.
  function refreshUnread() {
    if (!shown || !account.active || !account.api) return
    var serial = ++dotSerial
    var counted = Provider.unreadQuery(account.providerId)
    for (var i = 0; i < available.length; i++) {
      var tab = available[i]
      if (tab.unreadQuery !== "" && tab.unreadQuery !== counted)
        fetchDot(tab.key, tab.unreadQuery, serial)
    }
  }

  function fetchDot(key, unreadQuery, serial) {
    account.api.listMessages(unreadQuery, 1, "", function(page, error) {
      if (serial !== splits.dotSerial || error || !page) return
      var next = ({})
      for (var known in splits.unreadDots) next[known] = splits.unreadDots[known]
      next[key] = Array.isArray(page.ids) && page.ids.length > 0
      splits.unreadDots = next
    })
  }
}
