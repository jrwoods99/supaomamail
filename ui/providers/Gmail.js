.pragma library

// Provider presentation only. NativeDomain.js supplies all mail capabilities
// and mailbox queries; providers.resolve builds dynamic queries in Rust.

var ID = "gmail"

var NAME = "Gmail"

var SUMMARY = "Google's own API. Needs an OAuth client you create once."

var AUTH = "oauth"

var MARK = "gmail.png"

// The Inbox can be drawn as two tabs, the way Gmail's own Primary tab keeps
// Promotions, Social and Forums out of the way. What each tab lists is a native
// query (`inbox:primary`, `inbox:other`); this only names them.
var MAILBOXES = [
  {
    "key": "inbox",
    "label": "Inbox",
    "icon": "inbox",
    "splits": [
      { "key": "primary", "label": "Primary" },
      { "key": "other", "label": "Other" }
    ]
  },
  {
    "key": "unread",
    "label": "Unread",
    "icon": "unread"
  },
  {
    "key": "starred",
    "label": "Starred",
    "icon": "star"
  },
  {
    "key": "drafts",
    "label": "Drafts",
    "icon": "compose"
  },
  {
    "key": "sent",
    "label": "Sent",
    "icon": "sent"
  },
  {
    "key": "all",
    "label": "All mail",
    "icon": "archive",
    "optional": true
  },
  {
    "key": "spam",
    "label": "Spam",
    "icon": "spam",
    "optional": true
  },
  {
    "key": "trash",
    "label": "Trash",
    "icon": "trash",
    "optional": true
  },
  // What is snoozed, and when each comes back. Last, so every mailbox above
  // keeps the Ctrl digit it had; only a backend that keeps snoozes lists it.
  {
    "key": "snoozed",
    "label": "Snoozed",
    "icon": "snooze",
    "optional": true,
    "minimumApiVersion": 6
  }
]
