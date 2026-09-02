//! Tests for the frame: what each screen renders, how tall it asks to be,
//! where the two-group screens put their groups, and that the columns line
//! up. Split from `mod.rs` the way `app/tests.rs` was, and for the same
//! reason: the module was 52% test by line and neither half was scannable.

use super::*;
use crate::app::App;
use crate::testing::TestGtfs;
use chrono::NaiveDate;
use ratatui::{Terminal, backend::TestBackend};

/// Render one frame and return it as plain text rows.
fn frame(app: &mut App, w: u16, h: u16) -> Vec<String> {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| draw(f, app)).unwrap();
    let buf = term.backend().buffer().clone();
    (0..h)
        .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect()
}

/// Render one frame and return every cell's (foreground, background).
fn colours(app: &mut App, w: u16, h: u16) -> Vec<Vec<(Color, Color)>> {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| draw(f, app)).unwrap();
    let buf = term.backend().buffer().clone();
    (0..h)
        .map(|y| {
            (0..w)
                .map(|x| (buf[(x, y)].fg, buf[(x, y)].bg))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// A small network: one busy stop with more departures than can be shown.
fn pinned_app(dir: &std::path::Path) -> App {
    let g = TestGtfs::new()
        .route("5", "5", 3, "0057B8")
        .always("A")
        .trip("t5", "5", "A", "Elmvale")
        .stop("S1", "1902", "BANK / SOMERSET W")
        .stop_time("t5", "S1", 1, "10:00:00");
    App::offline(
        g.into_conn(),
        NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
        9 * 3600,
        Some(dir.join("pins")),
    )
    .unwrap()
}

fn busy_app() -> App {
    let mut g = TestGtfs::new()
        .route("5", "5", 3, "0057B8")
        .route("7", "7", 3, "6D6E70")
        .always("A")
        .stop("S1", "1902", "BANK / SOMERSET W");
    for i in 0..20 {
        let trip = format!("t{i}");
        g = g
            .trip(&trip, if i % 2 == 0 { "5" } else { "7" }, "A", "Elmvale")
            .stop_time(&trip, "S1", 1, &format!("10:{i:02}:00"));
    }
    let conn = g.into_conn();
    App::offline(
        conn,
        NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
        9 * 3600,
        None,
    )
    .unwrap()
}

#[test]
fn long_text_is_truncated_by_us_not_silently_clipped_by_the_terminal() {
    // ratatui clips at the buffer edge, so "every row is exactly w wide"
    // is true no matter what we do. The real question is whether *we*
    // shortened the text — an ellipsis is the evidence that we did.
    let conn = TestGtfs::new()
        .route_named(
            "111",
            "111",
            "Billings Bridge / Carleton <> Baseline via Somewhere",
            3,
            "x",
        )
        .always("A")
        .trip("t1", "111", "A", "Baseline")
        .stop("s1", "0001", "SOMEWHERE")
        .stop_time("t1", "s1", 1, "10:00:00")
        .into_conn();
    let mut app = App::offline(
        conn,
        NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
        9 * 3600,
        None,
    )
    .unwrap();
    app.enter().unwrap(); // Bus -> the route list, where long_name is the
    // secondary column

    let rows = frame(&mut app, 46, VIEWPORT_H);
    assert!(
        rows.iter().any(|r| r.contains('…')),
        "a long label at 46 columns should be visibly shortened:\n{rows:#?}"
    );
}

#[test]
fn the_panel_never_asks_for_more_rows_than_the_viewport_reserves() {
    // The departures branch was once unclamped: we fetched ten rows into a
    // body that could show eight, and the panel asked for a taller area
    // than the inline viewport had.
    //
    // BOARD_FETCH caps what is queried and BOARD_LIMIT caps what is drawn,
    // so reaching this through the UI can't exercise the clamp. Overfill
    // the board directly — the clamp is the backstop for exactly the case
    // where some other path does that.
    let mut app = busy_app();
    app.screen = Screen::Departures(Board::Stop {
        stop: crate::db::StopRow {
            stop_id: "S1".into(),
            code: "1902".into(),
            name: "BANK / SOMERSET W".into(),
        },
    });
    app.contents = crate::app::Contents::Board(
        (0..40)
            .map(|i| crate::db::Departure {
                secs: i * 60,
                trip_id: format!("t{i}"),
                route_short: "5".into(),
                route_color: "x".into(),
                headsign: "h".into(),
                after_midnight: false,
                live: None,
                canceled: false,
            })
            .collect(),
    );
    assert!(
        desired_height(&app) <= VIEWPORT_H,
        "40 departures asked for {} rows, viewport is {VIEWPORT_H}",
        desired_height(&app)
    );
}

#[test]
fn every_screen_fits_the_viewport_when_reached_through_the_ui() {
    let mut app = busy_app();
    for step in ["mode", "routes", "directions", "stops", "departures"] {
        assert!(desired_height(&app) <= VIEWPORT_H, "{step}");
        app.enter().unwrap();
    }
}

#[test]
fn a_long_headline_wraps_instead_of_losing_its_ending() {
    // The feed's titles run to 101 columns because they enumerate the
    // routes. Truncating puts the ellipsis exactly over the part that says
    // what is happening.
    let t = "Detour: Routes 57, 61, 62, 63, 66, 67, 88, 158, 256, 301, 303, 454 \
             during Bayshore Transitway closure";
    let lines = wrap(t, 70, 2);
    assert_eq!(lines.len(), 2, "{lines:#?}");
    assert!(
        lines[1].contains("Bayshore Transitway closure"),
        "the ending was lost: {lines:#?}"
    );
    for l in &lines {
        assert!(l.chars().count() <= 70, "over the width: {l:?}");
    }
}

#[test]
fn a_headline_too_long_even_wrapped_says_it_was_cut() {
    // Two lines is the budget. Running past it silently would read as a
    // sentence that simply stops.
    let lines = wrap(&"word ".repeat(80), 40, 2);
    assert_eq!(lines.len(), 2);
    assert!(lines[1].ends_with('…'), "no sign it was cut: {lines:#?}");
}

#[test]
fn a_short_headline_takes_one_line_and_no_ellipsis() {
    let lines = wrap("Detour: Cheo Roadway closure", 70, 2);
    assert_eq!(lines, vec!["Detour: Cheo Roadway closure"]);
}

#[test]
fn a_word_longer_than_the_width_is_cut_to_the_width() {
    // Nothing wraps a word that has no space in it, so the line it starts
    // has to be bounded on its own. The `Paragraph` does not wrap, so an
    // over-long line is one ratatui clips at the edge without a mark.
    let lines = wrap("Temporary Reconfiguration", 13, 2);
    for l in &lines {
        assert!(l.chars().count() <= 13, "over the width: {l:?}");
    }
    assert!(lines[1].ends_with('…'), "no sign it was cut: {lines:#?}");
}

/// A directions screen for a route the feed has published a detour about.
fn app_with_a_detour(headline: &str) -> App {
    // Three ways in, so a test about rows being squeezed off has more than
    // one row to lose.
    let g = TestGtfs::new()
        .route("44", "44", 3, "0057B8")
        .always("S")
        .trip("t1", "44", "S", "Billings Bridge")
        .trip("t2", "44", "S", "Hurdman")
        .trip("t3", "44", "S", "Greenboro")
        .stop("S1", "3009", "BANK / RIVERSIDE")
        .stop_time("t1", "S1", 1, "10:00:00")
        .stop_time("t2", "S1", 1, "10:10:00")
        .stop_time("t3", "S1", 1, "10:20:00");
    let mut app = App::offline(
        g.into_conn(),
        NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
        9 * 3600,
        None,
    )
    .unwrap();
    app.set_alerts(crate::alerts::parse(&format!(
        "<item><category>Detours</category>\
         <category>affectedRoutes-44</category>\
         <title>{headline}</title></item>"
    )));
    app.enter().unwrap(); // Bus -> routes
    app.enter().unwrap(); // route 44 -> directions
    app
}

#[test]
fn a_detour_takes_the_top_and_leaves_the_choices_at_the_bottom() {
    // The same two-group shape the pinned first screen uses: the thing you
    // are being told at the top, the thing you are choosing at the bottom,
    // and a gap saying they are different kinds of thing.
    let mut app = app_with_a_detour("Detour: Route 44 during Terminal Avenue bridge closure");

    let shown = frame(&mut app, 74, VIEWPORT_H);
    let warn = shown.iter().position(|r| r.contains('⚠')).unwrap();
    let first = shown.iter().position(|r| r.contains("toward")).unwrap();
    let last = shown.iter().rposition(|r| r.contains("toward")).unwrap();
    let rule = shown.iter().position(|r| r.starts_with('─')).unwrap();
    assert_eq!(warn, 0, "the detour is not at the top: {shown:#?}");
    assert_eq!(last, rule - 1, "the choices left the bottom: {shown:#?}");
    assert!(
        shown[warn + 1..first].iter().all(|r| r.trim().is_empty()),
        "no gap between the two groups: {shown:#?}"
    );
}

#[test]
fn a_detour_does_not_push_a_direction_off_the_screen() {
    // The message takes rows from a viewport that was already sized for
    // the list. Every way into the route still has to be selectable.
    let mut app = app_with_a_detour(
        "Detour: Routes 57, 61, 62, 63, 66, 67, 88, 158, 256, 301, 303, 454 \
         during Bayshore Transitway closure",
    );
    let ways = app.rows().len();

    let shown = frame(&mut app, 74, VIEWPORT_H);
    let drawn = shown.iter().filter(|r| r.contains("toward")).count();
    assert_eq!(drawn, ways, "a direction was pushed off: {shown:#?}");
}

/// A pinned stop whose next bus the feed has cancelled, and the directory the
/// pins file lives in.
///
/// The directory comes back with the app because dropping it deletes the file.
/// It is a fresh one per call rather than a shared name: three tests used one
/// path and raced, one thread's cleanup deleting the file another was still
/// writing.
///
/// The bus is four minutes out. That is the amber band, and a cancellation is
/// dim grey — at sixty minutes both are dim and a test could not tell them
/// apart.
fn app_with_a_cancelled_pin() -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("pins"), "s1\t0001\tBANK / SOMERSET W\n").unwrap();
    let g = TestGtfs::new()
        .route("5", "5", 3, "0057B8")
        .always("A")
        .trip("t5", "5", "A", "Elmvale")
        .stop("s1", "0001", "BANK / SOMERSET W")
        .stop_time("t5", "s1", 1, "09:04:00");
    let mut app = App::offline(
        g.into_conn(),
        NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
        9 * 3600,
        Some(dir.path().join("pins")),
    )
    .unwrap();

    // ScheduleRelationship 3 with no stop list, which is what the live feed
    // sends. It needs no epoch, so this test needs no clock arithmetic.
    let payload = crate::testing::TestRt::new(0).canceled("t5").build();
    *app.rt.lock().unwrap() = crate::app::RtState::Ready(crate::rt::parse(&payload).unwrap());
    app.apply_realtime();
    (app, dir)
}

