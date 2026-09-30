.pragma library
.import "../message/Message.js" as Mail

// When a snooze should end, read from a few typed characters.
//
// The snooze popup asks after every keystroke, so this answers a prefix as
// well as a finished phrase: "2m" is already three answers -- minutes, then
// months, then Mondays -- in the order somebody typing it most likely means,
// and each further letter narrows them. "tom 3" is tomorrow at three, and a
// bare three is a working-hours three first: 15:00, then 03:00.
//
// Nothing here reads the clock. The caller passes `now`, which is what lets
// the node tests pin one Wednesday morning and check every answer after it.
// Times are drawn on the window's own 24-hour clock, like every other time it
// shows; the answers are absolute instants, so the backend only ever compares
// milliseconds and never needs to know what a local Monday is.

var MINUTE = 60000
var HOUR = 60 * MINUTE
var DAY_MS = 24 * HOUR

// An answer this close to now would wake before the snooze had settled, and
// one this far out is past what the backend keeps.
var MIN_LEAD = 30000
var MAX_AHEAD = 5 * 366 * DAY_MS

var DEFAULT_MORNING = 8
var DEFAULT_LIMIT = 6

// Words that carry no time of their own: "in 2 hours", "tom at 3", "on fri",
// "this weekend". Dropping them is what lets a phrase be typed either way.
var FILLER = ["in", "at", "on", "for", "until", "till", "by", "this"]

var FULL_DAYS = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"]

// Ranked. When one abbreviation starts several unit names, this order decides
// which is offered first, so "2m" is minutes, then months, then Mondays.
var UNITS = [
  { key: "minute", names: ["minutes", "minute", "mins", "min"], one: "minute", many: "minutes" },
  { key: "hour", names: ["hours", "hour", "hrs", "hr"], one: "hour", many: "hours" },
  { key: "day", names: ["days", "day"], one: "day", many: "days" },
  { key: "week", names: ["weeks", "week", "wks", "wk"], one: "week", many: "weeks" },
  { key: "month", names: ["months", "month", "mos", "mo"], one: "month", many: "months" },
  { key: "year", names: ["years", "year", "yrs", "yr"], one: "year", many: "years" },
  { key: "weekday", weekday: 1, names: ["mondays", "monday", "mon"], one: "Monday", many: "Mondays" },
  { key: "weekday", weekday: 2, names: ["tuesdays", "tuesday", "tues", "tue"], one: "Tuesday", many: "Tuesdays" },
  { key: "weekday", weekday: 3, names: ["wednesdays", "wednesday", "wed"], one: "Wednesday", many: "Wednesdays" },
  { key: "weekday", weekday: 4, names: ["thursdays", "thursday", "thurs", "thur", "thu"], one: "Thursday", many: "Thursdays" },
  { key: "weekday", weekday: 5, names: ["fridays", "friday", "fri"], one: "Friday", many: "Fridays" },
  { key: "weekday", weekday: 6, names: ["saturdays", "saturday", "sat"], one: "Saturday", many: "Saturdays" },
  { key: "weekday", weekday: 0, names: ["sundays", "sunday", "sun"], one: "Sunday", many: "Sundays" }
]

// Every word that names a day or a time of day on its own, ranked the same
// way: a single "t" offers today, tonight, tomorrow, then Tuesday and
// Thursday, because the near answers are the likely ones.
var WORDS = [
  { type: "day", day: "today", names: ["today", "tod"] },
  { type: "timeword", hour: 20, names: ["tonight", "tonite"] },
  { type: "day", day: "tomorrow", names: ["tomorrow", "tom", "tmr", "tmrw", "tmw"] },
  { type: "day", day: "weekend", names: ["weekend", "wknd"] },
  { type: "special", key: "later", names: ["later"] },
  { type: "day", day: "weekday", weekday: 1, names: ["monday", "mon"] },
  { type: "day", day: "weekday", weekday: 2, names: ["tuesday", "tues", "tue"] },
  { type: "day", day: "weekday", weekday: 3, names: ["wednesday", "wed"] },
  { type: "day", day: "weekday", weekday: 4, names: ["thursday", "thurs", "thur", "thu"] },
  { type: "day", day: "weekday", weekday: 5, names: ["friday", "fri"] },
  { type: "day", day: "weekday", weekday: 6, names: ["saturday", "sat"] },
  { type: "day", day: "weekday", weekday: 0, names: ["sunday", "sun"] },
  { type: "timeword", hour: 8, names: ["morning", "morn"] },
  { type: "timeword", hour: 12, names: ["noon", "midday"] },
  { type: "timeword", hour: 13, names: ["afternoon", "aft"] },
  { type: "timeword", hour: 17, names: ["eod"] },
  { type: "timeword", hour: 18, names: ["evening", "eve"] },
  { type: "timeword", hour: 20, names: ["night"] },
  { type: "timeword", hour: 0, names: ["midnight"] },
  { type: "special", key: "eow", names: ["eow"] }
]

