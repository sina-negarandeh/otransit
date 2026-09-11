//! Tests for the drill-down itself: navigation, typing, pins on the first
//! screen, and what realtime does to a board once it lands.

use super::*;

use crate::testing::TestGtfs;

/// Two routes, two directions on one of them, three stops.
fn app() -> App {
    let g = TestGtfs::new()
        .route("5", "5", 3, "0057B8")
        .route("7", "7", 3, "6D6E70")
        .route("1", "1", 0, "D62408")
        .always("A")
        .trip("t5", "5", "A", "Elmvale")
        .trip("t5b", "5", "A", "Barrhaven")
        .trip("t7", "7", "A", "St-Laurent")
        .trip("t1", "1", "A", "Blair")
        .stop("s1", "0001", "BANK / SOMERSET W")
        .stop("s2", "0002", "BANK / GLADSTONE")
        .stop_time("t5", "s1", 1, "10:00:00")
        .stop_time("t5", "s2", 2, "10:05:00")
        .stop_time("t5b", "s1", 1, "11:00:00")
        .stop_time("t7", "s1", 1, "10:30:00")
        .stop_time("t1", "s2", 1, "10:45:00");
    App::offline(
        g.into_conn(),
        NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
        9 * 3600,
        None,
    )
    .unwrap()
}

/// The board on screen. Tests read it the way the renderer does.
fn deps(app: &App) -> &[crate::db::Departure] {
    app.contents.board().expect("this screen shows a board")
}

/// Just the scheduled times, for asserting on order.
fn board(app: &App) -> Vec<i32> {
    deps(app).iter().map(|d| d.secs).collect()
}

fn labels(app: &App) -> Vec<String> {
    app.rows().iter().map(Row::primary).collect()
}

/// The same fixture, with pins in a file the test owns.
///
/// A temp dir rather than the real config path: no test may read or write
/// the pins of whoever is running it.
fn app_with_pins(dir: &std::path::Path) -> App {
    let g = TestGtfs::new()
        .route("5", "5", 3, "0057B8")
        .always("A")
        .trip("t5", "5", "A", "Elmvale")
        .stop("s1", "0001", "BANK / SOMERSET W")
        .stop("s2", "0002", "BANK / GLADSTONE")
        .stop("s3", "0003", "BANK / LAURIER")
        .stop("s4", "0004", "BANK / SLATER")
        .stop("s5", "0005", "BANK / QUEEN")
        .stop("s6", "0006", "BANK / ALBERT")
        .stop("s7", "0007", "BANK / SPARKS")
        .stop_time("t5", "s1", 1, "10:00:00")
        .stop_time("t5", "s2", 2, "10:05:00")
        .stop_time("t5", "s3", 3, "10:10:00")
        .stop_time("t5", "s4", 4, "10:15:00")
        .stop_time("t5", "s5", 5, "10:20:00")
        .stop_time("t5", "s6", 6, "10:25:00")
        .stop_time("t5", "s7", 7, "10:30:00");
    App::offline(
        g.into_conn(),
        NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
        9 * 3600,
        Some(dir.join("pins")),
    )
    .unwrap()
}

/// Open the board for one stop, the only screen `p` works from.
fn open_stop(app: &mut App, stop_id: &str, code: &str, name: &str) {
    app.goto(Screen::Departures(Board::Stop {
        stop: StopRow {
            stop_id: stop_id.into(),
            code: code.into(),
            name: name.into(),
        },
    }))
    .unwrap();
}

