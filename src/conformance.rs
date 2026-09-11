//! What the fixed worlds in `conformance/` prove about the app.
//!
//! These assertions are not about `replay`. They are about the program replay
//! drives: which screens a session reaches, what a cancelled row draws, which
//! stops a pole number offers, what silence from every feed looks like, and how
//! the realtime cadence behaves when the feed refuses.
//!
//! They live together, and apart from the harness, because their subject is one
//! thing and it is not the harness. `replay` keeps the three tests that prove
//! itself: that every fixture replays, that a replay repeats, and that a frame
//! is drawn the way the event loop draws one.
//!
//! The vocabulary for reading a fixture lives here too. It used to sit inside
//! `replay`'s private test module, so `semantic` grew a second copy of the part
//! it needed.

use crate::replay::Frame;
use serde_json::Value;

pub(crate) fn suite() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance")
}

/// One checked-in session, replayed. A fixture, not the real cache: every
/// input it reads is in `conformance/`.
pub(crate) fn fixture(name: &str) -> Vec<Frame> {
    crate::replay::frames(&suite().join(name))
        .unwrap_or_else(|e| panic!("{name} did not replay: {e:#}"))
}

/// The session the rest of these assertions are about.
fn recorded() -> Vec<Frame> {
    fixture("drilldown")
}

/// Every row of every frame, for asking whether a session ever drew a thing.
pub(crate) fn all(frames: &[Frame]) -> Vec<String> {
    frames.iter().flat_map(|f| f.rows.iter().cloned()).collect()
}

/// Brand red, as the style map writes it.
const ACCENT: &str = "#da3839";

/// The style line for the row holding `needle`, if that row has one.
fn row_style<'a>(f: &'a Frame, needle: &str) -> Option<&'a String> {
    let y = f.rows.iter().position(|r| r.contains(needle))?;
    f.styles
        .iter()
        .find(|s| s.starts_with(&format!("{y:>3}  ")))
}

/// The status bar names the screen, so this is how a frame says which one
/// it is. The last row, because the bar is anchored there.
pub(crate) fn on_screen<'a>(frames: &'a [Frame], title: &str) -> Vec<&'a Frame> {
    frames
        .iter()
        .filter(|f| f.rows.last().is_some_and(|last| last.contains(title)))
        .collect()
}

/// What the app's polling policy had done by each frame of a fixture.
fn cadence(name: &str) -> Vec<(u64, u64, i64)> {
    fixture(name)
        .iter()
        .filter_map(|f| f.semantic.get("feed").cloned())
        .map(|feed| {
            let n = |k: &str| feed[k].as_i64().unwrap_or(-1);
            (n("requests") as u64, n("failures") as u64, n("due_in"))
        })
        .collect()
}

#[test]
fn silence_draws_as_silence() {
    // The fixture with no realtime, no detours and no weather. Each of
    // those failing looks exactly like the feed having nothing to say, so
    // what the app draws when there is genuinely nothing has to be written
    // down or the two can never be told apart.
    let frames = fixture("quiet-feeds");
    let rows = all(&frames);
    assert!(
        rows.iter().any(|r| r.contains("sched")),
        "no board was reached"
    );
    assert!(
        rows.iter().all(|r| !r.contains('⚠')),
        "a detour appeared with no updates.xml"
    );
    assert!(
        rows.iter().all(|r| !r.contains('°')),
        "a temperature appeared with no weather.json"
    );
    // The same route reads `4 late` in `drilldown`, which has an rt.json.
    // Without one, every row on every board is scheduled.
    for f in on_screen(&frames, "Departures") {
        for row in f.rows.iter().filter(|r| r.contains(" min")) {
            assert!(row.contains("sched"), "a live time with no feed: {row:?}");
        }
    }
}

#[test]
fn a_fixture_that_names_no_answers_is_asked_nothing() {
    // The queue is the whole of a fixture's realtime cadence. Without one
    // there is no cadence to compare, and reporting zeros would read as
    // "asked nothing, failed nothing" rather than "nobody was asking".
    for f in fixture("drilldown") {
        let feed = &f.semantic["feed"];
        assert!(feed["note"].is_string() || feed["note"].is_null());
        assert!(
            feed.get("requests").is_none(),
            "a fixture with no queued answers reported a cadence: {feed}"
        );
    }
}

