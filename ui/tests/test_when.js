const assert = require("assert")
const { execFileSync } = require("child_process")
const { load, deepEqual } = require("./load")

const when = load("snooze/When.js")

const local = (year, month, day, hour, minute) => new Date(year, month, day, hour, minute || 0).getTime()
const times = (list) => list.map((item) => item.at)
const labels = (list) => list.map((item) => item.label)
const details = (list) => list.map((item) => item.detail)

if (process.argv[2] === "--dst") {
  // Only meaningful where the zone really is New York; a platform that ignores
  // TZ has nothing to say about daylight saving here.
  if (new Date(2026, 6, 1).getTimezoneOffset() !== 240 || new Date(2026, 11, 1).getTimezoneOffset() !== 300) {
    console.log("when dst tests skipped: TZ is not honoured on this platform")
    return
  }
  // Saturday before the clocks go back on Sunday, November 1.
  const saturday = local(2026, 9, 31, 10, 15)
  deepEqual(times(when.predict("2d", saturday)), [local(2026, 10, 2, 8, 0)], "a day-level snooze keeps its wall-clock morning across DST")
  assert.strictEqual(new Date(when.predict("2d", saturday)[0].at).getHours(), 8)
  const beforeChange = local(2026, 10, 1, 0, 30)
  assert.strictEqual(when.predict("2h", beforeChange)[0].at - beforeChange, 2 * 3600000, "an hour offset is two real hours")
  deepEqual(times(when.predict("tom 3", saturday)), [local(2026, 10, 1, 15, 0), local(2026, 10, 1, 3, 0)])
  console.log("when dst tests passed")
  return
}

// Wednesday, September 30, 2026, at 10:15 in whatever zone runs the tests.
const now = local(2026, 8, 30, 10, 15)

// ------------------------------------------------------- the two asked for

const twoM = when.predict("2m", now)
deepEqual(times(twoM), [now + 2 * 60000, local(2026, 10, 30, 8), local(2026, 9, 12, 8)],
  "2m is minutes, then months, then Mondays")
deepEqual(labels(twoM), ["In 2 minutes", "In 2 months", "In 2 Mondays"])
deepEqual(details(twoM), ["Today, 10:17", "Mon, Nov 30, 08:00", "Mon, Oct 12, 08:00"])

const tomThree = when.predict("tom 3", now)
deepEqual(times(tomThree), [local(2026, 9, 1, 15), local(2026, 9, 1, 3)], "tom 3 is tomorrow at three in the afternoon first")
deepEqual(labels(tomThree), ["Tomorrow, 15:00", "Tomorrow, 03:00"])
deepEqual(details(tomThree), ["Thu, Oct 1", "Thu, Oct 1"])

// ---------------------------------------------------------- prefixes narrow

deepEqual(labels(when.predict("2mo", now)), ["In 2 months", "In 2 Mondays"])
deepEqual(labels(when.predict("2mon", now)), ["In 2 months", "In 2 Mondays"])
deepEqual(labels(when.predict("2mond", now)), ["In 2 Mondays"])
deepEqual(labels(when.predict("2min", now)), ["In 2 minutes"])
deepEqual(times(when.predict("2t", now)), [local(2026, 9, 13, 8), local(2026, 9, 8, 8)], "2t is Tuesdays, then Thursdays")
deepEqual(labels(when.predict("tom", now)), ["Tomorrow, 08:00"])
deepEqual(times(when.predict("tom", now)), [local(2026, 9, 1, 8)], "a day with no time wakes at eight")

const t = when.predict("t", now)
deepEqual(labels(t), ["Later today", "Today, 20:00", "Tomorrow, 08:00", "Tuesday, 08:00"],
  "t offers today, tonight, tomorrow, then Tuesday; Thursday is tomorrow and is not offered twice")

// ------------------------------------------------------------ bare numbers

deepEqual(times(when.predict("3", now)),
  [local(2026, 8, 30, 15), local(2026, 9, 1, 3), now + 3 * 3600000, local(2026, 9, 3, 8), local(2026, 9, 21, 8)],
  "a bare three is a time first, then the start of a duration")
deepEqual(times(when.predict("9", now)),
  [local(2026, 9, 1, 9), local(2026, 8, 30, 21), now + 9 * 3600000, local(2026, 9, 9, 8), local(2026, 11, 2, 8)],
  "nine is a morning first, and this morning's has gone by")
deepEqual(labels(when.predict("12", now)),
  ["Today, 12:00", "Tomorrow, 00:00", "In 12 minutes", "In 12 hours", "In 12 days", "In 12 weeks"])
deepEqual(labels(when.predict("30", now)), ["In 30 minutes", "In 30 hours", "In 30 days", "In 30 weeks"])

// --------------------------------------------------------------- durations

deepEqual(times(when.predict("2h", now)), [now + 2 * 3600000])
deepEqual(details(when.predict("2h", now)), ["Today, 12:15"])
deepEqual(times(when.predict("2d", now)), [local(2026, 9, 2, 8)])
deepEqual(labels(when.predict("2w", now)), ["In 2 weeks"], "two Wednesdays land on the same morning and are not repeated")
deepEqual(times(when.predict("in 2 hours", now)), [now + 2 * 3600000])
deepEqual(labels(when.predict("a week", now)), ["In 1 week"])
deepEqual(times(when.predict("2d 3pm", now)), [local(2026, 9, 2, 15)])
deepEqual(times(when.predict("2d 3", now)), [local(2026, 9, 2, 15), local(2026, 9, 2, 3)])

// --------------------------------------------------------- days and clocks