fn tmp(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("otransit-pins-{label}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ---------- pins ----------

/// One platform, two routes, both ending at Billings Bridge, taking different
/// roads to get there.
///
/// This is the real shape of stop 7581: 44 and 48 share a headsign and do not
/// share a path — the 48 runs a Canterbury corridor the 44 never touches. Two
/// buses to the same terminus are not two buses to the same place, so the app
/// must not offer one when you pinned the other.
fn app_with_two_routes(dir: &std::path::Path) -> App {
    let g = TestGtfs::new()
        .route("44", "44", 3, "0057B8")
        .route("48", "48", 3, "D30F1D")
        .always("A")
        .trip("t44", "44", "A", "Billings Bridge")
        .trip("t48", "48", "A", "Billings Bridge")
        .stop("s1", "0001", "TRANSITWAY / TERMINAL")
        .stop("via44", "0044", "RIVERSIDE / SMYTH")
        .stop("via48", "0048", "CANTERBURY / ARCH")
        .stop_time("t44", "s1", 1, "10:00:00")
        .stop_time("t44", "via44", 2, "10:10:00")
        .stop_time("t48", "s1", 1, "10:04:00")
        .stop_time("t48", "via48", 2, "10:14:00");
    App::offline(
        g.into_conn(),
        NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
        9 * 3600,
        Some(dir.join("pins")),
    )
    .unwrap()
}

/// Drill Bus -> `route` -> its only direction -> the first stop, and stop there.
fn drill_to_board(app: &mut App, route: &str) {
    app.goto(Screen::Mode).unwrap();
    let bus = app
        .rows()
        .iter()
        .position(|r| matches!(r, Row::Mode(Mode::Bus, _)))
        .expect("no Bus row");
    app.state.select(Some(bus));
    app.enter().unwrap();
    let want = app
        .rows()
        .iter()
        .position(|r| r.primary() == route)
        .unwrap_or_else(|| panic!("route {route} not running"));
    app.state.select(Some(want));
    app.enter().unwrap(); // directions
    app.state.select(Some(0));
    app.enter().unwrap(); // stops
    app.state.select(Some(0));
    app.enter().unwrap(); // the board
}

#[test]
fn two_routes_from_one_stop_can_both_be_pinned() {
    // Both end at Billings Bridge and both call here, but they serve different
    // stops after it. Pinning one must not stand in for the other, and pinning
    // the second must not replace the first.
    let dir = tmp("two-routes");
    let mut app = app_with_two_routes(&dir);

    drill_to_board(&mut app, "44");
    app.toggle_pin();
    drill_to_board(&mut app, "48");
    app.toggle_pin();

    app.goto(Screen::Mode).unwrap();
    let pinned: Vec<String> = app
        .rows()
        .iter()
        .filter_map(|r| match r {
            Row::Pin(p) => Some(p.next()?.route_short.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(pinned, ["44", "48"], "both routes should hold a pin");
}

#[test]
fn a_pin_shows_only_the_route_it_was_made_from() {
    // The 48 leaves four minutes after the 44 from this platform. A pin on the
    // 44 that reported the 48 would be sending you to a bus that does not
    // serve the stop you are going to.
    let dir = tmp("one-route");
    let mut app = app_with_two_routes(&dir);

    drill_to_board(&mut app, "44");
    app.toggle_pin();

    app.goto(Screen::Mode).unwrap();
    let Some(Row::Pin(p)) = app.rows().first().cloned() else {
        panic!("no pin row")
    };
    let shown: Vec<&str> = p.upcoming.iter().map(|d| d.route_short.as_str()).collect();
    assert!(
        shown.iter().all(|r| *r == "44"),
        "the pin offered another route: {shown:?}"
    );
}

#[test]
fn entering_a_route_pin_opens_the_board_it_was_pinned_from() {
    // Not just the right departures -- the right screen. A pin that opened an
    // unfiltered stop board would still show the right rows, because the row
    // itself is already narrowed, while the breadcrumb, the route gutter and
    // the detour line all quietly changed to a different screen's answers.
    let dir = tmp("enter-route-pin");
    let mut app = app_with_two_routes(&dir);
    drill_to_board(&mut app, "44");
    app.toggle_pin();

    app.goto(Screen::Mode).unwrap();
    app.state.select(Some(0));
    app.enter().unwrap();

    let Screen::Departures(Board::Route {
        route, headsign, ..
    }) = &app.screen
    else {
        panic!("a route pin opened {:?}", app.screen)
    };
    assert_eq!(route.short_name, "44");
    assert_eq!(headsign, "Billings Bridge");
}

#[test]
fn back_from_a_pin_lands_where_you_jumped_from() {
    // A pin is the one jump in this app: it drops you under a route without
    // walking the drill path. Structural `back` then unwinds a path you never
    // took, so leaving a route pin cost four presses of esc and leaving a stop
    // pin cost one -- two behaviours for one gesture.
    let dir = tmp("back-from-pin");
    let mut app = app_with_two_routes(&dir);

    // A pin made by drilling, which lands on a route board.
    drill_to_board(&mut app, "44");
    app.toggle_pin();
    app.goto(Screen::Mode).unwrap();
    app.state.select(Some(0));
    app.enter().unwrap();
    app.back().unwrap();
    assert!(
        matches!(app.screen, Screen::Mode),
        "esc left a route pin on {:?}",
        app.screen
    );

    // And one made from a search board, which lands on a stop board. Both are
    // pins, so both answer esc the same way.
    open_stop(&mut app, "via44", "0044", "RIVERSIDE / SMYTH");
    app.toggle_pin();
    app.goto(Screen::Mode).unwrap();
    let stop_pin = app
        .rows()
        .iter()
        .position(|r| matches!(r, Row::Pin(p) if p.board.stop().stop_id == "via44"))
        .expect("no stop pin");
    app.state.select(Some(stop_pin));
    app.enter().unwrap();
    app.back().unwrap();
    assert!(
        matches!(app.screen, Screen::Mode),
        "esc left a stop pin on {:?}",
        app.screen
    );
}

#[test]
fn a_pin_is_still_scoped_to_its_route_after_a_restart() {
    // Pinning pushes the board straight into the live list, so a test that
    // pins and reads in one session never exercises the resolver. Only a
    // restart reads the route back out of the file and looks it up again --
    // which is where the `7`/`7-1` booking-period problem lives.
    let dir = tmp("scoped-restart");
    let mut app = app_with_two_routes(&dir);
    drill_to_board(&mut app, "44");
    app.toggle_pin();

    let fresh = app_with_two_routes(&dir);
    let Some(Row::Pin(p)) = fresh.rows().first().cloned() else {
        panic!("no pin row after a restart")
    };
    let shown: Vec<&str> = p.upcoming.iter().map(|d| d.route_short.as_str()).collect();
    assert!(
        shown.iter().all(|r| *r == "44"),
        "the pin came back unscoped: {shown:?}"
    );
}

#[test]
fn a_pin_survives_a_restart() {
    // The one piece of state the app keeps. If it does not outlive the
    // process it has bought nothing over drilling down again.
    let dir = tmp("restart");
    let mut app = app_with_pins(&dir);
    open_stop(&mut app, "s2", "0002", "BANK / GLADSTONE");
    app.toggle_pin();

    let fresh = app_with_pins(&dir);
    let first = fresh.rows().first().cloned();
    assert!(
        matches!(&first, Some(Row::Pin(p)) if p.board.stop().stop_id == "s2"),
        "the pin is not the first row after a restart: {first:?}"
    );
}

#[test]
fn pins_sit_above_the_modes_so_the_cursor_starts_on_one() {
    // The whole point: land on the answer, not on the first question.
    let dir = tmp("above");
    let mut app = app_with_pins(&dir);
    open_stop(&mut app, "s2", "0002", "BANK / GLADSTONE");
    app.toggle_pin();
    app.goto(Screen::Mode).unwrap();

    let kinds: Vec<&str> = app
        .rows()
        .iter()
        .map(|r| match r {
            Row::Pin(_) => "pin",
            Row::Mode(..) => "mode",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["pin", "mode", "mode"]);
    assert_eq!(app.state.selected(), Some(0), "cursor starts on the pin");
}

#[test]
fn pressing_p_twice_leaves_no_pin() {
    let dir = tmp("toggle");
    let mut app = app_with_pins(&dir);
    open_stop(&mut app, "s2", "0002", "BANK / GLADSTONE");
    app.toggle_pin();
    assert_eq!(app.board_pin(), Some(crate::pins::PinState::Pinned));
    app.toggle_pin();
    assert_eq!(app.board_pin(), Some(crate::pins::PinState::Unpinned));
    assert!(
        app_with_pins(&dir)
            .rows()
            .iter()
            .all(|r| !matches!(r, Row::Pin(_)))
    );
}

#[test]
fn a_pin_whose_stop_left_the_feed_is_hidden_but_not_forgotten() {
    // `update` replaces the whole cache, so a pinned stop can vanish
    // between exports. A row that cannot be opened is worse than no row --
    // and deleting the line would lose the pin to a stop that comes back.
    let dir = tmp("dangling");
    std::fs::write(
        dir.join("pins"),
        "gone	9999	RETIRED STOP
s2	0002	BANK / GLADSTONE
",
    )
    .unwrap();
    let app = app_with_pins(&dir);

    let pins: Vec<String> = app
        .rows()
        .iter()
        .filter_map(|r| match r {
            Row::Pin(p) => Some(p.board.stop().stop_id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(pins, ["s2"], "the missing stop is still on screen");
    assert!(
        std::fs::read_to_string(dir.join("pins"))
            .unwrap()
            .contains("gone"),
        "the line was deleted rather than hidden"
    );
}

#[test]
fn a_first_screen_left_open_refills_instead_of_draining() {
    // The same defect the board has an entry for. `refresh` asked
    // only the board, so pins kept a departure fetched at launch and the
    // screen slowly filled with buses that had already gone.
    let dir = tmp("drain");
    let g = TestGtfs::new()
        .route("5", "5", 3, "0057B8")
        .always("A")
        .trip("first", "5", "A", "Elmvale")
        .trip("second", "5", "A", "Elmvale")
        .stop("s1", "0001", "BANK / SOMERSET W")
        .stop_time("first", "s1", 1, "10:00:00")
        .stop_time("second", "s1", 1, "11:00:00");
    std::fs::write(dir.join("pins"), "s1\t0001\tBANK / SOMERSET W\n").unwrap();
    let mut app = App::offline(
        g.into_conn(),
        NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
        9 * 3600,
        Some(dir.join("pins")),
    )
    .unwrap();

    let front = |a: &App| match a.rows().first() {
        Some(Row::Pin(p)) => p.next().map(|d| d.trip_id.clone()),
        _ => None,
    };
    assert_eq!(front(&app).as_deref(), Some("first"));

    // Ten past ten: the first bus has gone.
    app.advance_to(10 * 3600 + 10 * 60);
    app.refresh().unwrap();
    assert_eq!(
        front(&app).as_deref(),
        Some("second"),
        "the pin is still showing a bus that left"
    );
}

#[test]
fn a_pin_the_cache_cannot_resolve_does_not_use_up_a_slot() {
    // Dangling pins are hidden but kept, so counting the file would report
    // "pins full" over a list with room in it and no way to see why.
    let dir = tmp("cap-vs-dangling");
    let g = TestGtfs::new()
        .route("5", "5", 3, "0057B8")
        .always("A")
        .trip("t5", "5", "A", "Elmvale")
        .stop("s1", "0001", "BANK / SOMERSET W")
        .stop_time("t5", "s1", 1, "10:00:00");
    let mut file = String::new();
    for i in 0..crate::ui::MAX_PINS {
        file.push_str(&format!("gone{i}\t0000\tRETIRED STOP\n"));
    }
    std::fs::write(dir.join("pins"), file).unwrap();
    let mut app = App::offline(
        g.into_conn(),
        NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
        9 * 3600,
        Some(dir.join("pins")),
    )
    .unwrap();

    assert!(
        app.rows().iter().all(|r| !matches!(r, Row::Pin(_))),
        "a dangling pin was drawn"
    );
    for _ in 0..4 {
        app.enter().unwrap();
    }
    assert_eq!(
        app.board_pin(),
        Some(crate::pins::PinState::Unpinned),
        "a full file of unresolvable pins blocked a real one"
    );
}

#[test]
fn a_pin_goes_live_when_the_fetch_lands() {
    // A pin shows the same number the board would. Without this it sits on
    // the timetable while the board beside it is current, which is the
    // silent-wrong-answer failure this app is most prone to.
    let dir = tmp("live");
    let g = TestGtfs::new()
        .route("5", "5", 3, "0057B8")
        .always("A")
        .trip("t5", "5", "A", "Elmvale")
        .stop("s1", "0001", "BANK / SOMERSET W")
        .stop_time("t5", "s1", 1, "10:00:00");
    std::fs::write(dir.join("pins"), "s1\t0001\tBANK / SOMERSET W\n").unwrap();
    let date = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
    let mut app = App::offline(g.into_conn(), date, 9 * 3600, Some(dir.join("pins"))).unwrap();

    // Scheduled 10:00, predicted 10:07. Stated through the app's own clock:
    // built from `Local` instead, this asserted about the machine's zone and
    // agreed with the app only where the suite happened to run.
    let late = app.epoch_of(10 * 3600 + 7 * 60);
    let payload = crate::testing::TestRt::new(0)
        .arrival("t5", "s1", late)
        .build();
    *app.rt.lock().unwrap() = RtState::Ready(crate::rt::parse(&payload).unwrap());
    app.apply_realtime();

    let Some(Row::Pin(p)) = app.rows().first().cloned() else {
        panic!("no pin row")
    };
    assert_eq!(
        p.next().and_then(|d| d.live),
        Some(10 * 3600 + 7 * 60),
        "the pin is still showing the timetable"
    );
}

#[test]
fn a_pin_shows_the_bus_that_arrives_first_not_the_one_scheduled_first() {
    // Built wrong first: the pin fetched LIMIT 1, which is earliest by
    // timetable, and showed a bus the board did not have at the top. The
    // board carries a test for this exact defect already; a pin is a
    // one-row board and needs the same headroom and the same re-sort.
    //
    // Driven through `apply_realtime` rather than sorting by hand, because
    // sorting by hand tests a copy of the path instead of the path.
    let dir = tmp("overtake");
    let g = TestGtfs::new()
        .route("5", "5", 3, "0057B8")
        .route("7", "7", 3, "6D6E70")
        .always("A")
        .trip("early", "5", "A", "Elmvale")
        .trip("later", "7", "A", "St-Laurent")
        .stop("s1", "0001", "BANK / SOMERSET W")
        .stop_time("early", "s1", 1, "10:00:00")
        .stop_time("later", "s1", 1, "10:10:00");
    std::fs::write(dir.join("pins"), "s1\t0001\tBANK / SOMERSET W\n").unwrap();
    let date = NaiveDate::from_ymd_opt(2026, 8, 21).unwrap();
    let mut app = App::offline(g.into_conn(), date, 9 * 3600, Some(dir.join("pins"))).unwrap();

    // The 5 is scheduled first but running half an hour late, so the 7
    // scheduled ten minutes after it is what actually arrives first.
    let payload = crate::testing::TestRt::new(0)
        .arrival("early", "s1", app.epoch_of(10 * 3600 + 30 * 60))
        .arrival("later", "s1", app.epoch_of(10 * 3600 + 10 * 60))
        .build();
    *app.rt.lock().unwrap() = RtState::Ready(crate::rt::parse(&payload).unwrap());
    app.apply_realtime();

    let Some(Row::Pin(p)) = app.rows().first().cloned() else {
        panic!("no pin row")
    };
    assert_eq!(
        p.next().map(|d| d.route_short.clone()),
        Some("7".to_string()),
        "the pin showed the scheduled-earliest bus, not the next one: {:?}",
        p.upcoming
            .iter()
            .map(|d| (&d.route_short, d.live))
            .collect::<Vec<_>>()
    );
}

#[test]
fn pins_keep_the_order_they_were_pinned_in_not_the_cache_s() {
    // Where a pin sits is muscle memory, and SQL has no order to give
    // back: `IN (...)` returns rows in whatever order it likes. Pinning
    // s3 then s1 must not come back as s1 then s3.
    let dir = tmp("order");
    let mut app = app_with_pins(&dir);
    for (id, name) in [("s3", "BANK / LAURIER"), ("s1", "BANK / SOMERSET W")] {
        open_stop(&mut app, id, "0000", name);
        app.toggle_pin();
    }
    app.goto(Screen::Mode).unwrap();

    let order: Vec<String> = app
        .rows()
        .iter()
        .filter_map(|r| match r {
            Row::Pin(p) => Some(p.board.stop().stop_id.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(order, ["s3", "s1"], "the list reordered itself");
}

#[test]
fn the_list_never_grows_past_what_fits_on_screen() {
    // A pin list you have to scroll has lost the property that makes it
    // worth having, so the cap is the layout's, not a number picked here.
    let dir = tmp("cap");
    let mut app = app_with_pins(&dir);
    for (i, id) in ["s1", "s2", "s3", "s4", "s5", "s6", "s7"]
        .iter()
        .enumerate()
    {
        open_stop(&mut app, id, "0000", "X");
        app.toggle_pin();
        assert!(
            app.pins.live().len() <= crate::ui::MAX_PINS,
            "pin {i} pushed the list past the cap"
        );
    }
    assert_eq!(app.pins.live().len(), crate::ui::MAX_PINS);
    assert_eq!(
        app.board_pin(),
        Some(crate::pins::PinState::Full),
        "the last one was refused"
    );
}

#[test]
fn a_stop_reached_by_drilling_down_pins_the_stop_not_the_route() {
    // A drilled board is filtered to one route; the stop is the durable
    // half, and everything calling there is the better answer to whether
    // to leave now.
    let dir = tmp("drill");
    let mut app = app_with_pins(&dir);
    app.enter().unwrap(); // mode -> routes
    app.enter().unwrap(); // routes -> directions
    app.enter().unwrap(); // directions -> stops
    app.enter().unwrap(); // stops -> a Board::Route
    assert!(matches!(
        app.screen,
        Screen::Departures(Board::Route { .. })
    ));
    app.toggle_pin();

    app.goto(Screen::Mode).unwrap();
    assert!(
        matches!(app.rows().first(), Some(Row::Pin(_))),
        "pinning from a drilled board recorded nothing"
    );
}

#[test]
fn p_does_nothing_anywhere_but_a_board() {
    // Every other screen treats letters as filter input, so the toggle must
    // be inert there rather than quietly recording something.
    let dir = tmp("elsewhere");
    let mut app = app_with_pins(&dir);
    for step in ["mode", "routes", "directions", "stops"] {
        assert_eq!(app.board_pin(), None, "{step}");
        app.toggle_pin();
        assert!(app.pins.live().is_empty(), "{step} recorded a pin");
        app.enter().unwrap();
    }
}

// ---------- navigation ----------

#[test]
fn going_back_shows_the_same_rows_you_came_from() {
    // Each screen's rows used to be whatever a previous transition happened
    // to leave in a field. They are loaded from the screen itself now, so
    // arriving backwards has to produce exactly what arriving forwards did.
    let mut app = app();
    let mut down = vec![labels(&app)];
    for _ in 0..3 {
        app.enter().unwrap();
        down.push(labels(&app));
    }
    assert_eq!(
        down[1],
        vec!["5", "7"],
        "bus routes, sorted numerically: {down:?}"
    );

    let mut up = vec![labels(&app)];
    for _ in 0..3 {
        app.back().unwrap();
        up.push(labels(&app));
    }
    up.reverse();
    assert_eq!(up, down, "the way back does not match the way down");
}

#[test]
fn the_cursor_lands_on_the_row_you_chose_not_the_one_you_typed_past() {
    // Filtering renumbers the visible rows, so entering must act on the
    // row's payload rather than on the cursor's index into the full list.
    let mut app = app();
    app.enter().unwrap(); // routes
    app.push_filter('7').unwrap();
    assert_eq!(labels(&app), vec!["7"], "filtered to one route");
    app.enter().unwrap();
    assert_eq!(
        app.screen.route().map(|r| r.short_name.clone()),
        Some("7".to_string())
    );
}

#[test]
fn a_filter_matches_what_you_would_type_not_the_dim_detail() {
    // "toward Elmvale / 1 trips today": typing 1 must not match the count.
    let mut app = app();
    app.enter().unwrap(); // routes
    app.enter().unwrap(); // directions of route 5
    assert_eq!(labels(&app).len(), 2, "route 5 runs both ways");
    app.push_filter('1').unwrap();
    assert!(
        app.rows().is_empty(),
        "the trip count is not searchable: {:?}",
        labels(&app)
    );
}

#[test]
fn a_board_you_drilled_down_to_shows_only_that_route_and_direction() {
    // Which departures a board carries is decided by how it was reached.
    // The same stop, opened from search, is a different board (below).
    let mut app = app();
    for _ in 0..4 {
        app.enter().unwrap(); // bus -> route 5 -> a direction -> first stop
    }
    let Screen::Departures(Board::Route { headsign, stop, .. }) = &app.screen else {
        panic!("expected a drilled-down board, got {:?}", app.screen);
    };
    assert_eq!(stop.name, "BANK / SOMERSET W");
    // Route 7 also calls here and route 5 also runs the other way, so a
    // board that ignored how it was reached would carry three rows.
    assert_eq!(board(&app).len(), 1, "{:?}", board(&app));
    assert_eq!(deps(&app)[0].route_short, "5");
    assert_eq!(deps(&app)[0].headsign, *headsign);
}

#[test]
fn a_board_reached_by_search_shows_every_route_calling_there() {
    let mut app = app();
    for c in "somerset".chars() {
        app.push_filter(c).unwrap();
    }
    app.enter().unwrap();
    let mut trips: Vec<&str> = deps(&app).iter().map(|d| d.trip_id.as_str()).collect();
    trips.sort_unstable();
    assert_eq!(trips, vec!["t5", "t5b", "t7"], "the whole stop, both ways");
}

#[test]
fn the_first_screen_offers_bus_and_train_with_their_counts() {
    let app = app();
    assert_eq!(labels(&app), vec!["Bus", "O-Train"]);
    assert_eq!(app.rows()[0].secondary(), "2 routes running today");
    assert_eq!(
        app.rows()[1].secondary(),
        "1 lines · scheduled times only",
        "rail is counted separately"
    );
}

#[test]
fn a_feed_failure_is_reported_without_being_cut_at_a_url() {
    // The note used to split on ':' as well as newline, to turn
    // "http status: 401" into "http status" — which also threw away the
    // 401, and cut every transport error at the scheme of its URL.
    let app = app();
    *app.rt.lock().unwrap() =
        RtState::Failed("error sending request for url (https://api.example/x)".into());
    let note = app
        .rt_note()
        .expect("a failed feed still has something to say");
    assert!(
        note.contains("example"),
        "the note stops at the scheme: {note:?}"
    );
    assert!(note.starts_with("live: "), "{note:?}");
}

#[test]
fn a_feed_failure_note_keeps_only_its_first_line() {
    let app = app();
    *app.rt.lock().unwrap() = RtState::Failed("timed out\nwhile connecting".into());
    assert_eq!(app.rt_note().unwrap(), "live: timed out");
}

// ---------- typing ----------

#[test]
fn typing_at_the_first_screen_opens_the_search() {
    // The search is a screen, not a mode of the first one: it asks a
    // different question and lists a different thing.
    let mut app = app();
    assert_eq!(app.title(), "What are you taking?");
    app.push_filter('b').unwrap();
    assert!(
        matches!(app.screen, Screen::Search { .. }),
        "{:?}",
        app.screen
    );
    assert_eq!(app.title(), "Transit stops");
    assert_eq!(app.screen.typed(), "b");
    assert!(app.rows().iter().all(|r| matches!(r, Row::Hit(_))));
}

#[test]
fn deleting_the_last_of_the_query_leaves_the_search() {
    let mut app = app();
    app.push_filter('b').unwrap();
    assert!(app.pop_filter().unwrap());
    assert!(matches!(app.screen, Screen::Mode), "{:?}", app.screen);
    assert_eq!(labels(&app), vec!["Bus", "O-Train"]);
}

#[test]
fn a_filter_belongs_to_the_screen_that_took_it() {
    // The filter used to be one field on App, so it had to be cleared by
    // hand on every move. Each screen owns its own now.
    let mut app = app();
    app.enter().unwrap(); // routes
    app.push_filter('7').unwrap();
    assert_eq!(app.screen.typed(), "7");
    app.enter().unwrap(); // directions of route 7
    assert_eq!(app.screen.typed(), "", "the next screen inherited a filter");
    app.back().unwrap();
    assert_eq!(app.screen.typed(), "", "going back restored a stale filter");
    assert_eq!(labels(&app), vec!["5", "7"]);
}

#[test]
fn a_screen_shows_a_list_or_a_board_and_never_both() {
    let mut app = app();
    assert!(app.contents.board().is_none(), "the first screen is a list");
    for _ in 0..4 {
        app.enter().unwrap();
    }
    assert!(app.contents.board().is_some(), "a board");
    assert!(app.rows().is_empty(), "a board has no selectable rows");
    app.back().unwrap();
    assert!(app.contents.board().is_none(), "back to a list");
}

// ---------- the board over time ----------

#[test]
fn a_board_left_open_refills_as_departures_go() {
    // The board is queried once, and `tick` then moves the clock under it.
    // Without a refetch the visible rows are all buses that already left,
    // while the ones still to come sit unshown further down `deps`.
    let mut app = app();
    for _ in 0..4 {
        app.enter().unwrap(); // bus -> 5 -> a direction -> first stop
    }
    assert_eq!(deps(&app).len(), 1, "one departure on this route at s1");
    let gone = deps(&app)[0].secs;

    app.advance_to(gone + 60); // the bus has left
    app.refresh().unwrap();
    assert!(
        deps(&app).iter().all(|d| d.secs > app.now()),
        "a departed bus is still on the board: {:?}",
        board(&app)
    );
}

#[test]
fn a_board_is_left_alone_while_nothing_has_gone() {
    let mut app = app();
    for _ in 0..4 {
        app.enter().unwrap();
    }
    let before = board(&app);
    app.refresh().unwrap();
    assert_eq!(
        before,
        board(&app),
        "nothing has left, so nothing should have been re-queried"
    );
}

#[test]
fn typing_on_a_board_does_not_shadow_quit_and_back() {
    // A board has no list to narrow, so a keystroke there only had the
    // effect of making `typing` true, which disabled q and turned esc into
    // "clear the filter" with nothing to clear.
    let mut app = app();
    for _ in 0..4 {
        app.enter().unwrap();
    }
    app.push_filter('a').unwrap();
    assert!(
        app.screen.typed().is_empty(),
        "a board swallowed a keystroke"
    );
}

#[test]
fn a_failed_step_back_leaves_you_where_you_were() {
    // back() used to move the screen out before loading the one behind it,
    // so a query failure dropped the real screen and left Mode in its
    // place. Nothing observes that today only because the error exits.
    let mut app = app();
    for _ in 0..4 {
        app.enter().unwrap();
    }
    let before = format!("{:?}", app.screen);
    app.conn.execute("DROP TABLE stop_times", []).unwrap();
    assert!(app.back().is_err(), "the load should have failed");
    assert_eq!(
        format!("{:?}", app.screen),
        before,
        "a failed load stranded the app on another screen"
    );
}

// ---------- stop names ----------

#[test]
fn the_otrain_direction_suffix_is_dropped_from_station_names() {
    assert_eq!(tidy_stop_name("RIDEAU O-TRAIN EAST / EST"), "RIDEAU");
    assert_eq!(tidy_stop_name("BAYVIEW A"), "BAYVIEW A", "left alone");
}