#[test]
fn the_recorded_session_still_reaches_every_screen_it_recorded() {
    // The script is written against key bindings. Change one and its steps
    // stop meaning what they meant, which this is here to notice.
    let all: Vec<String> = recorded()
        .iter()
        .flat_map(|f| f.rows.iter().cloned())
        .collect();
    let seen = |needle: &str| all.iter().any(|r| r.contains(needle));
    assert!(seen("What are you taking?"), "never saw the first screen");
    assert!(seen("Which route?"), "never saw the route list");
    assert!(seen("Which way?"), "never saw the directions");
    assert!(seen("Transit stops"), "never saw the search");
    assert!(seen("Departures"), "never saw a board");
}

#[test]
fn the_detour_rides_down_from_the_route_to_the_stops() {
    // Both screens below a route carry it, which is what `below_a_route`
    // says and the reason it is one predicate rather than two matches.
    //
    // Asked per screen, not per frame: the session passes through the
    // directions twice, so counting frames says "two" even when only that
    // one screen carries it.
    let on = |title: &str| {
        recorded().iter().any(|f| {
            let rows = &f.rows;
            rows.last().is_some_and(|last| last.contains(title))
                && rows.iter().any(|r| r.contains('⚠'))
        })
    };
    assert!(on("Which way?"), "no detour on the directions");
    assert!(on("Which stop?"), "no detour on the stops");
}

#[test]
fn a_cancelled_trip_shows_no_countdown_anywhere_in_the_session() {
    // The fixture sends ScheduleRelationship 3 for one trip. Wherever that
    // row is drawn it must give an em dash, never a number.
    let rows: Vec<String> = recorded()
        .iter()
        .flat_map(|f| f.rows.iter().cloned())
        .filter(|r| r.contains("cancelled"))
        .collect();
    assert!(
        !rows.is_empty(),
        "the cancelled trip never reached a screen"
    );
    for r in &rows {
        assert!(
            r.contains('\u{2014}'),
            "a cancelled row kept a countdown: {r:?}"
        );
    }
}

#[test]
fn a_cancelled_trip_on_a_pin_gives_up_its_countdown_too() {
    // A board and a pin draw a departure through different arms of the same
    // function, and only the board's arm had a fixture. `drilldown` pins
    // this stop scoped to route 44, so it never draws the 48 cancelled
    // there. This one pins the stop itself, at an hour when that 48 is next.
    let rows = all(&fixture("pinned-stop"));
    let pin = rows
        .iter()
        .find(|r| r.contains("TRANSITWAY / TERMINAL"))
        .expect("the fixture pins TRANSITWAY / TERMINAL");
    assert!(
        pin.contains("cancelled"),
        "the pin drew a different trip: {pin:?}"
    );
    assert!(
        pin.contains('\u{2014}'),
        "a cancelled pin kept its countdown: {pin:?}"
    );
}

#[test]
fn every_rail_departure_reads_sched() {
    // The O-Train has no realtime at all. A rail row claiming otherwise
    // would be the app inventing a prediction nobody published.
    let mut boards = 0;
    for f in recorded() {
        let rows = &f.rows;
        if !rows
            .last()
            .is_some_and(|r| r.contains("Departures   O-Train"))
        {
            continue;
        }
        boards += 1;
        for r in rows.iter().filter(|r| r.contains(" min")) {
            assert!(r.contains("sched"), "rail row was not scheduled: {r:?}");
        }
    }
    assert!(boards > 0, "the session never opened a rail board");
}

#[test]
fn a_number_in_a_name_is_not_a_platform() {
    // `CANTERBURY / AD. 860` carries no platform_code. The 860 is a
    // municipal address. A platform is drawn in brand red beside the name,
    // so this is a question about the colours and not about the text --
    // both stops read the same in plain text and differ in the style map.
    let frames = fixture("platforms");
    // Brand red marks the cursor as well as a platform, so a row carrying
    // both has two runs of it and a row carrying only the cursor has one.
    let badge = |needle: &str| {
        on_screen(&frames, "Transit stops")
            .iter()
            .any(|f| row_style(f, needle).is_some_and(|s| s.matches(ACCENT).count() > 1))
    };
    assert!(
        badge("BILLINGS BRIDGE 3B"),
        "a real platform lost its badge"
    );
    assert!(
        !badge("CANTERBURY / AD. 860"),
        "an address in a name was drawn as a platform"
    );
}