/// The row a pinned stop draws, which must exist before anything is asserted
/// about it. A screen with no pin on it satisfies every claim below by
/// drawing nothing.
fn pin_row(shown: &[String]) -> &String {
    shown
        .iter()
        .find(|r| r.contains("BANK"))
        .expect("no pin row: the fixture did not load")
}

#[test]
fn a_cancelled_pin_shows_no_countdown() {
    // The board replaces the countdown with an em dash, because a bus that is
    // not coming has no "in 4 minutes" to give. A pin is a board one row long
    // and was still printing the number.
    let (mut app, _dir) = app_with_a_cancelled_pin();

    let shown = frame(&mut app, 74, VIEWPORT_H);
    let pin = pin_row(&shown);
    // The em dash is enough, and is all this level should claim. Nothing else
    // on the row draws one, so its presence proves `pin_line` asks `wait`.
    // What `wait` then returns is settled in `palette`, against the function
    // itself, where the assertion needs no fixture clock and no column
    // arithmetic to go wrong.
    assert!(pin.contains('\u{2014}'), "no em dash: {pin:?}");
}

#[test]
fn a_cancelled_pin_does_not_wear_an_urgency_colour() {
    // Amber is the palette's "off-nominal but not wrong", and beside the word
    // "cancelled" it reads as a bus you can still catch. The colour has to
    // agree with the text, because on this board the colour is read first.
    let (mut app, _dir) = app_with_a_cancelled_pin();

    let shown = frame(&mut app, 74, VIEWPORT_H);
    let y = shown
        .iter()
        .position(|r| r.contains("BANK"))
        .expect("no pin row: the fixture did not load");
    let rows = colours(&mut app, 74, VIEWPORT_H);
    assert!(
        rows[y].iter().all(|(fg, _)| *fg != super::palette::AMBER),
        "the countdown is still amber next to a cancelled bus"
    );
}

