import QtQuick
import qs.Commons
import qs.Ui
import "../snooze/When.js" as When

// What the last snooze sent away and until when, with the way to take it back,
// for the few seconds after it lands; later, the Snoozed mailbox and "Unsnooze
// now" in the picker are the way. Undo brings the messages back to the Inbox,
// read or unread as they were.
Rectangle {
  id: root

  // { ids, at } from the visible mailbox's snoozes, or null.
  property var snooze: null
  required property color textColor
  required property color accentColor
  required property color popupBackgroundColor
  required property color popupBorderColor
  required property string panelFontFamily

  signal undoRequested()

  readonly property string message: {
    if (!root.snooze) return ""
    var count = Array.isArray(root.snooze.ids) ? root.snooze.ids.length : 0
    var when = When.describe(Number(root.snooze.at) || 0, Date.now())
    return (count > 1 ? count + " messages snoozed until " : "Snoozed until ") + when
  }

  implicitWidth: content.implicitWidth + Style.space(16)
  implicitHeight: Math.max(content.implicitHeight + Style.space(12), Style.space(42))
  radius: Style.cornerRadius
  color: Qt.rgba(popupBackgroundColor.r, popupBackgroundColor.g,
    popupBackgroundColor.b, 1)
  border.width: Style.normalBorderWidth
  border.color: popupBorderColor

  Row {
    id: content
    anchors.centerIn: parent
    spacing: Style.space(12)

    Text {
      objectName: "snoozed-message"
      anchors.verticalCenter: parent.verticalCenter
      text: root.message
      textFormat: Text.PlainText
      color: root.textColor
      font.family: root.panelFontFamily
      font.pixelSize: Style.font.bodySmall
    }

    Button {
      objectName: "snoozed-undo-button"
      anchors.verticalCenter: parent.verticalCenter
      text: "Undo  alt+z"
      foreground: root.accentColor
      accent: root.accentColor
      bordered: true
      fontFamily: root.panelFontFamily
      fontSize: Style.font.bodySmall
      onClicked: root.undoRequested()
    }
  }
}