#[test]
fn one_stop_code_offers_every_platform_it_covers() {
    // 3034 is five stops. The code does not say which platform you are
    // standing on, so a search for it must not choose one for you.
    let frames = fixture("platforms");
    let listed = on_screen(&frames, "Transit stops")
        .iter()
        .map(|f| f.rows.iter().filter(|r| r.contains("#3034")).count())
        .max()
        .unwrap_or(0);
    assert_eq!(listed, 5, "a shared pole number lost platforms");
}

#[test]
fn a_list_filter_reads_more_than_the_column_it_shows() {
    // A route is found by its name as well as its number, and a stop by its
    // pole number as well as its name. Both are the in-place filter rather
    // than the database search, which is a different path.
    //
    // Asked of the frame that shows the filter in its status bar, not of
    // the screen. The unfiltered route list already holds two rows saying
    // "Hurdman", and the unfiltered stop list already holds
    // WALKLEY / BANFF, so asking the screen passed whatever the filter did.
    let frames = fixture("filter");
    let typed = |text: &str| {
        frames
            .iter()
            .find(|f| f.rows.last().is_some_and(|bar| bar.contains(text)))
            .unwrap_or_else(|| panic!("no frame has {text} in its status bar"))
    };

    // Both routes carry Hurdman in a long name and neither carries it in a
    // number, so a filter reading numbers alone empties this frame.
    let by_name = typed("/hurdman");
    assert_eq!(
        by_name
            .rows
            .iter()
            .filter(|r| r.contains("Hurdman"))
            .count(),
        2,
        "a route was not found by a word from its long name: {:?}",
        by_name.rows
    );

    // WALKLEY / BANFF holds no digits, so a filter reading names alone
    // empties this one.
    let by_code = typed("/8384");
    assert!(
        by_code.rows.iter().any(|r| r.contains("WALKLEY / BANFF")),
        "a stop was not found by its pole number: {:?}",
        by_code.rows
    );
}

#[test]
fn a_letter_on_a_list_goes_to_the_filter_and_not_to_its_other_meaning() {
    // `p` pins a board. On a list it is a letter, and guarding it on "the
    // filter is empty" ate the first letter of every search: no stop
    // beginning with P could be found.
    let frames = fixture("filter");
    assert!(
        on_screen(&frames, "Which route?")
            .iter()
            .any(|f| f.rows.last().is_some_and(|r| r.contains("/p"))),
        "p did not reach the filter on a list screen"
    );
    assert!(
        on_screen(&frames, "Departures")
            .iter()
            .any(|f| f.rows.last().is_some_and(|r| r.contains("p unpin"))),
        "p did not pin on a board"
    );
}

#[test]
fn a_move_away_from_a_pin_forgets_where_the_pin_came_from() {
    // Entering a pin is the one jump in this app, so it records the screen
    // it left. Every other move clears that record. `drilldown` enters a pin
    // and escs straight out, which uses the record and empties it; nothing
    // walked the path where a later move has to clear it.
    let frames = fixture("pin-onward");
    let last = frames.last().expect("no frames").rows.last().cloned();
    let last = last.unwrap_or_default();
    assert!(
        last.contains("Which way?"),
        "esc went home from four levels down instead of up one: {last:?}"
    );
}

#[test]
fn a_day_with_no_service_says_so_rather_than_showing_stops() {
    // The stops are still in the cache on a Saturday; the service is not
    // running. An implementation that searched stops without asking what
    // calls there would list them happily.
    let frames = fixture("empty");
    let rows = all(&frames);
    assert!(
        rows.iter().any(|r| r.contains("0 routes running today")),
        "a day with no service still counted routes"
    );
    assert!(
        rows.iter().any(|r| r.contains("0 found")),
        "a search on a dead day still found stops"
    );
}

#[test]
fn a_reading_that_does_not_fit_is_dropped_rather_than_cut() {
    // Half a temperature is worse than none, and an unknown icon code
    // draws no glyph for the same reason: a stand-in would sit beside the
    // `·` and read as a character of its own.
    // The whole row, not just the degree sign: a truncation that happened
    // to cut at the `°` would pass that and still leave `⛆ light rain · 21`
    // sitting on the rule. What must be true is that the line is a line.
    for f in fixture("too-narrow") {
        let rule = f.rows.first().expect("a frame with no rows").clone();
        assert_eq!(
            rule,
            "─".repeat(rule.chars().count()),
            "a reading reached a rule with no room for it, on {:?}",
            f.label
        );
    }
    let unknown = all(&fixture("weather-unknown"));
    let label = unknown
        .iter()
        .find(|r| r.contains('°'))
        .expect("the unknown-code fixture drew no weather at all");
    assert!(
        label.contains("funnel cloud") && label.contains("-12°"),
        "the reading was lost with its glyph: {label:?}"
    );
    for glyph in crate::weather::legend() {
        assert!(
            !label.contains(glyph.0),
            "an unknown code drew {:?} as a stand-in",
            glyph.0
        );
    }
}