var MONTH_NAMES = ["january", "february", "march", "april", "may", "june", "july",
  "august", "september", "october", "november", "december"]

// ------------------------------------------------------------------ helpers

function startsWith(name, prefix) {
  return prefix !== "" && name.substring(0, prefix.length) === prefix
}

function anyStartsWith(names, prefix) {
  for (var i = 0; i < names.length; i++) {
    if (startsWith(names[i], prefix)) return true
  }
  return false
}

function unitsFor(text) {
  var out = []
  for (var i = 0; i < UNITS.length; i++) {
    if (anyStartsWith(UNITS[i].names, text)) out.push(UNITS[i])
  }
  return out
}

function wordsFor(text) {
  var out = []
  for (var i = 0; i < WORDS.length; i++) {
    if (anyStartsWith(WORDS[i].names, text)) out.push(WORDS[i])
  }
  return out
}

// Three letters before a month is guessed: "ma" could be March or May, and a
// single letter would shadow every day word that shares it.
function monthFor(text) {
  if (text.length < 3) return -1
  for (var i = 0; i < MONTH_NAMES.length; i++) {
    if (startsWith(MONTH_NAMES[i], text)) return i
  }
  if (text === "sept") return 8
  return -1
}

function isNextWord(text) {
  return text === "nxt" || (text.length >= 2 && startsWith("next", text))
}

function meridiemOf(text) {
  if (text === "am" || text === "a") return "am"
  if (text === "pm" || text === "p") return "pm"
  return ""
}

function localDay(date) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate())
}

function at(day, hour, minute) {
  return new Date(day.getFullYear(), day.getMonth(), day.getDate(), hour, minute || 0).getTime()
}

function addDays(day, count) {
  return new Date(day.getFullYear(), day.getMonth(), day.getDate() + count)
}

function daysInMonth(year, month) {
  return new Date(year, month + 1, 0).getDate()
}

// Calendar months, clamped: a month from January 31 is the last of February,
// not the third of March.
function addMonths(day, count) {
  var first = new Date(day.getFullYear(), day.getMonth() + count, 1)
  var date = Math.min(day.getDate(), daysInMonth(first.getFullYear(), first.getMonth()))
  return new Date(first.getFullYear(), first.getMonth(), date)
}

// Monday is day one of a week, as it is in the calendar here.
function isoDay(date) {
  var day = date.getDay()
  return day === 0 ? 7 : day
}

// The next such weekday strictly after today: "fri" on a Friday is next week.
function upcomingWeekday(today, weekday) {
  var ahead = (weekday - today.getDay() + 7) % 7
  return addDays(today, ahead === 0 ? 7 : ahead)
}

function nextWeekMonday(today) {
  return addDays(today, 8 - isoDay(today))
}

// "next friday" is the Friday of next week; the Friday coming up is offered
// second, because English does not settle which of the two is meant.
function nextWeekWeekday(today, weekday) {
  var iso = weekday === 0 ? 7 : weekday
  return addDays(nextWeekMonday(today), iso - 1)
}

// Three hours on, rounded up to the local hour: 10:15 is later at 14:00.
function later(now) {
  var target = new Date(now.getTime() + 3 * HOUR)
  if (target.getMinutes() === 0 && target.getSeconds() === 0 && target.getMilliseconds() === 0)
    return target.getTime()
  target.setMinutes(0, 0, 0)
  return target.getTime() + HOUR
}

