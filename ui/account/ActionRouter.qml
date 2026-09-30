import QtQuick
import qs.Commons
import "Model.js" as Model

// What a key, a row's own button, a menu row or a rail stop asks the mailbox to
// do, and where the keyboard goes once it has. The window keeps the state this
// reads and writes -- the cursor, the ticks, the view -- because those are
// facts about the window; this only decides what acting on them means, the way
// RecoveryController decides for a recovered draft.
QtObject {
  id: router
  required property var app
  required property var picker
  required property var snoozer
  readonly property var service: app ? app.service : null
  // Whether the snooze picker was opened on a row outside the ticks, which it
  // then snoozes alone: the label picker's rule.
  property bool snoozeOnlyCursor: false

  // Acting on the selection acts on every ticked row at once, then drops the
  // selection: the rows it named have moved or changed, and a selection that
  // outlived the action would be one keystroke from repeating it.
  function actOnChecked(action) {
    if (!service || app.checkedIds.length === 0) return false
    // Never through another mailbox: a switch that reached the service by
    // any road drops the ticks before a batch can be built from them.
    if (app.checkedAccountId !== String(service.activeAccountId || "")) {
      app.checkedIds = []
      return false
    }
    var ids = app.checkedIds.slice()
    var leaves = !Model.survivesAction(service.mailboxKey, action,
      service.rawQuery, service.hasLabels, service.rawLabelId)
    var next = leaves ? Model.cursorAfterRemovals(service.messages, ids, app.cursorId) : app.cursorId
    // The open message going with the selection closes the reader, the way
    // acting on it alone does: it is about to leave this list.
    var wasOpen = app.currentView === "reader" && ids.indexOf(service.selectedId) >= 0
    if (!service.actMany(ids, action)) return false
    app.checkedIds = []
    if (!leaves) return true
    if (wasOpen) {
      if (next !== "") app.openMessage(next)
      else app.backToList()
      return true
    }
    app.cursorId = next
    app.revealCursorRow()
    return true
  }

  // Opened on the cursor rather than on the selection, the way every other
  // acting key works: `v` in the list means the row under the cursor, and in
  // the reader there is only one message it could mean. Refuse an unavailable
  // move before asking for a destination, through the same provider guard that
  // checks the final action before its optimistic update.
  // Opened on a message outside the ticks, the picker moves that one alone.
  function openLabelPicker(onlyCursor) {
    if (!service || (app.cursorId === "" && !app.selectionActive)) return false
    app.labelPickerOnlyCursor = onlyCursor === true
    // A merged list draws no labels, so there is nothing to offer and the
    // picker would open empty on a destination list it cannot fill — and a
    // chosen id would belong to whichever mailbox happened to be active
    // rather than to the row. Refused where it cannot be honoured, which is
    // the same rule every other unavailable action follows.
    // Only the merged-list refusal belongs here. A single mailbox whose
    // provider has no move verb is the provider guard's answer, and saying
    // "needs one mailbox on screen" over it would name the wrong reason.
    if (service.unified) {
      service.fail("Moving to a label needs one mailbox on screen")
      return false
    }
    if (service.refuseUnavailableAction("label:destination", app.cursorId)) return false
    picker.open()
    return true
  }

  // The same shape as the move picker: on the cursor, or on the ticks, and
  // refused before it asks for a time wherever a snooze could not be kept. A
  // merged list is several mailboxes, and a time is picked for one's messages.
  function openSnoozePicker(onlyCursor) {
    if (!service || (app.cursorId === "" && !app.selectionActive)) return false
    router.snoozeOnlyCursor = onlyCursor === true
    if (service.unified) {
      service.fail("Snoozing needs one mailbox on screen")
      return false
    }
    if (service.refuseUnavailableAction("unsnooze", app.cursorId)) return false
    snoozer.open(router.snoozedUntil())
    return true
  }

  function snoozeTargets() {
    if (app.selectionActive && !router.snoozeOnlyCursor) return app.checkedIds.slice()
    return app.cursorId === "" ? [] : [app.cursorId]
  }

  // When the messages about to be snoozed come back, if every one of them is
  // snoozed already, or 0: the picker then offers to bring them back now.
  function snoozedUntil() {
    var snoozes = service ? service.snoozes : null
    var ids = router.snoozeTargets()
    if (!snoozes || ids.length === 0) return 0
    var first = 0
    for (var i = 0; i < ids.length; i++) {
      var at = snoozes.wakeAt(ids[i])
      if (at <= 0) return 0
      if (first === 0) first = at
    }
    return first
  }

  // The picker's answer, sent the way every other action is: a snooze is
  // optimistic like an archive and put back if the backend refuses it. From
  // here the last snooze is no longer the one Undo takes back, even while
  // this one waits its turn behind another action.
  function snoozeChosen(at) {
    if (service && service.snoozes) service.snoozes.supersede()
    return router.actOnCursor(Model.snoozeAction(at), router.snoozeOnlyCursor)
  }

  function unsnoozeChosen() {
    return router.actOnCursor("unsnooze", router.snoozeOnlyCursor)
  }

  // A row's own button: the selection when the row is ticked, that row alone
  // when it is not — the rule the row menu follows, so a click and a key on
  // the same row cannot mean different sets of messages.
  function actFromRow(id, action) {
    if (!service) return false
    var outside = app.checkedIds.indexOf(id) < 0
    app.cursorId = id
    if (action === "star") {
      if (app.selectionActive && !outside)
        return router.actOnChecked(Model.starActionFor(Model.summariesById(service.messages, app.checkedIds)))
      service.toggleStar(id)
      return true
    }
    return router.actOnCursor(action, outside)
  }

  // The subject the popup names, from the row or the open message.
  function agentSubjectFor(id) {
    if (!service) return ""
    var index = Model.indexById(service.messages, id)
    if (index >= 0) return String(service.messages[index].subject || "")
    if (service.selectedId === id && service.selectedMessage)
      return String(service.selectedMessage.subject || "")
    return ""
  }

  // With rows ticked, the ask is about all of them, one job with a count.
  function openAgentAt(id, sceneX, sceneY) {
    if (!service || !service.hasAgent) return false
    if (app.selectionActive && app.checkedIds.indexOf(String(id || "")) >= 0) {
      app.agentPrompt.openForSelection(app.checkedIds, sceneX, sceneY)
      return true
    }
    if (String(id || "") === "") return false
    app.agentPrompt.openFor(id, router.agentSubjectFor(id), sceneX, sceneY)
    return true
  }

  function openAgentCentered(id) {
    if (!service || !service.hasAgent) return false
    if (app.selectionActive) {
      var centre = app.mapToGlobal(Math.max(0, app.width / 2 - Style.space(190)),
        Math.max(0, app.height / 2 - Style.space(90)))
      app.agentPrompt.openForSelection(app.checkedIds, centre.x, centre.y)
      return true
    }
    if (String(id || "") === "") return false
    app.agentPrompt.openCenteredFor(id, router.agentSubjectFor(id))
    return true
  }

  // Acting on the open message closes it: it is about to leave this list.
  //
  // With rows ticked, the key means all of them rather than the one under the
  // cursor — the same key, the same guard in `MailAccount`, one more row in
  // the request. `onlyCursor` is for the row menu opened on a row outside the
  // selection, which means that row and nothing else.
  function actOnCursor(action, onlyCursor) {
    if (!service) return false
    if (app.selectionActive && onlyCursor !== true) return router.actOnChecked(action)
    if (app.cursorId === "") return false
    var acted = app.cursorId
    var row = service.messages[Model.indexById(service.messages, acted)]
    // "Was open" is the conversation's: with the rail up the reader can be
    // showing a member of the acted row rather than the row itself, and
    // archiving from a member has to open the next row or go back rather than
    // leave a message that has just moved on screen.
    //
    // And a preview is not open at all. It satisfies "is this the selected
    // one" without having been opened, which made `e` on a previewed row call
    // `openMessage` on the *next* one — an archive that reads a message, which
    // is the fault this feature exists to avoid.
    var wasOpen = app.currentView === "reader" && !service.selectionIsPreview
      && (service.selectedId === acted || Model.rowHoldsMember(row, service.selectedId))
    // Worked out before the action, while the row still has neighbours.
    var next = Model.cursorAfterRemoval(service.messages, acted)
    // The same six facts `MailAccount.act` decides with. Asking with three of
    // them made the cursor repair disagree with the list it repairs: moving a
    // message back to the inbox removes the row on a provider that moves, and
    // this read it as staying. The row itself is the sixth: a conversation
    // answers on its recomputed block, so a mark-read in the Unread view keeps
    // the row while a reply is still unread.
    var leaves = !Model.survivesAction(service.mailboxKey, action,
      service.rawQuery, service.hasLabels, service.rawLabelId, row)
    if (!service.act(acted, action)) return false
    if (!leaves) return true
    // The row is going and the cursor must not go with it: a cursor on a
    // message that is no longer listed cannot be found, so the next j restarts
    // at the top. Archiving one message used to send it back to the first row.
    if (wasOpen) {
      if (next !== "") app.openMessage(next)
      else app.backToList()
      return true
    }
    app.cursorId = next
    app.revealCursorRow()
    return true
  }

  // Acting on one member from its stop on the rail: the one message and not
  // the conversation, whichever member it is. If it was the message on screen
  // and the action takes it out of this view, the reader moves to the
  // neighbouring stop — the newer one above, else the older below — rather
  // than sitting on a message that has just left; a conversation with no other
  // stop goes back to the list, as the list's own delete does.
  function actOnMember(action, id) {
    var member = String(id || "")
    if (!service || member === "") return false
    var wasOpen = app.currentView === "reader" && service.selectedId === member
    // Worked out before the action, while the member is still a stop.
    var projection = service.conversationProjection || ({})
    var stop = (projection.navigation || ({}))[member]
    var next = stop ? String(stop.neighbor || "") : ""
    var members = service.memberSummaries
    var summary = members && typeof members === "object" ? members[member] : null
    var leaves = !Model.survivesAction(service.mailboxKey, action,
      service.rawQuery, service.hasLabels, service.rawLabelId, summary || null)
    if (!service.act(member, action, false, true)) return false
    if (!leaves || !wasOpen) return true
    if (next !== "") app.openMember(next)
    else app.backToList()
    return true
  }
}