#[test]
fn a_cancelled_departure_shows_no_countdown_on_the_board_either() {
    // The board has drawn the em dash since before the pin existed, and no
    // test covered it. Both now ask one function, so a regression would take
    // the board and the pin together.
    let (mut app, _dir) = app_with_a_cancelled_pin();
    // The pin by what it is, not by where the cursor happens to start. A fixed
    // row 0 is what sent both dev tools into a board they then mislabelled.
    let pin = app
        .rows()
        .iter()
        .position(|r| matches!(r, Row::Pin(_)))
        .expect("no pin row");
    app.state.select(Some(pin));
    app.enter().unwrap();

    let shown = frame(&mut app, 74, VIEWPORT_H);
    // The clock time is the column a pin does not have, so finding it proves
    // this is the board and not the row we came from.
    let row = shown
        .iter()
        .find(|r| r.contains("09:04"))
        .expect("not on the board");
    assert!(row.contains('\u{2014}'), "no em dash: {row:?}");
}

#[test]
fn the_detour_is_amber_and_the_choices_below_it_are_not() {
    // It has to read as a warning at a glance, and the rows under it have to
    // keep reading as a list of choices. Both screens it appears on have no
    // wait column, so this is the only amber on them.
    let mut app = app_with_a_detour("Detour: Route 44 during Terminal Avenue bridge closure");

    let rows = colours(&mut app, 74, VIEWPORT_H);
    let warn = &rows[0];
    assert!(
        warn.iter().any(|(fg, _)| *fg == super::palette::AMBER),
        "the detour is not amber"
    );
    assert!(
        rows[1..]
            .iter()
            .all(|r| r.iter().all(|(fg, _)| *fg != super::palette::AMBER)),
        "amber leaked onto the choices below"
    );
}