#[test]
fn a_night_icon_code_draws_the_weather_of_its_day_form() {
    // Codes 30 to 39 are the night forms of 0 to 9. `weather-unknown` proves
    // an unknown code draws no glyph, which is the same outcome an unfolded
    // night code produces, so it cannot tell the two apart.
    let rows = all(&fixture("weather-night"));
    let rule = rows.first().cloned().unwrap_or_default();
    assert!(
        rule.contains('☁') && rule.contains("mainly cloudy"),
        "code 32 did not fold to its day form: {rule:?}"
    );
}

#[test]
fn a_first_refusal_reaches_the_status_bar() {
    // `cadence` refuses only after a board is on screen, where the refusal
    // is correctly ignored. With nothing to protect, the failure has to be
    // said out loud: silence would read as "no buses are running" on a
    // morning the agency was down.
    let rows = all(&fixture("feed-down"));
    assert!(
        rows.iter().any(|r| r.contains("live: http status: 503")),
        "the refusal never reached the status bar"
    );
}

#[test]
fn the_cadence_backs_off_after_a_refusal_and_recovers() {
    // The one fixture whose clock moves. Every number here is the app's own
    // decision: the script says what the feed answers and never when it is
    // asked, so a port that polls on a different cadence takes a different
    // number of answers and diverges on the first `wait`.
    assert_eq!(
        cadence("cadence"),
        vec![
            // Before anything: one attempt, answered, 25s until the next.
            (1, 0, 25),
            (1, 0, 25), // typing asks nothing
            (1, 0, 25), // nor does opening a board
            // 30s on. The attempt came due at 25s and is recorded there,
            // not where this script noticed it, so the minute a refusal
            // costs runs from 25 and has 55 of it left at 30.
            (2, 1, 55),
            (2, 1, 25),  // 30s more is still inside that minute
            (3, 2, 105), // it expired, and a second refusal doubled the wait
            (3, 2, 45),  // still counting down: the backoff really grew
            (4, 0, 20),  // answered, so the backoff is gone rather than halved
            // A wait spanning several intervals at the recovered cadence.
            // Three attempts, not one: the app would have made them all,
            // and serving a single answer per step hid two of them.
            (7, 0, 5),
            // A refusal after a recovery. The wait it costs starts again at
            // a minute. A backoff that survived its own recovery would
            // double the old value instead, and ask for 215 here.
            (8, 1, 35),
            // Past the end of the queue: the cadence came round, found
            // nothing to answer with, and the attempt stays owed.
            (8, 1, 0),
            // And a long wait, for the board below rather than the feed.
            (8, 1, 0),
        ],
        "the polling policy is not the one the app runs"
    );
}

#[test]
fn a_refusal_keeps_the_board_it_could_not_replace() {
    // Stale predictions carrying an honest age beat no predictions, and the
    // age is on screen. Two refusals in a row must not blank a board, and
    // the answer after them must move it.
    let frames = fixture("cadence");
    let late = |f: &Frame| {
        f.rows
            .iter()
            .find(|r| r.contains("08:26") || r.contains("08:32"))
            .cloned()
    };
    // Split where the feed came back, rather than counting from the end:
    // how many frames follow the recovery is the script's business.
    let seen: Vec<String> = frames.iter().filter_map(late).collect();
    let back = seen
        .iter()
        .position(|r| r.contains("10 late"))
        .unwrap_or_else(|| panic!("the feed came back and the board did not move: {seen:?}"));
    assert!(
        back >= 3,
        "the outage was too short to prove anything: {seen:?}"
    );
    assert!(
        seen[..back].iter().all(|r| r.contains("4 late")),
        "a refusal wiped a board it should have kept: {seen:?}"
    );
    assert!(
        seen[back..].iter().all(|r| r.contains("10 late")),
        "the board slipped back after the feed recovered: {seen:?}"
    );

    // And the mechanism, not just the effect. A departure row would keep
    // its old prediction either way, because nothing clears one. What says
    // the board was *kept* is the status bar still reporting an age: the
    // refusal did not replace a state that already had a board in it.
    for f in &frames {
        let note = f.semantic["feed"]["note"].as_str().unwrap_or_default();
        assert!(
            note.is_empty() || note.starts_with("live") && !note.contains("live:"),
            "a refusal reached the status bar: {note:?}"
        );
    }
}