// ---------------------------------------------------------------- reading

function clockToken(hour, minute, meridiem) {
  return { kind: "clock", hour: hour, minute: minute, meridiem: meridiem }
}

function digitsClock(digits, meridiem) {
  var value = Number(digits)
  if (digits.length <= 2) return clockToken(value, 0, meridiem)
  return clockToken(Math.floor(value / 100), value % 100, meridiem)
}

// One pass over the raw words. Anything that is not a number, a clock, a date
// or a plain word makes the whole phrase unreadable, and the popup then says
// so rather than guessing around it.
function tokenize(text) {
  var source = String(text || "").toLowerCase().replace(/[,;]+/g, " ").replace(/\s+/g, " ").trim()
  var out = []
  if (source === "") return out
  var raw = source.split(" ")
  for (var i = 0; i < raw.length; i++) {
    var piece = raw[i]
    var match
    if ((match = /^(\d{4})-(\d{1,2})-(\d{1,2})$/.exec(piece))) {
      out.push({ kind: "date", year: Number(match[1]), month: Number(match[2]) - 1, day: Number(match[3]) })
    } else if ((match = /^(\d{1,2})\/(\d{1,2})(?:\/(\d{2}|\d{4}))?$/.exec(piece))) {
      var year = match[3] ? Number(match[3]) : -1
      if (year >= 0 && year < 100) year += 2000
      out.push({ kind: "date", year: year, month: Number(match[1]) - 1, day: Number(match[2]) })
    } else if ((match = /^(\d{1,2}):(\d{0,2})(am|pm|a|p)?$/.exec(piece))) {
      // A minute half typed counts as its tens: "12:3" is on the way to 12:30.
      var minutes = match[2] === "" ? 0 : match[2].length === 1 ? Number(match[2]) * 10 : Number(match[2])
      out.push(clockToken(Number(match[1]), minutes, meridiemOf(match[3] || "")))
    } else if ((match = /^(\d{1,4})(am|pm|a|p)$/.exec(piece))) {
      out.push(digitsClock(match[1], meridiemOf(match[2])))
    } else if ((match = /^(\d{1,2})(st|nd|rd|th)$/.exec(piece))) {
      out.push({ kind: "ordinal", day: Number(match[1]) })
    } else if ((match = /^(\d{1,4})([a-z]+)$/.exec(piece))) {
      out.push({ kind: "num", value: Number(match[1]), digits: match[1] })
      out.push({ kind: "word", text: match[2] })
    } else if (/^\d{1,4}$/.test(piece)) {
      out.push({ kind: "num", value: Number(piece), digits: piece })
    } else if (/^[a-z]+$/.test(piece)) {
      if (FILLER.indexOf(piece) < 0) out.push({ kind: "word", text: piece })
    } else {
      return null
    }
  }
  return out
}

// "3 pm" is one clock, and "a week" is one week.
function joined(tokens) {
  var out = []
  for (var i = 0; i < tokens.length; i++) {
    var token = tokens[i]
    var next = tokens[i + 1]
    var clockLike = token.kind === "num" || (token.kind === "clock" && token.meridiem === "")
    if (clockLike && next && next.kind === "word" && meridiemOf(next.text) !== "") {
      out.push(token.kind === "num" ? digitsClock(token.digits, meridiemOf(next.text))
        : clockToken(token.hour, token.minute, meridiemOf(next.text)))
      i++
    } else if (token.kind === "word" && (token.text === "a" || token.text === "an")
        && next && next.kind === "word" && unitsFor(next.text).length > 0) {
      out.push({ kind: "num", value: 1, digits: "1" })
    } else {
      out.push(token)
    }
  }
  return out
}

