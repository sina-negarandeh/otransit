//! What the app decided, as a small vocabulary two implementations can share.
//!
//! The frames in `replay` say what a person sees. This says what the program
//! concluded. Both are compared, and a difference here is reported first,
//! because it usually explains a difference in the frame while the reverse is
//! not true. Neither outranks the other as a contract.
//!
//! It exists because a rendered diff couples the two implementations tighter
//! than the behaviour warrants. A Go port using a different terminal library
//! can pad a widget differently and be entirely correct, and a cell-exact diff
//! cannot say so. This layer still can.
//!
//! # What is in it, and what is not
//!
//! One test decides: **would a person notice if this value changed?** If not,
//! it does not belong. The snapshot is a list of decisions, not a dump of the
//! state behind them. A complete dump would be this app's structs wearing JSON,
//! and a second implementation would have to reproduce its internal shape to
//! match — which is the coupling this layer exists to remove.
//!
//! So, deliberately absent:
//!
//! - **`trip_id`.** Nothing draws it. Two departures are told apart by their
//!   position, and their order is itself a decision this already records.
//! - **The breadcrumb trail and the screen title.** Both are a rendering of
//!   `screen`, which is here, and their layout is the other contract's job.
//! - **The predicted epoch a departure came from.** Tempting, because it is the
//!   input the two derived times are derived from. But reading it here means
//!   asking the realtime a second time, by a second route, and the answer could
//!   then disagree with the one `apply_realtime` used. The input is already
//!   fixed: it is in the fixture's `rt.json`, which both implementations read.
//!   `live`, `wait` and `late` are three derivations from it, which is what was
//!   actually wanted.
//! - **Colours.** Entirely the other contract's business.
//!
//! # Where the values come from
//!
//! Every one is read through the accessor `ui::draw` reads it through, at the
//! moment the frame is drawn. Never recomputed here, and never reached for past
//! the renderer into the state behind it. A snapshot derived independently
//! could say `cancelled` while the frame showed a countdown, and the suite
//! would confirm the contradiction on both sides at once. This app has shipped
//! that bug once already, when two places answered one question about a
//! cancelled row.
//!
//! That is also why `weather`, `feed` and `detour` are the assembled lines
//! rather than their parts: the line is what the renderer is handed, and the
//! separator in `⛆ light rain · 21°` is a decision in its own right.
//!
//! # The feed
//!
//! `feed` carries the note the status bar draws, and — where a fixture drives
//! the realtime on a clock it controls — what the app's polling policy has
//! done: how many times it asked, how many of those failed, and how long until
//! it asks again.
//!
//! Those three are the temporal contract in miniature. A port that polls twice
//! as often, or backs off differently after a refusal, agrees on every frame's
//! text and diverges here. `due_in` is the sharpest of them, because it is the
//! policy's decision stated directly rather than inferred from how often
//! something happened.
//!
//! The moments themselves are not in the vocabulary, and must not be. A script
//! supplies the answers and the app decides when to ask. A harness that named
//! the moments would be comparing two implementations against its own
//! timetable rather than against each other.

use crate::app::{App, Contents, Row};
use serde_json::{Value, json};

/// One frame's decisions.
///
/// Keys come back sorted, because `serde_json` builds its objects on a
/// `BTreeMap` and Go's `encoding/json` sorts map keys too. Neither side has to
/// do anything to agree on the order.
pub fn snapshot(app: &App, polling: Option<crate::app::Poll>) -> Value {
    let mut out = json!({
        "screen": screen(app),
        "now": app.now(),
        "selected": app.state.selected(),
        "filter": app.screen.typed(),
        "feed": feed(app, polling),
        "weather": app.weather(),
        "detour": app.route_alert(),
    });

    // A screen lists rows or shows a board, never both, so the snapshot carries
    // one or the other. Mirrors `Contents`, which is one field for the same
    // reason: two that could disagree would need keeping in step.
    let (key, value) = match &app.contents {
        Contents::List(rows) => ("rows", Value::Array(rows.iter().map(row).collect())),
        Contents::Board(deps) => (
            "departures",
            Value::Array(deps.iter().map(|d| departure(d, app)).collect()),
        ),
    };
    out[key] = value;

    // Only a board can be pinned, and only there does the key say so.
    if let Some(state) = app.board_pin() {
        out["pinned"] = json!(format!("{state:?}").to_lowercase());
    }
    // Only the first screen draws the pins, so only there is their absence a
    // fact. Reporting an empty list from a board said nothing was pinned when
    // the truth was that pins are not on this screen.
    if let Contents::List(rows) = &app.contents {
        out["pins"] = pins(rows);
    }
    out
}

/// The realtime feed: what the status bar says about it, and what the app's
/// own cadence has done where a fixture drives one.
///
/// One name rather than two, because the note and the schedule are two views of
/// the same subject and a reader chasing a cadence difference wants them side
/// by side. A fixture that names no outcomes gets the note alone: it has no
/// cadence to compare, and inventing zeros would read as "asked nothing, failed
/// nothing" rather than "nobody was asking".
fn feed(app: &App, polling: Option<crate::app::Poll>) -> Value {
    let note = json!(app.rt_note());
    let Some(p) = polling else {
        return json!({ "note": note });
    };
    json!({
        "note": note,
        "requests": p.requests(),
        "failures": p.failures(),
        "due_in": p.due_in(app.epoch()),
    })
}

/// Which screen, as the one word a person would use for it.
fn screen(app: &App) -> &'static str {
    use crate::app::Screen;
    match app.screen {
        Screen::Mode => "mode",
        Screen::Search { .. } => "search",
        Screen::Routes { .. } => "routes",
        Screen::Directions { .. } => "directions",
        Screen::Stops { .. } => "stops",
        Screen::Departures(_) => "departures",
    }
}

/// A listed row: the two labels it draws with.
///
/// Not the value behind them. A route row stands for a `Route`, which carries
/// ids, a colour and a sort order that no one reads off the screen.
fn row(r: &Row) -> Value {
    json!({ "primary": r.primary(), "secondary": r.secondary() })
}

/// A departure, with each derivation the board makes from it.
///
/// `scheduled` is the timetable. `live` is the prediction projected onto the
/// same axis, `wait` the countdown drawn in the wait column, and `late` the
/// signed minutes in the note beside it. All three have been wrong before, and
/// each is wrong in its own way: `live` across a fall-back hour, `wait` and
/// `late` across midnight, where naive subtraction reads five minutes late as
/// twenty-four hours early.
fn departure(d: &crate::db::Departure, app: &App) -> Value {
    json!({
        "route": d.route_short,
        "headsign": d.headsign,
        "scheduled": d.secs,
        "live": d.live,
        "wait": crate::app::mins_until(d.when(), app.now()),
        "late": d.live.map(|live| crate::app::lateness(live, d.secs)),
        "cancelled": d.canceled,
        "after_midnight": d.after_midnight,
    })
}

/// The pinned boards, in the order they were pinned.
///
/// Identity, not appearance: a pin is a stop plus the route and direction
/// narrowing it, and the first screen draws only the stop's name. Two routes to
/// one terminus are not two routes to one place, which is the whole reason the
/// route is in here.
fn pins(rows: &[Row]) -> Value {
    Value::Array(
        rows.iter()
            .filter_map(|r| match r {
                Row::Pin(p) => {
                    let (stop, route, headsign) = p.board.key();
                    Some(json!({ "stop": stop, "route": route, "headsign": headsign }))
                }
                _ => None,
            })
            .collect(),
    )
}
