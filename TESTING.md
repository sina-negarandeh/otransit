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

Twenty-one real defects were found during development. Five came from reviewing
the fixes for the first ten, which is its own lesson: a behaviour-preserving
refactor is exactly where a regression hides, and self-verified work is the
weakest kind. Two came from CI's first run, a sharper version of the same
lesson: both were tests that passed for months on one machine because they read
something the machine happened to provide. The last was found by using the app,
which no amount of reviewing would have surfaced: two commands read the same
cache and reported opposite things about it.

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

The first ten were each in a file with no coverage; the four tests that existed
were on string formatting, the part least likely to break. The eight that
followed were in files that by then had plenty — which is why "the tests pass"
is never the question. The question is whether a test fails when the code is
wrong.

The goal is not a coverage percentage. It is that **the next bug of these
shapes fails a test before it reaches a terminal.**

### Where it stands

Every defect above now has a test that catches it. What each module holds,
in the order the app moves through them:

| Module | Covers |
|---|---|
| `app/tests.rs` | navigation, typing, pins, board scope and refresh |
| `app/clock.rs` | the service day, lateness, the wait column |
| `app/poll.rs` | poll cadence and failure policy |
| `db/browse.rs` | routes, directions, boards, ordering |
| `db/search.rs` | search, ranking, platform stripping |
| `db/calendar.rs` | service days and their exceptions |
| `gtfs.rs` | ingest, `parse_hms`, CSV handling |
| `fetch.rs` | 304 handling, empty bodies, feed freshness |
| `rt.rs` | the realtime parser and its anomalies |
| `ui/mod.rs` | the frame: sizing, the status bar, alignment, the cursor |
| `ui/layout.rs` | badges, column widths, the gutter |
| `ui/palette.rs` | which colours can carry a rule |
| `logo.rs` | the mark, both fallbacks, its ground line |
| `pins.rs` | the pin file: round trips, odd names, a hand-edited typo |
| `alerts.rs` | the updates feed: which kinds count, where routes come from |
| `main.rs` | which keys reach the filter and which act |
| `tests/terminal.rs` | inline viewport, clean exit, no tty |

Four fixes here carry no test, and say so rather than carrying a fake one:
deriving the board's fixed width from `WAIT_W`, capping the drawn rows at
`BOARD_LIMIT`, handing `status_bar` the count `draw` already had, and routing
route types through `Mode`. All four are refactors whose output is identical
byte for byte; a test could only assert on call counts, which tests the
implementation rather than the behaviour.

## Three rules

### 1. Write the test first

Red, then green, then refactor. No exceptions for bug fixes: reproduce the bug
as a failing test *before* touching the code. A fix without a failing test
first is a guess that happened to work.

This matters more here than usual, because most of these bugs fail *quietly* —
a stale trip ID shows as "no buses tracked", a broken parse shows as `sched` on
every row. Silent failures need a test to have ever been observed failing.

### 2. Isolation means a controlled world, not no dependencies

A function that queries SQLite is tested against a **SQLite database the test
built**, containing exactly the three stops and two routes the test cares
about. That is isolated: deterministic, fast, no hidden state, no shared
fixtures between tests.

It is *not* isolated to run against `~/Library/Caches/otransit/gtfs.db`. That
is 443MB of live data that changes daily.

Same for the realtime parser: isolated means a checked-in captured payload,
not a network call.

The distinction that matters is **does the test control every input**, not
**does the function touch a dependency**.

### 3. No test may depend on live data or the network

Not the real cache, not the published feed, not the realtime API. A test that
asserts "route 7 runs today" passes until OC Transpo changes service and then
fails at 3am for reasons unrelated to the code.

Every input is either constructed by the test or checked in as a fixture.

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

"Maximal coverage" means: **everything reachable without I/O is tested.** The
shell is deliberately not, because testing it means mocking the world, and the
mocks would be more likely to be wrong than the twenty lines they cover.

### Known traps the query layer must be tested against

These are real properties of the OC Transpo feed. Each one has already caused
or nearly caused a bug:

- **Booking-period duplicates.** `route_id` `7` and `7-1` are the same route in
  different service periods. Queries must dedupe by `short_name` and filter by
  active service date.
- **Times past 24:00.** `arrival_time` reaches `28:xx`. A trip scheduled
  `25:10` on Friday is what you catch at `01:10` on Saturday, and it must
  appear on Saturday's board.
- **Service-day calendars.** `calendar_dates` adds and removes services on
  specific dates; both exception types must be honoured.
- **Non-unique `stop_code`.** One code can cover seven platforms.
- **Sparse `platform_code`.** Present for 215 of 5,859 stops, absent otherwise.
- **Ordering.** Boards are ordered by *actual* arrival, so ordering must be
  asserted after realtime is applied, not before.

### Anomalies the parser must be tested against

Captured from the live feed. The endpoint has `beta` in its URL, so its shape
will change:

- PascalCase field names with `HasX` booleans beside every optional `X`.
- No `Delay` field at all — only absolute `Arrival.Time` epochs.
- `ScheduleRelationship = 3` (CANCELED) with an empty `StopTimeUpdate` list.
- `ScheduleRelationship = 8` — **not in the GTFS-RT spec**. Added/unscheduled
  trips, with negative trip IDs absent from the static feed.
- Entries carrying only `Departure`, no `Arrival`.
- `HasTime: false` alongside a meaningless `Time` value.
- Predictions in the past.

A parse that silently returns zero arrivals must be distinguishable from a feed
with no active trips. Assert counts, not just absence of error.

## The fixtures

All three live in [src/testing.rs](src/testing.rs), compiled only under
`#[cfg(test)]`.

**`TestGtfs`** — an in-memory cache, built through `gtfs::create_schema` so it
can never drift from production's schema.

```rust
let g = TestGtfs::new()
    .route("7", "7", 3, "0057B8")          // id, short_name, route_type, colour
    .always("WD")                          // a service that runs every day
    .trip("t1", "7", "WD", "St-Laurent")
    .stop_on_platform("S1", "3009", "RIDEAU A", "A")
    .stop_time("t1", "S1", 1, "25:10:00"); // may exceed 24:00, as the feed does
```

**`TestRt`** — a TripUpdates payload in OC Transpo's .NET shape, with a method
per anomaly, so no test hand-writes PascalCase JSON:

```rust
let bytes = TestRt::new(feed_ts)
    .arrival("t1", "S1", 1000)
    .canceled("t2")
    .unscheduled("t3", "S1", 3000)   // ScheduleRelationship 8
    .no_time("t4", "S1")             // HasTime: false
    .build();
```

**`TestFeed`** — a temp directory of real GTFS CSV files, for testing the
ingest itself. Every file starts as a minimal valid default; a test overwrites
only the one it is about.

The rules they exist to keep:

- **In memory or a temp dir.** The unit suite finishes in a fraction of a
  second, which is the whole point: a suite that keeps up with editing is a
  suite that gets run.
- **Same schema as production.** Never hand-copied.
- **Declarative and minimal.** A test about after-midnight times declares one
  trip, not a synthetic city.
- **No sharing between tests.** A fresh fixture per test; shared state creates
  order dependencies, which are their own class of bug.

## Making code testable

Some of the bugs above were untestable as written. That is a code problem, not
a testing problem. The pattern is **functional core, imperative shell**: the
decision is a pure function, the effect is a thin wrapper around it.

All three cases are now done, and they are the shape to follow:

**Time is an input, never a global.** `App::offline(conn, date, now)` takes both
the service date and the clock, so after-midnight and last-bus cases are
reproducible. It also spawns no thread — `App::new` calls `rt::find_key()`,
which reads `.env`, so a test using it would have found the real key and hit
the live API.

**Policy split from effect.** `next_poll(succeeded, backoff)` and
`replaces_on_failure(state)` are pure; `poll_realtime` is a thin loop around
them. Writing that split immediately exposed a live bug: the loop had been
reading success from the *shared state*, so a failure that correctly preserved
the previous board left it `Ready` and backoff never engaged.