// The hour a clock means, in the order it is most likely meant. A bare hour
// from one to six is an afternoon before it is a small hour, seven to eleven
// a morning, and twelve is noon before it is midnight.
function clockAlternatives(hour, minute, meridiem) {
  if (minute < 0 || minute > 59 || hour < 0 || hour > 23) return []
  if (meridiem !== "") {
    if (hour < 1 || hour > 12) return []
    var base = hour % 12
    return [{ type: "time", hour: meridiem === "pm" ? base + 12 : base, minute: minute }]
  }
  if (hour === 0 || hour > 12) return [{ type: "time", hour: hour, minute: minute }]
  if (hour === 12) return [{ type: "time", hour: 12, minute: minute }, { type: "time", hour: 0, minute: minute }]
  var pm = { type: "time", hour: hour + 12, minute: minute }
  var am = { type: "time", hour: hour, minute: minute }
  return hour <= 6 ? [pm, am] : [am, pm]
}

// A number with nothing after it is a time first -- that is what "tom 3"
// needs it to be -- and then the start of a duration still being typed.
function numberAlternatives(token) {
  if (token.digits.length >= 3) {
    return clockAlternatives(Math.floor(token.value / 100), token.value % 100, "")
  }
  var n = token.value
  var out = n <= 23 ? clockAlternatives(n, 0, "") : []
  if (n >= 10) out.push({ type: "offset", unit: UNITS[0], n: n })
  if (n >= 1 && n <= 48) out.push({ type: "offset", unit: UNITS[1], n: n })
  if (n >= 1 && n <= 90) out.push({ type: "offset", unit: UNITS[2], n: n })
  if (n >= 1 && n <= 52) out.push({ type: "offset", unit: UNITS[3], n: n })
  return out
}

function dayOfMonthToken(token) {
  if (!token) return -1
  if (token.kind === "ordinal") return token.day
  if (token.kind === "num" && token.digits.length <= 2) return token.value
  return -1
}

// Each piece of the phrase becomes one component holding every reading of
// it, best first. Returns null when a piece cannot be read at all.
function components(tokens) {
  var out = []
  for (var i = 0; i < tokens.length; i++) {
    var token = tokens[i]
    var next = tokens[i + 1]
    if (token.kind === "num") {
      var units = next && next.kind === "word" ? unitsFor(next.text) : []
      if (units.length > 0 && token.value >= 1 && token.value <= 999) {
        var offsets = []
        for (var u = 0; u < units.length; u++) offsets.push({ type: "offset", unit: units[u], n: token.value })
        out.push(offsets)
        i++
        continue
      }
      if (next && next.kind === "word" && monthFor(next.text) >= 0 && token.digits.length <= 2) {
        out.push([{ type: "day", day: "date", year: -1, month: monthFor(next.text), date: token.value }])
        i++
        continue
      }
      var readings = numberAlternatives(token)
      if (readings.length === 0) return null
      out.push(readings)
    } else if (token.kind === "clock") {
      var clocks = clockAlternatives(token.hour, token.minute, token.meridiem)
      if (clocks.length === 0) return null
      out.push(clocks)
    } else if (token.kind === "date") {
      out.push([{ type: "day", day: "date", year: token.year, month: token.month, date: token.day }])
    } else if (token.kind === "ordinal") {
      if (next && next.kind === "word" && monthFor(next.text) >= 0) {
        out.push([{ type: "day", day: "date", year: -1, month: monthFor(next.text), date: token.day }])
        i++
      } else {
        out.push([{ type: "day", day: "ordinal", date: token.day }])
      }
    } else if (isNextWord(token.text)) {
      out.push(nextReadings(next && next.kind === "word" ? next.text : ""))
      if (next && next.kind === "word") i++
    } else if (monthFor(token.text) >= 0 && dayOfMonthToken(next) >= 1) {
      out.push([{ type: "day", day: "date", year: -1, month: monthFor(token.text), date: dayOfMonthToken(next) }])
      i++
    } else {
      var words = wordsFor(token.text)
      var readingsOfWord = []
      for (var w = 0; w < words.length; w++) readingsOfWord.push(words[w])
      if (monthFor(token.text) >= 0)
        readingsOfWord.push({ type: "day", day: "date", year: -1, month: monthFor(token.text), date: 1 })
      if (readingsOfWord.length === 0) return null
      out.push(readingsOfWord)
    }
  }
  return out
}