#[test]
fn a_board_left_open_long_enough_loses_the_bus_that_went() {
    // Every other fixture holds its clock still, so nothing was ever asking
    // a board to re-query. `cadence` waits past a departure, and the row
    // for it has to be gone by the last frame.
    let frames = fixture("cadence");
    let first = frames
        .iter()
        .find(|f| f.rows.iter().any(|r| r.contains("08:32")));
    assert!(first.is_some(), "the late bus was never on the board");
    let last = frames.last().expect("no frames");
    // The board has to still be on screen. A row is absent both when it was
    // re-queried away and when the session walked off the screen holding
    // it, and only the first is what this is about.
    assert!(
        last.rows
            .last()
            .is_some_and(|bar| bar.contains("Departures")),
        "the session left the board, so its last frame proves nothing"
    );
    assert!(
        !last.rows.iter().any(|r| r.contains("08:32")),
        "a departed bus stayed on a board the clock had moved past"
    );
}

// ---- the semantic layer, over every fixture ----

/// Every frame of every fixture, snapshot and rows together.
///
/// Through `replay::fixtures`, which already knows what marks a directory
/// as a fixture and already sorts them. Walking the directory again here
/// meant a second copy of both, and the copy did not sort.
pub(crate) fn every_frame() -> Vec<Frame> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance");
    let all: Vec<Frame> = crate::replay::fixtures(&dir)
        .expect("no fixtures")
        .iter()
        .flat_map(|f| crate::replay::frames(f).expect("a fixture did not replay"))
        .collect();
    assert!(all.len() > 50, "only {} frames", all.len());
    all
}

/// Every key in a snapshot, at every depth: `departures[].late` as well as
/// `departures`.
///
/// Depth is the point. The vocabulary is the set of concepts this layer
/// permits, and a concept nested inside another is still one. Guarding only
/// the top level let a departure grow `trip_id` and `route_color` -- the two
/// fields the module doc names as deliberately absent, and explains why --
/// without anything noticing.
fn key_paths(v: &Value, prefix: &str, out: &mut Vec<String>) {
    match v {
        Value::Object(map) => {
            for (k, val) in map {
                let path = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                out.push(path.clone());
                key_paths(val, &path, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                key_paths(item, &format!("{prefix}[]"), out);
            }
        }
        _ => {}
    }
}

#[test]
fn the_vocabulary_stays_small() {
    // Every name is a decision a person would notice. This test is the
    // gate: rules one and two both push toward completeness, and a complete
    // dump is this app's structs wearing JSON, which a second
    // implementation would have to copy rather than agree with. Adding a
    // name should cost an argument, so it costs a diff here.
    let mut seen = Vec::new();
    for f in every_frame() {
        key_paths(&f.semantic, "", &mut seen);
    }
    seen.sort();
    seen.dedup();
    assert_eq!(
        seen,
        [
            "departures",
            "departures[].after_midnight",
            "departures[].cancelled",
            "departures[].headsign",
            "departures[].late",
            "departures[].live",
            "departures[].route",
            "departures[].scheduled",
            "departures[].wait",
            "detour",
            "feed",
            "feed.due_in",
            "feed.failures",
            "feed.note",
            "feed.requests",
            "filter",
            "now",
            "pinned",
            "pins",
            "pins[].headsign",
            "pins[].route",
            "pins[].stop",
            "rows",
            "rows[].primary",
            "rows[].secondary",
            "screen",
            "selected",
            "weather",
        ],
        "the conformance vocabulary changed"
    );
}

#[test]
fn the_snapshot_and_the_frame_describe_one_moment() {
    // Rule one, made checkable. The snapshot reads the values the frame was
    // drawn from, so what it reports has to be what the frame shows.
    // Derived independently the two could disagree, and the suite would
    // then confirm the contradiction on both sides at once.
    let mut detours = 0;
    let mut cancels = 0;
    let mut lates = 0;
    for f in every_frame() {
        let text = f.rows.join("\n");
        let Some(s) = f.semantic.as_object() else {
            continue;
        };

        match s["detour"].as_str() {
            Some(_) => {
                detours += 1;
                assert!(text.contains('⚠'), "a detour was decided and not drawn");
            }
            None => assert!(!text.contains('⚠'), "a detour was drawn and not decided"),
        }

        for d in s
            .get("departures")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if d["cancelled"] == Value::Bool(true) {
                cancels += 1;
                assert!(
                    text.contains("cancelled") && text.contains('\u{2014}'),
                    "a cancelled trip kept its countdown in the frame"
                );
                continue;
            }
            // The note beside a live row says the same thing this field
            // does. One fixture is 290 seconds late on purpose, where
            // truncating the seconds says four minutes and rounding says
            // five, so a second way of deriving this shows up here.
            let Some(late) = d["late"].as_i64() else {
                continue;
            };
            lates += 1;
            let note = match late {
                -1..=1 => "on time".to_string(),
                l if l > 0 => format!("{l} late"),
                l => format!("{} early", -l),
            };
            assert!(
                text.contains(&note),
                "the board and the snapshot disagree about lateness: {note:?} is not drawn"
            );
        }
    }
    assert!(
        detours > 0 && cancels > 0 && lates > 0,
        "{detours} detours, {cancels} cancels, {lates} lateness notes"
    );
}