**Ordering pulled out of the UI.** `db::sort_by_actual_arrival(&mut [Departure])`
was inline in `App::decorate`, where no test could reach it.

Do these refactors *as* the tests that need them are written, not as a separate
project.

## Prove the test, not just the code

A test that has never been seen failing is unproven. After writing one, break
the code and confirm it fails — for the right reason, and ideally alone.

This has caught **three vacuous tests** that would otherwise have sat there
looking like coverage:

- `no_row_ever_overflows_the_terminal_width` could not fail: ratatui clips at
  the buffer edge, so every row is exactly `w` wide no matter what we do.
  Replaced with one asserting an ellipsis appears, which is evidence that *we*
  shortened the text.
- A viewport-height test could not exceed `MAX_ROWS`, because `BOARD_LIMIT`
  already caps the fetch. It now overfills the board directly, exercising the
  clamp as the backstop it is.
- A column-alignment test used `str::find`, a **byte** index — `❯` and `…` are
  three bytes each, so it reported drift where the columns lined up fine.

And one wrong belief: removing our BOM stripping changed nothing, because the
`csv` crate already strips it. The test was testing the dependency. It now
states the property instead, and the code says the trim is belt-and-braces.

There is no mutation-testing tool wired up; this is done by hand, at the moment
a test is written, when the cost is a single edit.

## Conventions

**Name tests as behaviour claims, not function names.**

```rust
#[test] fn departures_are_ordered_by_actual_arrival_not_schedule() {}
#[test] fn a_trip_scheduled_past_midnight_appears_on_the_next_day() {}
#[test] fn multi_token_search_matches_tokens_in_any_order() {}
```

Not `test_departures`, `test_search_2`. The name is the specification; if it
fails in CI, the name alone should say what broke.

**One behaviour per test.** A test asserting five things fails on the first and
hides the rest.

**Assert the property, not the snapshot,** wherever a property exists. `assert!(
times.is_sorted())` survives a schedule change; `assert_eq!(times[0], "09:43")`
does not.

**Unit tests live beside the code** in `#[cfg(test)] mod tests`. Cross-module
and fixture-heavy tests live in `tests/`.

**Every bug gets a regression test carrying its story:**

```rust
// The SQL prefilter used the whole query as one literal substring, so
// "bank somerset" never matched "BANK / SOMERSET W" and the token
// matching below it was unreachable.
#[test] fn multi_token_search_matches_tokens_in_any_order() {}
```

## What we deliberately do not test

Saying this explicitly stops it being re-litigated:

- **SQLite, ratatui, ureq, serde_json.** Testing dependencies tests the wrong thing.
- **The network.** `fetch::download` and `rt::fetch` are thin I/O wrappers.
  Their *logic* (304 handling, empty-body rejection) gets extracted and tested;
  the socket does not.
- **How a terminal draws what we send.** Whether Warp paints a background cell
  correctly is not ours to assert. What we *emit* is ours, and
  `tests/terminal.rs` covers it through a real pty.
- **Exact rendered layout.** Snapshot tests of full frames break on every
  cosmetic change and get blanket-updated, which makes them worthless. Assert
  structural properties instead: columns align, the board is bounded by
  `MAX_ROWS`, nothing overflows the width.
- **Performance, as a unit test.** Timing assertions are flaky. Keep the
  benchmark as a separate command that reports, and does not fail a build.

## Adding a test: checklist

- [ ] Does it fail before the fix, for the right reason? (Run it and watch.)
- [ ] **Did you break the code and watch it fail?** An untried test is unproven.
- [ ] Does the name state the behaviour, not the function?
- [ ] Does it control every input — no clock, no network, no real cache?
- [ ] Does it build its own fixture rather than sharing one?
- [ ] Does it assert a property where a property exists?
- [ ] Is it testing *our* code, not a dependency's?
- [ ] If it's a regression test, does a comment record the original bug?
- [ ] Does it run in milliseconds?
