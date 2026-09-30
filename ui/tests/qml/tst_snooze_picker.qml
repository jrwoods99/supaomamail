import QtQuick 2.15
import QtTest 1.3
import "../../components" as Components
import "../../snooze/When.js" as When

// Snoozing asks for a time, typed: the answers arrive best first as the
// letters do, the arrows walk them, Return takes one and Escape takes none. A
// click only places the cursor, as in the move picker.
Item {
  id: host
  width: 640
  height: 480

  property var chosen: []
  property int unsnoozed: 0

  Components.SnoozePicker {
    id: picker
    anchors.fill: parent
    textColor: Qt.rgba(0.1, 0.1, 0.1, 1)
    accentColor: Qt.rgba(0.2, 0.4, 0.8, 1)
    dimColor: Qt.rgba(0.4, 0.4, 0.4, 1)
    popupBackgroundColor: Qt.rgba(0.95, 0.95, 0.95, 1)
    popupBorderColor: Qt.rgba(0.2, 0.2, 0.2, 1)
    panelFontFamily: "sans"
    onTimeChosen: function(at) { host.chosen = host.chosen.concat([at]) }
    onUnsnoozeChosen: host.unsnoozed += 1
  }

  TestCase {
    name: "SnoozePicker"
    when: windowShown

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

    function openWith(until) {
      if (picker.opened) keyClick(Qt.Key_Escape)
      host.chosen = []
      host.unsnoozed = 0
      picker.open(until)
      tryVerify(function() { return picker.opened })
      wait(60)
    }

    // By key, the way the rest of the suite types: letters arrive lower case.
    function type(text) {
      for (var i = 0; i < text.length; i++) {
        var code = text.charCodeAt(i)
        if (code >= 97 && code <= 122) keyClick(Qt.Key_A + code - 97)
        else if (code >= 48 && code <= 57) keyClick(Qt.Key_0 + code - 48)
        else if (text.charAt(i) === " ") keyClick(Qt.Key_Space)
        else fail("type() has no key for " + text.charAt(i))
      }
    }

    function labels() {
      return picker.rows.map(function(row) { return row.label })
    }

    function cleanup() {
      if (picker.opened) keyClick(Qt.Key_Escape)
    }

    // Which of the usual times are on offer depends on the day and the hour --
    // on a Sunday "next week" is tomorrow morning and is offered once -- so the
    // list is compared with When.js's own answer for the moment it opened.
    function test_an_empty_field_offers_the_usual_times() {
      openWith(0)
      compare(labels(), When.defaults(picker.now).map(function(row) { return row.label }))
      verify(labels().indexOf("Tomorrow") >= 0, "tomorrow morning is always ahead: " + labels().join(", "))
      compare(named(picker.Window.window.contentItem, "snooze-picker-title").text, "Snooze until")
    }

    function test_2m_is_minutes_then_months_then_mondays_and_return_takes_the_first() {
      openWith(0)
      type("2m")
      compare(labels().slice(0, 3), ["In 2 minutes", "In 2 months", "In 2 Mondays"])
      keyClick(Qt.Key_Return)
      tryCompare(picker, "opened", false)
      compare(host.chosen.length, 1)
      compare(host.chosen[0], picker.now + 2 * 60000, "two minutes after the key was typed")
    }

    function test_tom_3_is_the_afternoon_first_and_the_arrows_wrap() {
      openWith(0)
      type("tom 3")
      compare(labels(), ["Tomorrow, 15:00", "Tomorrow, 03:00"])
      keyClick(Qt.Key_Down)
      compare(picker.cursorIndex, 1)
      keyClick(Qt.Key_Down)
      compare(picker.cursorIndex, 0, "past the last answer is the first")
      keyClick(Qt.Key_Up)
      compare(picker.cursorIndex, 1)
      keyClick(Qt.Key_Return)
      tryCompare(picker, "opened", false)
      var early = new Date(picker.now)
      early.setDate(early.getDate() + 1)
      early.setHours(3, 0, 0, 0)
      compare(host.chosen[0], early.getTime())
    }

    function test_typing_puts_the_cursor_back_on_the_best_answer() {
      openWith(0)
      type("2")
      keyClick(Qt.Key_Down)
      verify(picker.cursorIndex > 0)
      type("d")
      compare(picker.cursorIndex, 0)
      compare(labels()[0], "In 2 days")
    }

    function test_mail_already_snoozed_can_be_brought_back_now() {
      openWith(Date.now() + 3 * 3600000)
      compare(labels()[0], "Unsnooze now")
      verify(named(picker.Window.window.contentItem, "snooze-picker-title").text.indexOf("Snoozed until ") === 0)
      keyClick(Qt.Key_Return)
      tryCompare(picker, "opened", false)
      compare(host.unsnoozed, 1)
      compare(host.chosen.length, 0)
    }

    function test_nonsense_offers_nothing_and_return_takes_nothing() {
      openWith(0)
      type("asdf")
      compare(picker.rows.length, 0)
      keyClick(Qt.Key_Return)
      verify(picker.opened, "nothing to take keeps the field open")
      compare(host.chosen.length, 0)
    }

    function test_escape_closes_without_choosing() {
      openWith(0)
      type("2h")
      keyClick(Qt.Key_Escape)
      tryCompare(picker, "opened", false)
      compare(host.chosen.length, 0)
    }

    function test_a_single_click_only_moves_the_cursor() {
      openWith(0)
      // Five answers at any hour; late on a Sunday the empty field has one.
      type("2")
      var second = named(picker.Window.window.contentItem, "snooze-picker-row-1")
      verify(second)
      mouseClick(second)
      compare(picker.cursorIndex, 1)
      compare(host.chosen.length, 0)
      verify(picker.opened)
    }

    function test_an_answer_gone_by_is_worked_out_again_rather_than_taken() {
      openWith(0)
      type("2m")
      // As if the popup had been left open for ten minutes.
      picker.now = picker.now - 10 * 60000
      compare(picker.rows[0].label, "In 2 minutes")
      keyClick(Qt.Key_Return)
      verify(picker.opened)
      compare(host.chosen.length, 0)
      verify(picker.rows[0].at > Date.now(), "the answers are read from the clock again")
    }
  }
}