#[test]
fn the_detour_stays_up_while_you_pick_a_stop() {
    // The stops screen still sits under exactly one route -- it draws the
    // route's gutter -- and it is the screen where a detour decides which
    // way you walk.
    let mut app = app_with_a_detour("Detour: Route 44 during Terminal Avenue bridge closure");
    app.enter().unwrap(); // a direction -> its stops

    let shown = frame(&mut app, 74, VIEWPORT_H);
    assert!(
        shown.iter().any(|r| r.contains('⚠')),
        "the detour vanished on the stops screen: {shown:#?}"
    );
}

#[test]
fn a_pinned_first_screen_puts_the_answers_at_the_top_and_the_ways_in_at_the_bottom() {
    // Two groups with a gap: the pins sit against the ground line the logo
    // stands on, the modes stay just above the rule. The viewport reserves
    // these rows on every screen anyway, so the top of it is free.
    let dir = std::env::temp_dir().join("otransit-ui-menu");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("pins"), "S1\t1902\tBANK / SOMERSET W\n").unwrap();
    let mut app = pinned_app(&dir);

    let shown = frame(&mut app, 74, VIEWPORT_H);
    let first = shown.iter().position(|r| r.contains("BANK")).unwrap();
    let modes = shown.iter().position(|r| r.contains("Bus")).unwrap();
    let rule = shown.iter().position(|r| r.starts_with('─')).unwrap();
    assert_eq!(first, 0, "the pin is not at the top: {shown:#?}");
    assert_eq!(modes, rule - 2, "the modes left the bottom: {shown:#?}");
    assert!(
        shown[first + 1..modes].iter().all(|r| r.trim().is_empty()),
        "no gap between the two groups: {shown:#?}"
    );
}

