# Testing approach

How this project is tested, and why. Read this before writing a test or
changing code that has one.

## Contents

- [Why this document exists](#why-this-document-exists)
- [Three rules](#three-rules)
- [The layers](#the-layers)
- [The fixtures](#the-fixtures)
- [Making code testable](#making-code-testable)
- [Prove the test, not just the code](#prove-the-test-not-just-the-code)
- [Conventions](#conventions)
- [What we deliberately do not test](#what-we-deliberately-do-not-test)
- [Adding a test: checklist](#adding-a-test-checklist)

## Why this document exists

Development found twenty-five real defects. Where they came from is the
argument for everything below.

Five came from reviewing the fixes for the first ten. That is its own lesson. A
refactor that preserves behaviour is exactly where a regression hides, and
self-verified work is the weakest kind.

Two came from the first CI run, which is a sharper version of the same lesson.
Both were tests that passed for months on one machine, because each read
something that machine happened to provide.

Two came from using the app, and no amount of review would have found either.
Two commands read the same cache and reported opposite things about it. And
`esc` from a route-scoped pin unwound a drill path the user never walked, so
leaving a pin took four presses where opening it took one. Both were found by a
person doing the thing, not by anyone reading the diff -- including the two
reviews that had already passed over the second one.

One came from a partial extraction. A board draws a cancelled trip with three
rules. Only one of them moved into a function the pin could share, so the pin
printed the word "cancelled" and kept the amber countdown beside it. Extract a
behaviour for a second caller, or leave it alone. Half of it is worse than
neither, because the caller then looks correct.

The last two came from checking a finished feature against the **live** source
instead of its fixture. The fixture was a trimmed copy of the feed, so anything
it did not contain was invisible. That hid a character reference in the newest
headline. It also hid the fact that the tool used to look at screens had walked
the wrong path since pins were added. A fixture proves the parser. Only the
source proves the fixture.

| Defect | File | Had tests? |
|---|---|---|
| 304 Not Modified arrived as `Ok`, unzipped an empty body | `fetch.rs` | no |
| Realtime fetched once and never refreshed | `app/poll.rs` | no |
| Refresh cadence drifted to 25–50s | `rt.rs` | no |
| Board sorted by scheduled time, displayed by live time | `db/browse.rs` | no |
| Multi-token search unreachable behind the SQL prefilter | `db/search.rs` | no |
| ~340ms of blocking per keystroke | `db/search.rs` | no |
| `desired_height` unclamped for departures | `ui/mod.rs` | partly |
| Breadcrumb lost the platform designator | `db/search.rs` | no |
| A board froze its clock the moment it opened | `app/mod.rs` | no |
| The wait column staggered its "h" on single-digit minutes | `app/clock.rs` | partly |
| The same column overflowed the board entirely past 10 hours | `app/clock.rs` | partly |
| A board left open drained instead of refilling | `app/mod.rs` | no |
| Typing on a board silently disabled `q` and `esc` | `app/mod.rs` | no |
| A capped search reported its limit as a match count | `ui/mod.rs` | no |
| A platform code was measured in bytes, not cells | `ui/layout.rs` | partly |
| A pin showed the scheduled-earliest bus, not the next one | `app/mod.rs` | no |
| `p` swallowed the first filter keystroke on every list screen | `main.rs` | no |
| A first screen left open drained, exactly as a board once did | `app/mod.rs` | no |
| The service day was asserted against the machine's timezone | `app/clock.rs` | yes, wrongly |
| The pty tests ran against whatever cache the machine had | `tests/terminal.rs` | yes, wrongly |
| `update` called the cache current while the browser called it stale | `main.rs` | no |
| A hand-listed entity table left `&#127752;` on screen, and decoded `&amp;lt;` twice | `alerts.rs` | yes, wrongly |
| `dump` and `screenshot` walked into a pin and labelled it "directions" | `dev.rs` | no |
| A cancelled pin kept its countdown, in amber, beside the word "cancelled" | `ui/layout.rs` | no |
| `esc` from a route-scoped pin unwound a drill path nobody walked | `app/mod.rs` | no |

Each of the first ten was in a file with no coverage. The four tests that
existed were on string formatting, which is the part least likely to break. The
thirteen that followed were in files that had plenty of tests by then. Two of
those were in code the suite covered and *passed* over. That is why "the tests
pass" is never the question. The question is whether a test fails when the code
is wrong.

The goal is not a coverage percentage. It is that **the next bug of these
shapes fails a test before it reaches a terminal.**

### Where it stands

Every defect above now has a test that catches it. This is what each module
holds, in the order the app moves through them:

| Module | Covers |
|---|---|
| `app/tests.rs` | navigation, typing, pins, board scope and refresh |
| `app/clock.rs` | the service day, lateness, the wait column |
| `app/poll.rs` | poll cadence and failure policy |
| `db/browse.rs` | routes, directions, boards, ordering, resolving a pin by name |
| `db/search.rs` | search, ranking, platform stripping |
| `db/calendar.rs` | service days and their exceptions |
| `gtfs.rs` | ingest, `parse_hms`, CSV handling |
| `fetch.rs` | 304 handling, empty bodies, feed freshness |
| `rt.rs` | the realtime parser and its anomalies |
| `ui/tests.rs` | the frame: sizing, the two-group screens, alignment, the cursor, a cancelled row |
| `ui/layout.rs` | badges, column widths, the gutter |
| `ui/palette.rs` | which colours can carry a rule, and what a cancelled row shows |
| `logo.rs` | the mark, both fallbacks, its ground line |
| `pins.rs` | the pin file: round trips, odd names, a typo, an older file, an ambiguous route, the fingerprint |
| `alerts.rs` | the updates feed: which kinds count, where routes come from, references decoded |
| `dev.rs` | that the headless walk finds the modes rather than a fixed row |
| `main.rs` | which keys reach the filter and which act |
| `tests/terminal.rs` | inline viewport, clean exit, no tty |

Four fixes here carry no test, and say so instead of carrying a fake one:

- The board's fixed width is derived from `WAIT_W`.
- The drawn rows are capped at `BOARD_LIMIT`.
- `status_bar` is handed the count `draw` already had.
- Route types are routed through `Mode`.

All four are refactors whose output is identical byte for byte. A test could
only assert on call counts, which tests the implementation and not the
behaviour.

A fifth carries no test for the opposite reason. `alerts::FEED_URL` pointed at a
path that answers 301, and worked only because ureq follows redirects. To assert
that a URL answers 200, a test must use the network, and rule 3 forbids that.
A person checks it with `otransit probe` instead.

A sixth is defensive and cannot fail today. `goto` clears `returning_to`, the
screen a pin jumped from. A pin is only drawn on the first screen, so the
recorded screen is always `Screen::Mode`, and the structural answer for every
screen you can reach next is also `Screen::Mode`. A stale flag and a correct one
point at the same place. The line is there so that stays true if a pin is ever
drawn somewhere else, and a test for it would assert on a field rather than on
anything a person could see.

## Three rules

### 1. Write the test first

Red, then green, then refactor. A bug fix is no exception. Reproduce the bug as
a failing test *before* you change the code. A fix without a failing test first
is a guess that happened to work.

This matters more here than usual, because most of these bugs fail *quietly*. A
stale trip ID shows as "no buses tracked". A broken parse shows as `sched` on
every row. A test for a silent failure is worth nothing until someone has seen
it fail.

### 2. Isolation means a controlled world, not no dependencies

Test a function that queries SQLite against a **SQLite database the test
built**. It holds exactly the three stops and two routes the test is about. That
is isolated: deterministic, fast, with no hidden state and no fixture shared
between tests.

A test that runs against `~/Library/Caches/otransit/gtfs.db` is *not* isolated.
That file holds 433 MB of live data, and it changes daily.

The realtime parser works the same way. Isolated means a captured payload that
is checked in, not a network call.

The question that matters is **does the test control every input**, not **does
the function use a dependency**.

### 3. No test may depend on live data or the network

Not the real cache, not the published feed, not the realtime API. A test that
asserts "route 7 runs today" passes until OC Transpo changes the service. Then
it fails at 3am for a reason that has nothing to do with the code.

The test builds every input, or a checked-in fixture supplies it.

## The layers

| Layer | Examples | Isolated by | Target |
|---|---|---|---|
| **Pure functions** | `fmt_wait`, `badge_label`, `strip_platform`, `match_rank`, `natural_key`, `lateness`, `parse_hms` | nothing to isolate | every branch |
| **Query layer** | `departures`, `search_stops`, `active_services`, `routes_for_type`, `stops_for_direction` | in-memory SQLite from the fixture builder | every function, plus every known trap below |
| **Parsers** | `rt::parse` | checked-in trimmed payload | every field, plus every anomaly below |
| **Policy** | refresh cadence, backoff, staleness, schema-version check | extracted from their effectful shells | every branch |
| **Render** | `ui::draw` | `ratatui::TestBackend` | smoke: does each screen render, do columns align |
| **Terminal** | what we emit to a real tty | a pty, in `tests/terminal.rs` | no alt-screen, clean exit, clear non-tty error |
| **Shell** | `main`, `fetch`, thread spawning | — | not tested |

"Maximal coverage" means this: **everything reachable without I/O has a test.**
The shell has none on purpose. To test it, you must mock the world, and those
mocks would be more likely to be wrong than the twenty lines they cover.

### Known traps the query layer must be tested against

These are real properties of the OC Transpo feed. Each one has already caused
or nearly caused a bug:

- **Booking-period duplicates.** `route_id` `7` and `7-1` are the same route in
  different service periods. Queries must dedupe by `short_name` and filter by
  active service date.
- **Times past 24:00.** `arrival_time` reaches `28:xx`. A trip scheduled for
  `25:10` on Friday is the one you catch at `01:10` on Saturday. It must appear
  on Saturday's board.
- **Service-day calendars.** `calendar_dates` adds and removes services on given
  dates. Honour both exception types.
- **Non-unique `stop_code`.** One code can cover seven platforms.
- **Sparse `platform_code`.** Present for 215 of 5,859 stops, absent otherwise.
- **Ordering.** A board is ordered by *actual* arrival. Assert the order after
  the code applies realtime, not before.

### Anomalies the parser must be tested against

Captured from the live feed. The endpoint has `beta` in its URL, so its shape
will change:

- PascalCase field names with `HasX` booleans beside every optional `X`.
- No `Delay` field at all. There are only absolute `Arrival.Time` epochs.
- `ScheduleRelationship = 3` (CANCELED) with an empty `StopTimeUpdate` list.
- `ScheduleRelationship = 8`, which is **not in the GTFS-RT spec**. These are
  added or unscheduled trips. Their trip IDs are negative and are not in the
  static feed.
- Entries that carry only `Departure` and no `Arrival`.
- `HasTime: false` beside a `Time` value that means nothing.
- Predictions in the past.

A parse that silently returns zero arrivals must look different from a feed with
no active trips. Assert on counts, not only on the absence of an error.

## The fixtures

All three live in [src/testing.rs](src/testing.rs) and compile only under
`#[cfg(test)]`.

**`TestGtfs`** — an in-memory cache. It is built through `gtfs::create_schema`,
so it can never drift from the schema production uses.

```rust
let g = TestGtfs::new()
    .route("7", "7", 3, "0057B8")          // id, short_name, route_type, colour
    .always("WD")                          // a service that runs every day
    .trip("t1", "7", "WD", "St-Laurent")
    .stop_on_platform("S1", "3009", "RIDEAU A", "A")
    .stop_time("t1", "S1", 1, "25:10:00"); // may exceed 24:00, as the feed does
```

**`TestRt`** — a TripUpdates payload in OC Transpo's .NET shape. It has one
method per anomaly, so no test writes PascalCase JSON by hand:

```rust
let bytes = TestRt::new(feed_ts)
    .arrival("t1", "S1", 1000)
    .canceled("t2")
    .unscheduled("t3", "S1", 3000)   // ScheduleRelationship 8
    .no_time("t4", "S1")             // HasTime: false
    .build();
```

**`TestFeed`** — a temporary directory of real GTFS CSV files, for testing the
ingest itself. Every file starts as a minimal valid default. A test overwrites
only the file it is about.

The rules these fixtures exist to keep:

- **In memory, or in a temporary directory.** The unit suite finishes in a
  fraction of a second. That is the whole point: a suite fast enough to keep up
  with editing is a suite that people run.
- **The same schema as production.** Never copy it by hand.
- **Declarative and minimal.** A test about times after midnight declares one
  trip, not a synthetic city.
- **No fixture shared between tests.** Build a fresh one in each test. Shared
  state creates order dependencies, which are their own class of bug.

## Making code testable

Some of the bugs above could not be tested as the code was written. That is a
code problem, not a testing problem. The pattern to apply is **functional core,
imperative shell**. The decision is a pure function. The effect is a thin
wrapper around it.

All four cases below are done, and they are the shape to follow.

**Time is an input, never a global.** `App::offline(conn, date, now, pins)`
takes the service date and the clock, so cases after midnight and at the last
bus are reproducible. It also starts no thread. `App::new` calls
`rt::find_key()`, which reads `.env`, so a test that used it would find the real
key and call the live API.

**Policy is split from effect.** `next_poll(succeeded, backoff)` and
`replaces_on_failure(state)` are pure. `poll_realtime` is a thin loop around
them. Writing that split exposed a live bug at once. The loop had read success
from the *shared state*, so a failure that correctly kept the previous board
left the state `Ready` and the backoff never started.

**Ordering is out of the UI.** `db::sort_by_actual_arrival(&mut [Departure])`
was inline in `App::decorate`, where no test could reach it.

**A background result can be supplied.** The alerts slot is private and
`App::offline` never fetches, so the code that draws a detour was unreachable
from a test. `App::set_alerts` is `#[cfg(test)]` and feeds the real parser in.

Do these refactors *while* you write the tests that need them. Do not save them
for a separate project.

## Prove the test, not just the code

A test that nobody has seen fail is unproven. After you write one, break the
code and confirm the test fails. It must fail for the right reason, and ideally
it must fail alone.

This has caught **four vacuous tests** that would otherwise have sat there and
looked like coverage:

- `no_row_ever_overflows_the_terminal_width` could not fail. ratatui clips at
  the buffer edge, so every row is exactly `w` wide whatever the code does. The
  replacement asserts that an ellipsis appears, which is evidence that *we*
  shortened the text.
- A viewport-height test could not exceed `MAX_ROWS`, because `BOARD_LIMIT`
  already caps the fetch. It now overfills the board directly, which exercises
  the clamp as the backstop it is.
- A column-alignment test used `str::find`, which is a **byte** index. `❯` and
  `…` are three bytes each, so the test reported drift where the columns were
  aligned.
- `an_ampersand_that_starts_nothing_survives…` used `&amp;` as its bare
  ampersand. That decodes, so the test never reached the branch it was named
  after, and the mutation passed. A real bare ampersand (`Rideau & Sussex`)
  kills it. The test was written and caught in the same minute, which is the
  whole argument for mutating at the moment of writing.

One test also carried a wrong belief. Removing our BOM stripping changed
nothing, because the `csv` crate already strips it. The test was testing the
dependency. It now states the property instead, and the code says that the trim
is a second line of defence.

No mutation-testing tool is set up. Do this by hand, at the moment you write the
test, when the cost is one edit.

## Conventions

**Name tests as behaviour claims, not function names.**

```rust
#[test] fn departures_are_ordered_by_actual_arrival_not_schedule() {}
#[test] fn a_trip_scheduled_past_midnight_appears_on_the_next_day() {}
#[test] fn multi_token_search_matches_tokens_in_any_order() {}
```

Do not write `test_departures` or `test_search_2`. The name is the
specification. When it fails in CI, the name alone must say what broke.

**One behaviour per test.** A test that asserts five things fails on the first
and hides the other four.

**Assert the property, not the snapshot**, wherever a property exists.
`assert!(times.is_sorted())` survives a schedule change.
`assert_eq!(times[0], "09:43")` does not.

**Unit tests live beside the code** in `#[cfg(test)] mod tests`. When that
module grows large, move it to a child file and leave `mod tests;` behind, as
`app/tests.rs` and `ui/tests.rs` do. Cross-module tests and tests that need
heavy fixtures live in `tests/`.

**Every bug gets a regression test that carries its story:**

```rust
// The SQL prefilter used the whole query as one literal substring, so
// "bank somerset" never matched "BANK / SOMERSET W" and the token
// matching below it was unreachable.
#[test] fn multi_token_search_matches_tokens_in_any_order() {}
```

## What we deliberately do not test

This list is written down so that nobody argues it again:

- **SQLite, ratatui, ureq, serde_json.** A test of a dependency tests the wrong
  thing.
- **The network.** `fetch::download` and `rt::fetch` are thin I/O wrappers. We
  extract and test their *logic*, which is 304 handling and empty-body
  rejection. We do not test the socket.
- **How a terminal draws what we send.** Whether Warp paints a background cell
  correctly is not ours to assert. What we *emit* is ours, and
  `tests/terminal.rs` covers it through a real pty.
- **The exact rendered layout.** A snapshot test of a full frame breaks on every
  cosmetic change, and then someone updates all of them at once, which makes the
  snapshots worthless. Assert structural properties instead: the columns align,
  `MAX_ROWS` bounds the board, and nothing exceeds the width.
- **Performance, as a unit test.** A timing assertion is flaky. Keep the
  benchmark as a separate command that reports a number and never fails a build.

## Adding a test: checklist

- [ ] Does it fail before the fix, and for the right reason? Run it and watch.
- [ ] **Did you break the code and watch it fail?** An untried test is unproven.
- [ ] Does the name state the behaviour instead of the function?
- [ ] Does it control every input? No clock, no network, no real cache.
- [ ] Does it build its own fixture instead of sharing one?
- [ ] Does it assert a property, where a property exists?
- [ ] Does it test *our* code and not a dependency?
- [ ] If it is a regression test, does a comment record the original bug?
- [ ] Does it run in milliseconds?
