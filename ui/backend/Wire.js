.pragma library

function request(id, method, params) {
  return JSON.stringify({ jsonrpc: "2.0", id: id, method: method, params: params }) + "\n"
}

function response(line) {
  var value
  try { value = JSON.parse(line) } catch (error) { return null }
  return responseValue(value)
}

function responseValue(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)
      || value.jsonrpc !== "2.0" || typeof value.id !== "string") return null
  var success = Object.prototype.hasOwnProperty.call(value, "result")
  var failure = Object.prototype.hasOwnProperty.call(value, "error")
  if (success === failure) return null
  if (failure && (!value.error || typeof value.error.code !== "number"
      || typeof value.error.message !== "string")) return null
  return value
}

// Only the backend's declared event channel may bypass request correlation.
function notification(line) {
  var value
  try { value = JSON.parse(line) } catch (error) { return null }
  return notificationValue(value)
}

function notificationValue(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)
      || value.jsonrpc !== "2.0" || ["mail.updated", "accounts.changed", "outbox.changed", "gmail.settled", "snooze.changed"].indexOf(value.method) < 0
      || Object.prototype.hasOwnProperty.call(value, "id")
      || !value.params || typeof value.params !== "object" || Array.isArray(value.params)
      || Object.keys(value).some(function(key) { return ["jsonrpc", "method", "params"].indexOf(key) < 0 })) return null
  if (value.method === "accounts.changed"
      && (Object.keys(value.params).length !== 1 || typeof value.params.revision !== "string"
          || !/^[a-f0-9]{64}$/.test(value.params.revision))) return null
  if (value.method === "outbox.changed"
      && (typeof value.params.accountId !== "string" || !Array.isArray(value.params.entries))) return null
  if (value.method === "gmail.settled"
      && (typeof value.params.accountId !== "string" || typeof value.params.ticket !== "string"
          || typeof value.params.ok !== "boolean" || typeof value.params.error !== "string")) return null
  if (value.method === "snooze.changed" && !snoozeChanged(value.params)) return null
  return value
}

// What woke carries a sender's subject and name, which are plain text to be
// shown and nothing else; anything shaped otherwise is not this event.
function snoozeChanged(params) {
  if (typeof params.accountId !== "string" || typeof params.revision !== "string"
      || !Array.isArray(params.woken) || params.woken.length > 1000) return false
  for (var i = 0; i < params.woken.length; i++) {
    var woken = params.woken[i]
    if (!woken || typeof woken !== "object" || typeof woken.messageId !== "string"
        || typeof woken.subject !== "string" || typeof woken.from !== "string") return false
  }
  return true
}