#[test]
fn a_pin_says_which_way_its_bus_is_going() {
    // Two platforms of one station share a pole number and a route number
    // and go opposite ways. Without the direction the rows read the same
    // and one of them sends you backwards.
    let dir = std::env::temp_dir().join("otransit-ui-toward");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("pins"), "A\t3021\tUOTTAWA A\nB\t3021\tUOTTAWA B\n").unwrap();
    let g = TestGtfs::new()
        .route("56", "56", 3, "0057B8")
        .always("S")
        .trip("out", "56", "S", "Tunney's Pasture")
        .trip("back", "56", "S", "King Edward")
        .stop("A", "3021", "UOTTAWA A")
        .stop("B", "3021", "UOTTAWA B")
        .stop_time("out", "A", 1, "10:00:00")
        .stop_time("back", "B", 1, "10:05:00");
    let mut app = App::offline(
        g.into_conn(),
        NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
        9 * 3600,
        Some(dir.join("pins")),
    )
    .unwrap();

    let shown = frame(&mut app, 80, VIEWPORT_H);
    let pins: Vec<&String> = shown.iter().filter(|r| r.contains("UOTTAWA")).collect();
    assert_eq!(pins.len(), 2, "{shown:#?}");
    assert!(
        pins[0].contains("Tunney"),
        "no direction on the first: {}",
        pins[0]
    );
    assert!(
        pins[1].contains("King Edward"),
        "no direction on the second: {}",
        pins[1]
    );
    assert_ne!(
        pins[0].trim_start_matches([' ', '❯']),
        pins[1].trim_start_matches([' ', '❯']),
        "two opposite directions render identically"
    );
}

#[test]
fn a_pin_row_stays_inside_a_narrow_terminal() {
    // Six columns is as many as fit. The proposal this replaced carried the
    // mode, the word "toward" and a clock time as well, and ran 96 cells.
    let dir = std::env::temp_dir().join("otransit-ui-width");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("pins"), "S1\t1902\tBANK / SOMERSET W\n").unwrap();
    let mut app = pinned_app(&dir);
    for w in [60u16, 74, 80, 120] {
        for row in frame(&mut app, w, VIEWPORT_H) {
            assert!(
                row.chars().count() <= w as usize,
                "width {w}: row runs past the terminal: {row:?}"
            );
        }
    }
}

#[test]
fn one_cursor_runs_through_both_groups() {
    // Two widgets, one selection. If each group tracked its own the cursor
    // would appear twice, or vanish crossing the gap.
    let dir = std::env::temp_dir().join("otransit-ui-cursor");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("pins"), "S1\t1902\tBANK / SOMERSET W\n").unwrap();
    let mut app = pinned_app(&dir);

    for step in 0..3 {
        let shown = frame(&mut app, 74, VIEWPORT_H);
        let cursors = shown.iter().filter(|r| r.contains('❯')).count();
        assert_eq!(cursors, 1, "step {step} drew {cursors} cursors: {shown:#?}");
        app.move_by(1);
    }
}