// What follows "next". A word typed out in full, or a weekday's own
// abbreviation, comes before the words it is merely the start of: "next mon"
// is Monday before it is the month it also begins.
function nextReadings(text) {
  if (text === "") return [{ type: "day", day: "nextweek" }, { type: "day", day: "nextmonth" }]
  var exact = []
  var partial = []
  function add(isExact, reading) {
    if (isExact) exact.push(reading)
    else partial.push(reading)
  }
  if (startsWith("week", text)) add(text === "week", { type: "day", day: "nextweek" })
  if (startsWith("month", text)) add(text === "month", { type: "day", day: "nextmonth" })
  if (startsWith("weekend", text) || text === "wknd")
    add(text === "weekend" || text === "wknd", { type: "day", day: "weekend", next: true })
  for (var i = 0; i < WORDS.length; i++) {
    var word = WORDS[i]
    if (word.day !== "weekday" || !anyStartsWith(word.names, text)) continue
    var named = word.names.indexOf(text) >= 0
    add(named, { type: "day", day: "weekday", weekday: word.weekday, next: true })
    add(named, { type: "day", day: "weekday", weekday: word.weekday })
  }
  return exact.concat(partial)
}

// ---------------------------------------------------------------- answers

function combinations(parts) {
  var out = [[]]
  for (var i = 0; i < parts.length; i++) {
    var grown = []
    for (var j = 0; j < out.length; j++) {
      for (var k = 0; k < parts[i].length; k++) grown.push(out[j].concat([parts[i][k]]))
    }
    out = grown
    if (out.length > 400) return []
  }
  return out
}

// The day a day reading names, as a local midnight. Null when the reading
// names no day it can reach.
function dayOf(reading, today, now, hour, minute) {
  if (reading.day === "today") return today
  if (reading.day === "tomorrow") return addDays(today, 1)
  if (reading.day === "weekday")
    return reading.next ? nextWeekWeekday(today, reading.weekday) : upcomingWeekday(today, reading.weekday)
  if (reading.day === "weekend") {
    var saturday = upcomingWeekday(today, 6)
    return reading.next ? addDays(saturday, 7) : saturday
  }
  if (reading.day === "nextweek") return nextWeekMonday(today)
  if (reading.day === "nextmonth") return addMonths(today, 1)
  if (reading.day === "date") {
    if (reading.month < 0 || reading.month > 11 || reading.date < 1) return null
    var year = reading.year >= 0 ? reading.year : today.getFullYear()
    if (reading.date > daysInMonth(year, reading.month)) return null
    var day = new Date(year, reading.month, reading.date)
    // A date with no year that has already gone by this year is next year's.
    if (reading.year < 0 && at(day, hour, minute) <= now.getTime())
      day = new Date(year + 1, reading.month, reading.date)
    return day
  }
  if (reading.day === "ordinal") {
    for (var ahead = 0; ahead < 13; ahead++) {
      var month = new Date(today.getFullYear(), today.getMonth() + ahead, 1)
      if (reading.date < 1 || reading.date > daysInMonth(month.getFullYear(), month.getMonth())) continue
      var candidate = new Date(month.getFullYear(), month.getMonth(), reading.date)
      if (at(candidate, hour, minute) > now.getTime()) return candidate
    }
    return null
  }
  return null
}

function offsetDay(reading, today) {
  var n = reading.n
  var key = reading.unit.key
  if (key === "day") return addDays(today, n)
  if (key === "week") return addDays(today, 7 * n)
  if (key === "month") return addMonths(today, n)
  if (key === "year") return addMonths(today, 12 * n)
  if (key === "weekday") return addDays(upcomingWeekday(today, reading.unit.weekday), 7 * (n - 1))
  return null
}

function plural(n, unit) {
  return n === 1 ? unit.one : unit.many
}

