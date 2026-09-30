import QtQuick
import QtQuick.Controls as QQC
import qs.Commons
import qs.Ui
import "../account/Model.js" as Model
import "../snooze/When.js" as When

// When a message comes back, typed rather than picked off a calendar.
//
// "2m" is already three answers -- two minutes, two months, two Mondays -- and
// they are offered in that order as the letters arrive; each further letter
// narrows them. The arrows walk the answers and Return takes one, the way the
// move picker works, and with nothing typed the field offers the handful of
// times people reach for. On messages already snoozed the first row brings
// them back now instead.
Item {
  id: root

  required property color textColor
  required property color accentColor
  required property color dimColor
  required property color popupBackgroundColor
  required property color popupBorderColor
  required property string panelFontFamily

  // When the messages being snoozed come back already, or 0.
  property double snoozedUntil: 0
  property string searchQuery: ""
  // Read when the popup opens and again on every keystroke, so the answers
  // hold still while they are being read.
  property double now: Date.now()

  readonly property bool opened: menu.opened
  readonly property var predictions: When.predict(root.searchQuery, root.now,
    { morningHour: 8, limit: 6 })
  readonly property var rows: root.snoozedUntil > 0
    ? [{ unsnooze: true, label: "Unsnooze now", detail: "Back in the Inbox" }].concat(root.predictions)
    : root.predictions

  // Reset to the top on every keystroke, because the answers underneath it
  // have changed.
  property int cursorIndex: 0

  signal timeChosen(double at)
  signal unsnoozeChosen()

  anchors.fill: parent
  z: 50

  function open(until) {
    root.snoozedUntil = Number(until) || 0
    searchField.reset()
    root.searchQuery = ""
    root.now = Date.now()
    root.cursorIndex = 0
    menu.open()
    place()
    searchField.takeFocus()
  }

  function close() {
    menu.close()
  }

  // Placed again on every height change, like the move picker: a Popup has no
  // height until it is first built, and typing shortens the list.
  function place() {
    if (!menu.visible) return
    var menuHeight = menu.height > 0 ? menu.height : menu.implicitHeight
    menu.x = Math.max(Style.space(8), (root.width - menu.width) / 2)
    menu.y = Math.max(Style.space(8), (root.height - menuHeight) / 3)
  }

  function moveCursor(delta) {
    if (root.rows.length === 0) return
    cursorIndex = Model.wrappedIndex(cursorIndex, delta, root.rows.length)
  }

  function chooseCursor() {
    if (root.cursorIndex < 0 || root.cursorIndex >= root.rows.length) return
    var chosen = root.rows[root.cursorIndex]
    // A popup left open can outlive its own answers: "in 2 minutes" read ten
    // minutes ago has gone by. The answers are worked out again instead.
    if (!chosen.unsnooze && !When.acceptable(chosen.at, Date.now())) {
      root.now = Date.now()
      root.cursorIndex = 0
      return
    }
    menu.close()
    if (chosen.unsnooze) root.unsnoozeChosen()
    else root.timeChosen(chosen.at)
  }

  QQC.Popup {
    id: menu
    width: Math.min(Style.space(360), root.width - Style.space(24))
    implicitHeight: card.implicitHeight + Style.space(16)
    padding: Style.space(8)
    modal: false
    focus: true
    closePolicy: QQC.Popup.CloseOnEscape | QQC.Popup.CloseOnPressOutside
    onHeightChanged: root.place()
    onOpened: root.place()

    background: Rectangle {
      radius: Style.cornerRadius
      color: root.popupBackgroundColor
      border.width: 1
      border.color: root.popupBorderColor
    }

    contentItem: Column {
      id: card
      spacing: Style.space(8)

      Text {
        objectName: "snooze-picker-title"
        width: parent.width
        textFormat: Text.PlainText
        text: root.snoozedUntil > 0
          ? "Snoozed until " + When.describe(root.snoozedUntil, root.now)
          : "Snooze until"
        color: root.textColor
        font.family: root.panelFontFamily
        font.pixelSize: Style.font.bodySmall
        font.bold: true
        elide: Text.ElideRight
      }

      // The field answers its own keys: an open Popup takes every key before
      // the shortcut map sees it. Digits are letters here, since "2m" starts
      // with one, which is why no row carries a number.
      SwitcherSearch {
        id: searchField
        objectName: "snooze-picker-field"
        width: parent.width
        foreground: root.textColor
        accent: root.accentColor
        font.family: root.panelFontFamily
        font.pixelSize: Style.font.bodySmall
        placeholderText: "2m, tom 3, fri 9am, oct 5..."
        onTextChanged: {
          root.searchQuery = text
          root.now = Date.now()
          root.cursorIndex = 0
        }
        onMoved: function(delta) { root.moveCursor(delta) }
        onChosen: root.chooseCursor()
      }

      ListView {
        id: answerList

        width: parent.width
        implicitHeight: Math.min(contentHeight, Style.space(280))
        clip: true
        interactive: false
        model: root.rows
        currentIndex: root.cursorIndex
        highlightMoveDuration: 0

        delegate: Rectangle {
          id: answerRow
          required property var modelData
          required property int index
          objectName: "snooze-picker-row-" + index
          readonly property bool hasCursor: root.cursorIndex === answerRow.index

          width: answerList.width
          implicitHeight: Style.space(34)
          radius: Style.cornerRadius
          color: answerRow.hasCursor || rowHover.hovered
            ? Style.hoverFillFor(root.textColor, root.accentColor) : "transparent"
          border.width: answerRow.hasCursor ? Style.normalBorderWidth : 0
          border.color: Style.hoverBorderFor(root.textColor, root.accentColor)

          Text {
            id: answerLabel
            anchors.left: parent.left
            anchors.leftMargin: Style.space(10)
            anchors.right: answerDetail.left
            anchors.rightMargin: Style.space(8)
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: String(answerRow.modelData.label || "")
            color: root.textColor
            font.family: root.panelFontFamily
            font.pixelSize: Style.font.bodySmall
            elide: Text.ElideRight
          }

          Text {
            id: answerDetail
            anchors.right: parent.right
            anchors.rightMargin: Style.space(10)
            anchors.verticalCenter: parent.verticalCenter
            textFormat: Text.PlainText
            text: String(answerRow.modelData.detail || "")
            color: root.dimColor
            font.family: root.panelFontFamily
            font.pixelSize: Style.font.caption
          }

          HoverHandler { id: rowHover }

          // As in the move picker: a click places the cursor, and taking the
          // answer is a second gesture, Return or a double-click.
          MouseArea {
            anchors.fill: parent
            onClicked: root.cursorIndex = answerRow.index
            onDoubleClicked: {
              root.cursorIndex = answerRow.index
              root.chooseCursor()
            }
          }
        }
      }

      Text {
        width: parent.width
        visible: root.rows.length === 0
        textFormat: Text.PlainText
        text: "No time matches that"
        color: root.dimColor
        font.family: root.panelFontFamily
        font.pixelSize: Style.font.caption
      }
    }
  }
}