#[test]
fn the_status_bar_is_the_last_row_on_every_screen() {
    let mut app = busy_app();
    for _ in 0..5 {
        let rows = frame(&mut app, 90, VIEWPORT_H);
        let last = rows.last().unwrap();
        assert!(
            last.contains('?') || last.contains("Departures"),
            "bottom row should be the status bar, got: {last:?}"
        );
        app.enter().unwrap();
    }
}

#[test]
fn the_rule_sits_directly_above_the_status_bar() {
    let mut app = busy_app();
    let rows = frame(&mut app, 90, VIEWPORT_H);
    let rule = &rows[rows.len() - 2];
    assert!(
        rule.chars().all(|c| c == '─'),
        "second-to-last row should be the rule, got: {rule:?}"
    );
}

#[test]
fn the_cursor_is_brand_red_and_the_rest_of_the_row_is_not() {
    // Colouring the marker through `highlight_style` would repaint the
    // whole selected line, and on this screen the grey pole number and dim
    // destination are the only things telling two sides of a corner apart.
    let mut app = busy_app();
    app.enter().unwrap(); // the route list, cursor on the first row
    let rows = colours(&mut app, 70, VIEWPORT_H);
    let cursor = rows
        .iter()
        .find(|r| r[1].0 == ACCENT)
        .unwrap_or_else(|| panic!("no red marker on any row"));
    assert!(
        cursor[3..].iter().all(|c| c.0 != ACCENT),
        "the accent leaked past the marker into the row"
    );
}

#[test]
fn the_marker_column_is_reserved_on_unselected_rows_too() {
    // The marker is part of the row now, so a blank one has to hold the
    // same three cells — otherwise every badge but the one under the
    // cursor steps three columns left.
    let mut app = busy_app();
    app.enter().unwrap();
    let starts: Vec<usize> = colours(&mut app, 70, VIEWPORT_H)
        .iter()
        .filter_map(|r| r.iter().position(|(_, bg)| *bg != Color::Reset))
        .collect();
    assert!(starts.len() > 1, "expected several badges, got {starts:?}");
    assert!(
        starts.windows(2).all(|w| w[0] == w[1]),
        "the badge column moves from row to row: {starts:?}"
    );
}

#[test]
fn a_capped_search_does_not_report_its_limit_as_a_total() {
    // search_stops takes SEARCH_LIMIT rows. Printing that as "25 found"
    // states a total the query never counted.
    let mut g = TestGtfs::new()
        .route("7", "7", 3, "6D6E70")
        .always("A")
        .trip("t1", "7", "A", "St-Laurent");
    for i in 0..SEARCH_LIMIT + 5 {
        let id = format!("s{i}");
        g = g
            .stop(&id, &format!("{i:04}"), &format!("RIDEAU / {i}"))
            .stop_time("t1", &id, i as i64 + 1, "10:00:00");
    }
    let mut app = App::offline(
        g.into_conn(),
        NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
        9 * 3600,
        None,
    )
    .unwrap();
    for c in "rideau".chars() {
        app.push_filter(c).unwrap();
    }
    assert_eq!(app.rows().len(), SEARCH_LIMIT, "the query is capped");
    let status = frame(&mut app, 100, VIEWPORT_H).pop().unwrap();
    assert!(
        status.contains(&format!("{SEARCH_LIMIT}+ found")),
        "the cap is reported as a total: {status:?}"
    );
}