// One combination of readings, as an answer, or null when the readings do
// not make a time together ("tom 2h", two days, two clocks).
function answer(readings, now, opts) {
  var today = localDay(now)
  var offset = null
  var day = null
  var time = null
  var special = null
  for (var i = 0; i < readings.length; i++) {
    var reading = readings[i]
    if (reading.type === "offset") {
      if (offset || day) return null
      if (reading.unit.key === "minute" || reading.unit.key === "hour") offset = reading
      else day = reading
    } else if (reading.type === "day") {
      if (day || offset) return null
      day = reading
    } else if (reading.type === "time" || reading.type === "timeword") {
      if (time) return null
      time = reading
    } else if (reading.type === "special") {
      if (special) return null
      special = reading
    }
  }
  if (offset) {
    if (day || time || special) return null
    var span = offset.n * (offset.unit.key === "minute" ? MINUTE : HOUR)
    var when = now.getTime() + span
    return { at: when, label: "In " + offset.n + " " + plural(offset.n, offset.unit), detail: describe(when, now.getTime()) }
  }
  if (special) {
    if (day || time) return null
    if (special.key === "later") {
      var soon = later(now)
      if (localDay(new Date(soon)).getTime() !== today.getTime()) return null
      return { at: soon, label: "Later today", detail: clock(new Date(soon)) }
    }
    var friday = isoDay(today) <= 5 ? addDays(today, 5 - isoDay(today)) : upcomingWeekday(today, 5)
    var endOfWeek = at(friday, 17, 0)
    if (endOfWeek <= now.getTime() + MIN_LEAD) endOfWeek = at(addDays(friday, 7), 17, 0)
    return { at: endOfWeek, label: "End of the week", detail: absolute(endOfWeek, now.getTime()) }
  }
  var hour = time ? time.hour : opts.morningHour
  var minute = time && time.type === "time" ? time.minute : 0
  if (!day) {
    // A time with no day is the next time the clock reads it.
    var first = at(today, hour, minute)
    if (first <= now.getTime() + MIN_LEAD) first = at(addDays(today, 1), hour, minute)
    return plain(first, now)
  }
  if (day.type === "offset") {
    var base = offsetDay(day, today)
    if (!base) return null
    var target = at(base, hour, minute)
    if (day.unit.key === "weekday" && day.n === 1) return plain(target, now)
    return { at: target, label: "In " + day.n + " " + plural(day.n, day.unit), detail: describe(target, now.getTime()) }
  }
  if (day.day === "today" && !time) {
    var laterToday = later(now)
    if (localDay(new Date(laterToday)).getTime() !== today.getTime()) return null
    return { at: laterToday, label: "Later today", detail: clock(new Date(laterToday)) }
  }
  var resolved = dayOf(day, today, now, hour, minute)
  if (!resolved) return null
  var moment = at(resolved, hour, minute)
  if (!time) {
    var named = namedDay(day)
    if (named !== "") return { at: moment, label: named, detail: absolute(moment, now.getTime()) }
  }
  return plain(moment, now)
}

function namedDay(reading) {
  if (reading.day === "weekend") return reading.next ? "Next weekend" : "This weekend"
  if (reading.day === "nextweek") return "Next week"
  if (reading.day === "nextmonth") return "Next month"
  return ""
}

function plain(moment, now) {
  var label = describe(moment, now.getTime())
  var days = Math.round((localDay(new Date(moment)).getTime() - localDay(now).getTime()) / DAY_MS)
  var detail = days <= 6 ? shortDate(new Date(moment), now) : "In " + days + " days"
  return { at: moment, label: label, detail: detail }
}

// ------------------------------------------------------------------ words

function clock(date) {
  return Mail.pad(date.getHours()) + ":" + Mail.pad(date.getMinutes())
}

function shortDate(date, now) {
  var text = Mail.WEEKDAYS[date.getDay()] + ", " + Mail.MONTHS[date.getMonth()] + " " + date.getDate()
  return date.getFullYear() === now.getFullYear() ? text : text + ", " + date.getFullYear()
}

// "Thu, Oct 1, 15:00": the instant itself, with no relative word.
function absolute(ms, nowMs) {
  var date = new Date(ms)
  return shortDate(date, new Date(nowMs)) + ", " + clock(date)
}

