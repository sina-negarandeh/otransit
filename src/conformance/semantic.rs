//! What the app decided, over every fixture.
//!
//! The other half of the conformance contract. A rendered diff couples two
//! implementations to one terminal library, and a port that pads a widget
//! differently can be entirely correct, so the snapshot is a second and
//! independent comparison. `src/semantic.rs` says what the vocabulary leaves
//! out and why.
//!
//! Apart from its sibling because the two are read for different reasons, and
//! because one file of both had grown past nine hundred lines. It needs none of
//! the row-reading vocabulary next door: a snapshot is read as JSON, so
//! `every_frame` and `key_paths` are the whole of what this file works with.

use crate::replay::Frame;
use serde_json::Value;

/// Every frame of every fixture, snapshot and rows together.
///
/// Through `replay::fixtures`, which already knows what marks a directory
/// as a fixture and already sorts them. Walking the directory again here
/// meant a second copy of both, and the copy did not sort.
fn every_frame() -> Vec<Frame> {
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