#[test]
fn a_stop_board_keeps_its_last_column_inside_the_terminal() {
    // The headsign column is whatever is left after the fixed ones, so if
    // that accounting drifts from WAIT_W the row runs one cell long and
    // the terminal clips the status text on the right.
    let fixture = || {
        TestGtfs::new()
            .route("7", "7", 3, "6D6E70")
            .always("A")
            .trip("t1", "7", "A", "St-Laurent")
            .stop("s1", "0001", "RIDEAU / AUGUSTA")
            .stop_time("t1", "s1", 1, "10:00:00")
            .into_conn()
    };
    // Widths where the headsign column is not clamped, so the arithmetic
    // is actually load-bearing.
    for width in [56u16, 60, 64, 68] {
        let mut app = App::offline(
            fixture(),
            NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
            9 * 3600,
            None,
        )
        .unwrap();
        for c in "rideau".chars() {
            app.push_filter(c).unwrap();
        }
        app.enter().unwrap();
        let rows = frame(&mut app, width, VIEWPORT_H);
        let board = rows
            .iter()
            .find(|r| r.contains("10:00"))
            .unwrap_or_else(|| panic!("width {width}: no board row: {rows:#?}"));
        assert!(
            board.contains("sched"),
            "width {width}: the status column is clipped: {board:?}"
        );
    }
}

#[test]
fn a_multibyte_platform_does_not_shift_the_columns_beside_it() {
    // The platform field was reserved with `plat.len()` — bytes — while
    // every other width in this file counts chars. TESTING.md already
    // records one byte-vs-char defect in this exact column.
    let g = TestGtfs::new()
        .route("7", "7", 3, "6D6E70")
        .always("A")
        .trip("t1", "7", "A", "St-Laurent")
        .stop_on_platform("s1", "0001", "RIDEAU A", "A")
        .stop_on_platform("s2", "0002", "RIDEAU É", "É")
        .stop_time("t1", "s1", 1, "10:00:00")
        .stop_time("t1", "s2", 2, "10:05:00");
    let mut app = App::offline(
        g.into_conn(),
        NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
        9 * 3600,
        None,
    )
    .unwrap();
    for c in "rideau".chars() {
        app.push_filter(c).unwrap();
    }
    let cols: Vec<usize> = frame(&mut app, 100, VIEWPORT_H)
        .iter()
        .filter_map(|r| r.chars().position(|c| c == '#'))
        .collect();
    assert_eq!(cols.len(), 2, "both results should be visible");
    assert!(
        cols.windows(2).all(|w| w[0] == w[1]),
        "the accented platform shifted the code column: {cols:?}"
    );
}

#[test]
fn search_results_keep_their_columns_aligned_whatever_the_name_length() {
    let g = TestGtfs::new()
        .route("7", "7", 3, "x")
        .always("A")
        .trip("t1", "7", "A", "St-Laurent")
        .stop("s1", "0001", "RIDEAU")
        .stop("s2", "0002", "RIDEAU / A VERY LONG STREET NAME INDEED")
        .stop("s3", "0003", "RIDEAU / X")
        .stop_time("t1", "s1", 1, "10:00:00")
        .stop_time("t1", "s2", 2, "10:05:00")
        .stop_time("t1", "s3", 3, "10:10:00");
    let mut app = App::offline(
        g.into_conn(),
        NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
        9 * 3600,
        None,
    )
    .unwrap();
    for c in "rideau".chars() {
        app.push_filter(c).unwrap();
    }
    assert_eq!(app.rows().len(), 3, "fixture should match all three");

    let rows = frame(&mut app, 100, VIEWPORT_H);
    // Every result row shows a #code; they must all start at one column.
    // Count characters, not bytes: `❯` and `…` are three bytes each, so a
    // byte offset would report drift where the columns line up fine.
    let cols: Vec<usize> = rows
        .iter()
        .filter_map(|r| r.chars().position(|c| c == '#'))
        .collect();
    assert_eq!(cols.len(), 3, "three results should be visible");
    assert!(
        cols.windows(2).all(|w| w[0] == w[1]),
        "the code column drifts: {cols:?}"
    );
}