// "Today, 15:00", "Tomorrow, 08:00", "Friday, 15:00" within the week, and
// the date itself beyond it.
function describe(ms, nowMs) {
  var date = new Date(ms)
  var now = new Date(nowMs)
  var days = Math.round((localDay(date).getTime() - localDay(now).getTime()) / DAY_MS)
  if (days === 0) return "Today, " + clock(date)
  if (days === 1) return "Tomorrow, " + clock(date)
  if (days > 1 && days <= 6) return FULL_DAYS[date.getDay()] + ", " + clock(date)
  return absolute(ms, nowMs)
}

// For the Snoozed mailbox's time lane, where a row has room for a few
// characters: the clock today, a weekday within the week, a date beyond.
function compact(ms, nowMs) {
  var date = new Date(ms)
  var now = new Date(nowMs)
  var days = Math.round((localDay(date).getTime() - localDay(now).getTime()) / DAY_MS)
  if (days === 0) return clock(date)
  if (days > 0 && days <= 6) return Mail.WEEKDAYS[date.getDay()] + " " + clock(date)
  if (date.getFullYear() === now.getFullYear()) return Mail.MONTHS[date.getMonth()] + " " + date.getDate()
  return Mail.MONTHS[date.getMonth()] + " " + date.getDate() + ", " + date.getFullYear()
}

// Whether an instant is one a snooze may end at.
function acceptable(ms, nowMs) {
  var value = Number(ms)
  return isFinite(value) && value > Number(nowMs) + MIN_LEAD && value <= Number(nowMs) + MAX_AHEAD
}

// ----------------------------------------------------------------- public

function options(value) {
  var given = value || ({})
  var morning = Number(given.morningHour)
  var limit = Number(given.limit)
  return {
    morningHour: isFinite(morning) && morning >= 0 && morning <= 23 ? Math.floor(morning) : DEFAULT_MORNING,
    limit: isFinite(limit) && limit >= 1 ? Math.floor(limit) : DEFAULT_LIMIT
  }
}

// What an empty field offers: the handful of answers people reach for.
function defaults(nowMs, value) {
  var opts = options(value)
  var now = new Date(Number(nowMs))
  var today = localDay(now)
  var out = []
  var soon = later(now)
  if (localDay(new Date(soon)).getTime() === today.getTime())
    out.push({ at: soon, label: "Later today", detail: clock(new Date(soon)) })
  if (now.getHours() < 17) {
    var evening = at(today, 18, 0)
    out.push({ at: evening, label: "This evening", detail: clock(new Date(evening)) })
  }
  var tomorrow = at(addDays(today, 1), opts.morningHour, 0)
  out.push({ at: tomorrow, label: "Tomorrow", detail: absolute(tomorrow, nowMs) })
  if (today.getDay() !== 6 && today.getDay() !== 0) {
    var weekend = at(upcomingWeekday(today, 6), opts.morningHour, 0)
    out.push({ at: weekend, label: "This weekend", detail: absolute(weekend, nowMs) })
  }
  var week = at(nextWeekMonday(today), opts.morningHour, 0)
  out.push({ at: week, label: "Next week", detail: absolute(week, nowMs) })
  return finish(out, nowMs, opts.limit)
}

function finish(list, nowMs, limit) {
  var out = []
  var seen = ({})
  for (var i = 0; i < list.length && out.length < limit; i++) {
    var item = list[i]
    if (!item || !acceptable(item.at, nowMs)) continue
    if (seen[item.at] === true) continue
    seen[item.at] = true
    out.push(item)
  }
  return out
}

// Every reading of `text` that lands on a usable instant, best first:
// [{ at, label, detail }], where `at` is epoch milliseconds.
function predict(text, nowMs, value) {
  var opts = options(value)
  var now = new Date(Number(nowMs))
  var tokens = tokenize(text)
  if (tokens === null) return []
  if (tokens.length === 0) return defaults(nowMs, value)
  var parts = components(joined(tokens))
  if (parts === null || parts.length === 0) return []
  var combos = combinations(parts)
  var out = []
  for (var i = 0; i < combos.length; i++) out.push(answer(combos[i], now, opts))
  return finish(out, nowMs, opts.limit)
}