#[test]
fn a_reading_the_frame_has_no_room_for_is_still_a_decision() {
    // The one place the two layers part company, and on purpose. The app
    // decides the weather; the renderer decides it does not fit. Those are
    // different questions, and only the second knows about widths. So the
    // implication runs one way: a label on the rule must match what was
    // decided, but a decision need not reach the rule.
    let mut dropped = 0;
    for f in every_frame() {
        let Some(w) = f.semantic.get("weather").and_then(Value::as_str) else {
            continue;
        };
        let rule = f.rows.first().map(String::as_str).unwrap_or_default();
        if rule.contains('°') {
            assert!(
                rule.contains(w),
                "the rule drew a reading nobody decided: {rule:?}"
            );
        } else {
            dropped += 1;
            assert_eq!(
                rule.trim_end(),
                "─".repeat(rule.chars().count()).trim_end(),
                "a reading was cut rather than dropped"
            );
        }
    }
    assert!(dropped > 0, "no fixture is narrow enough to drop a reading");
}

#[test]
fn a_prediction_carries_every_derivation_the_board_makes_from_it() {
    // The input is the fixture's rt.json, which both implementations read.
    // What is under test is what each does with it, so all three
    // derivations are here and a port cannot agree by accident: `live`
    // projects the epoch onto the service day, `wait` counts down to it,
    // and `late` is the signed minutes against the timetable.
    let live: Vec<Value> = every_frame()
        .iter()
        .filter_map(|f| {
            f.semantic
                .get("departures")
                .and_then(Value::as_array)
                .cloned()
        })
        .flatten()
        .filter(|d| !d["live"].is_null())
        .collect();
    assert!(!live.is_empty(), "no fixture has a live prediction");
    for d in &live {
        let (sched, l) = (
            d["scheduled"].as_i64().unwrap(),
            d["live"].as_i64().unwrap(),
        );
        let late = d["late"].as_i64().expect("a live row with no lateness");
        assert_eq!(
            late,
            ((l - sched) as f64 / 60.0).round() as i64,
            "lateness disagrees with the two times it is derived from: {d}"
        );
    }
}

#[test]
fn a_pin_is_a_stop_and_the_route_that_narrows_it() {
    // 44 and 48 both end at Billings Bridge by roads that do not meet, so a
    // pin is not a stop. The first screen draws only the stop's name, which
    // is exactly why the identity belongs in here.
    let pinned: Vec<Value> = every_frame()
        .iter()
        .filter_map(|f| f.semantic.get("pins").and_then(Value::as_array).cloned())
        .flatten()
        .collect();
    assert!(!pinned.is_empty(), "no fixture pins anything");
    for p in &pinned {
        assert!(p["stop"].is_string(), "a pin with no stop: {p}");
    }
    assert!(
        pinned.iter().any(|p| p["route"].is_string()),
        "no pin carries the route it was made from: {pinned:?}"
    );
}
