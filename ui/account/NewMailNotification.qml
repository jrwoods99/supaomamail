import QtQuick
import Quickshell
import Quickshell.Io
import "Model.js" as Model

// Each account owns its notifications, even while another mailbox is visible.
Item {
  id: root
  visible: false

  required property string notificationForeground
  required property string notificationAccent
  required property string pluginDir
  required property string accountId
  property bool nativeNotifications: false
  property bool pluginNotifications: true
  signal activated(string targetAccountId, string messageId)

  // Each notification owns its waiter and target. A later poll must not replace
  // the message an earlier notification will open.
  Component {
    id: notificationProcessComponent
    Process {
      id: notificationProcess
      property string targetAccountId: ""
      property string messageId: ""
      stdout: StdioCollector {
        onStreamFinished: {
          if (text.trim() === "default")
            root.activated(notificationProcess.targetAccountId,
              notificationProcess.messageId)
        }
      }
      onExited: destroy()
    }
  }

  function notify(arrivals) {
    var list = Array.isArray(arrivals) ? arrivals : []
    if (list.length === 0) return
    var title = Model.notificationTitle(list[0])
    var body = Model.notificationBody(list[0])
    // One notification per batch; clicking reads its newest message.
    if (list.length > 1) {
      var names = []
      for (var i = 0; i < list.length && i < 3; i++) names.push(Model.notificationTitle(list[i]))
      title = Model.pluralize(list.length, "new message")
      body = names.join(", ")
    }
    announce(title, body, String(list[0].id || ""))
  }

  // One notification in the caller's words, opening `messageId` when clicked:
  // mail back from snooze is announced this way, since it is not new.
  function announce(title, body, messageId) {
    var id = String(messageId || "")
    if (nativeNotifications && typeof Quickshell.showNotification === "function") {
      Quickshell.showNotification(root.accountId + ":" + id, title, body, root.accountId, id)
      return
    }
    if (!pluginNotifications) return
    // Sender-authored text stays behind "--", never in options or shell code.
    var request = notificationProcessComponent.createObject(root, {
      targetAccountId: root.accountId,
      messageId: id,
      command: ["python3", root.pluginDir + "/scripts/notify-mail.py",
        root.notificationForeground, root.notificationAccent, "--", String(title), String(body)]
    })
    if (request) request.running = true
  }

}