deepEqual(times(when.predict("fri 3", now)), [local(2026, 9, 2, 15), local(2026, 9, 2, 3)])
deepEqual(times(when.predict("3pm tom", now)), [local(2026, 9, 1, 15)], "the order the words are typed in does not matter")
deepEqual(times(when.predict("tom at 3", now)), [local(2026, 9, 1, 15), local(2026, 9, 1, 3)])
deepEqual(times(when.predict("next fri", now)), [local(2026, 9, 9, 8), local(2026, 9, 2, 8)],
  "next Friday is next week's, with the coming one second")
deepEqual(times(when.predict("next mon", now)), [local(2026, 9, 5, 8), local(2026, 9, 30, 8)],
  "next mon is Monday first -- next week's Monday is also the coming one -- and next month second")
deepEqual(labels(when.predict("next m", now)), ["Next month", "Monday, 08:00"], "a bare letter keeps the table's order")
deepEqual(labels(when.predict("next week", now)), ["Next week", "Next weekend"])
deepEqual(times(when.predict("wed", now)), [local(2026, 9, 7, 8)], "a weekday named on that weekday is next week's")
deepEqual(labels(when.predict("fri", now)), ["Friday, 08:00"])
deepEqual(details(when.predict("fri", now)), ["Fri, Oct 2"])
deepEqual(times(when.predict("3:30", now)), [local(2026, 8, 30, 15, 30), local(2026, 9, 1, 3, 30)])
deepEqual(times(when.predict("930", now)), [local(2026, 9, 1, 9, 30), local(2026, 8, 30, 21, 30)])
deepEqual(times(when.predict("12:3", now)), [local(2026, 8, 30, 12, 30), local(2026, 9, 1, 0, 30)], "a half-typed minute counts as its tens")
deepEqual(times(when.predict("eod", now)), [local(2026, 8, 30, 17)])
deepEqual(times(when.predict("tonight", now)), [local(2026, 8, 30, 20)])
deepEqual(times(when.predict("tom morning", now)), [local(2026, 9, 1, 8)])
deepEqual(labels(when.predict("later", now)), ["Later today"])
deepEqual(times(when.predict("later", now)), [local(2026, 8, 30, 14)], "later is three hours on, rounded up to the hour")

// ------------------------------------------------------------------- dates

deepEqual(times(when.predict("oct 5", now)), [local(2026, 9, 5, 8)])
deepEqual(times(when.predict("5 oct", now)), [local(2026, 9, 5, 8)])
deepEqual(times(when.predict("october 5th", now)), [local(2026, 9, 5, 8)])
deepEqual(times(when.predict("10/5 2pm", now)), [local(2026, 9, 5, 14)])
deepEqual(times(when.predict("2026-10-05 9am", now)), [local(2026, 9, 5, 9)])
deepEqual(times(when.predict("may", now)), [local(2027, 4, 1, 8)], "a month already gone by this year is next year's")
deepEqual(labels(when.predict("may", now)), ["Sat, May 1, 2027, 08:00"])
deepEqual(times(when.predict("5th", now)), [local(2026, 9, 5, 8)])

// ------------------------------------------------------------ nothing fits

deepEqual(when.predict("today 9am", now), [], "a time already gone by is not offered")
deepEqual(when.predict("asdf", now), [])
deepEqual(when.predict("3.5", now), [])
deepEqual(when.predict("tom 2h", now), [], "a day and an hour offset do not make one time")
deepEqual(when.predict("2019-01-01", now), [])

// --------------------------------------------------------------- defaults

const empty = when.predict("", now)
deepEqual(labels(empty), ["Later today", "This evening", "Tomorrow", "This weekend", "Next week"])
deepEqual(times(empty), [local(2026, 8, 30, 14), local(2026, 8, 30, 18), local(2026, 9, 1, 8), local(2026, 9, 3, 8), local(2026, 9, 5, 8)])
deepEqual(details(empty), ["14:00", "18:00", "Thu, Oct 1, 08:00", "Sat, Oct 3, 08:00", "Mon, Oct 5, 08:00"])
const evening = local(2026, 8, 30, 21, 30)
deepEqual(labels(when.predict("", evening)), ["Tomorrow", "This weekend", "Next week"],
  "late at night there is no later today and no this evening")
deepEqual(labels(when.predict("", local(2026, 9, 3, 10))), ["Later today", "This evening", "Tomorrow", "Next week"],
  "on a Saturday this weekend is already here")
deepEqual(times(when.predict("", now, { morningHour: 9 })).slice(2), [local(2026, 9, 1, 9), local(2026, 9, 3, 9), local(2026, 9, 5, 9)])
assert.strictEqual(when.predict("3", now, { limit: 2 }).length, 2)

// ---------------------------------------------------------------- display

assert.strictEqual(when.describe(local(2026, 9, 12, 8), now), "Mon, Oct 12, 08:00")
assert.strictEqual(when.describe(local(2026, 9, 2, 15), now), "Friday, 15:00")
assert.strictEqual(when.compact(local(2026, 8, 30, 15), now), "15:00")
assert.strictEqual(when.compact(local(2026, 9, 2, 15), now), "Fri 15:00")
assert.strictEqual(when.compact(local(2026, 9, 12, 8), now), "Oct 12")
assert.strictEqual(when.compact(local(2027, 0, 4, 8), now), "Jan 4, 2027")

assert.strictEqual(when.acceptable(now + 10000, now), false, "too soon to be worth snoozing")
assert.strictEqual(when.acceptable(now + 60000, now), true)
assert.strictEqual(when.acceptable(now + 6 * 366 * 86400000, now), false, "past what the backend keeps")
assert.strictEqual(when.acceptable("soon", now), false)

// ------------------------------------------------------------ daylight saving

execFileSync(process.execPath, [__filename, "--dst"], {
  env: Object.assign({}, process.env, { TZ: "America/New_York" }),
  stdio: "inherit"
})

console.log("when tests passed")
