//! Development helpers. Neither is part of the app proper: `dump` walks the
//! query path headlessly, `screenshot` renders every screen as plain text via
//! ratatui's TestBackend so the UI can be checked without a terminal.

use crate::app::{self, App};
use anyhow::Result;
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer, style::Color};

/// Walk Mode -> Route -> Direction -> Stop -> Departures for one route,
/// printing what each level returns.
///
/// Drives the real `App` rather than re-querying: the drill-down should have
/// one implementation, and this is worth nothing as a diagnostic if it walks a
/// different path from the one the browser walks.
pub fn dump(app: &mut App, want: &str, stop_query: Option<&str>) -> Result<()> {
    app.block_on_realtime(30);
    println!(
        "realtime: {}",
        app.rt_note().unwrap_or_else(|| "n/a".into())
    );
    println!("service date {}", app.service_date);

    let shot = Shot {
        route: Some(want),
        stop: stop_query,
        ..Shot::default()
    };
    for mode in [app::Mode::Bus, app::Mode::Train] {
        app.focus_search()?;
        let label = mode.label().to_uppercase();
        let Some(row) = mode_row(app, mode) else {
            continue;
        };
        app.state.select(Some(row));
        app.enter()?;

        let routes = app.rows();
        println!("\n{label} routes running today: {}", routes.len());
        let names: Vec<String> = routes.iter().take(18).map(app::Row::primary).collect();
        println!("  first: {}", names.join(", "));

        // Each level: aim at what the caller asked for, then step into it.
        let Some(i) = aim(app, &shot)? else { continue };
        app.state.select(Some(i));
        let long = app.rows()[i].secondary();
        app.enter()?;
        let ids = app
            .screen
            .route()
            .map(|r| r.route_ids.clone())
            .unwrap_or_default();
        println!("\n  route {want}: {long}  (route_ids: {ids:?})");

        for d in app.rows().iter().take(4) {
            println!("    {:<34} {}", d.primary(), d.secondary());
        }
        let Some(i) = aim(app, &shot)?.or(Some(0)) else {
            continue;
        };
        app.state.select(Some(i));
        let toward = app.rows()[i].primary();
        app.enter()?;

        let stops = app.rows();
        println!("\n    {} stops {toward}", stops.len());
        let Some(i) = aim(app, &shot)?.or(Some(stops.len() / 3)) else {
            continue;
        };
        app.state.select(Some(i));
        let here = stops[i].primary();
        app.enter()?;

        println!(
            "\n    next departures at {here}, now {}:",
            app::fmt_hm(app.now())
        );
        let board = app.contents.board().unwrap_or_default();
        if board.is_empty() {
            println!("      (none left today)");
        }
        for d in board {
            let status = if d.canceled {
                "CANCELLED".to_string()
            } else if let Some(live) = d.live {
                match app::lateness(live, d.secs) {
                    -1..=1 => "on time".into(),
                    l if l > 0 => format!("{l} late"),
                    l => format!("{} early", -l),
                }
            } else {
                "sched".into()
            };
            println!(
                "      sched {}  ->  {}  in {:>3} min   {:<10}{}",
                app::fmt_hm(d.secs),
                app::fmt_hm(d.when()),
                app::mins_until(d.when(), app.now()),
                status,
                if d.after_midnight {
                    " [after midnight]"
                } else {
                    ""
                }
            );
        }
    }
    Ok(())
}

/// What to render. Parsed from the command line, which is why it is a value
/// rather than six positional arguments.
#[derive(Default)]
pub struct Shot<'a> {
    pub w: u16,
    pub h: u16,
    pub train: bool,
    pub route: Option<&'a str>,
    pub stop: Option<&'a str>,
    pub search: Option<&'a str>,
}

/// Render each screen in turn and print the terminal buffer as text.
pub fn screenshot(app: &mut App, shot: &Shot) -> Result<()> {
    app.block_on_feeds(30);
    let mut term = Terminal::new(TestBackend::new(shot.w, shot.h))?;
    let mut frame = |app: &mut App, label: &str| -> Result<()> {
        // The event loop does this once a frame, so a screenshot that skipped
        // it would show the timetable where the app shows live times.
        app.apply_realtime();
        term.draw(|f| crate::ui::draw(f, app))?;
        println!("\n--- {label} ---");
        let buf = term.backend().buffer().clone();
        for y in 0..shot.h {
            println!("{}", render_line(&buf, y, shot.w));
        }
        Ok(())
    };

    // Search path: the results screen, then the stop board it leads to.
    if let Some(q) = shot.search {
        frame(app, "1. first screen")?;
        for c in q.chars() {
            app.push_filter(c)?;
        }
        frame(app, &format!("2. searching {q:?}"))?;
        app.enter()?;
        app.apply_realtime();
        return frame(app, "3. stop board");
    }

    // Drill-down path: one frame per level, entering between them.
    let steps = [
        "1. mode",
        "2. routes",
        "3. directions",
        "4. stops",
        "5. departures",
    ];
    for label in steps {
        let aimed = aim(app, shot)?;
        app.state.select(Some(aimed.unwrap_or(0)));
        frame(app, label)?;
        if !matches!(app.screen, app::Screen::Departures(_)) {
            app.enter()?;
        }
    }
    Ok(())
}

/// Which row a mode is on, on a first screen that may open with pins above it.
///
/// Not a fixed index. Pins sit above the modes, so a hardcoded 0 walks into
/// whatever the machine running this happens to have pinned.
fn mode_row(app: &App, want: app::Mode) -> Option<usize> {
    app.rows()
        .iter()
        .position(|r| matches!(r, app::Row::Mode(m, _) if *m == want))
}

/// Which row the caller asked for on this screen, if any.
fn aim(app: &mut App, shot: &Shot) -> Result<Option<usize>> {
    let lower = |s: &str| s.to_lowercase();
    match (&app.screen, shot.route, shot.stop) {
        (app::Screen::Mode, _, _) => Ok(mode_row(
            app,
            if shot.train {
                app::Mode::Train
            } else {
                app::Mode::Bus
            },
        )),
        (app::Screen::Routes { .. }, Some(want), _) => {
            Ok(app.rows().iter().position(|r| r.primary() == want))
        }
        (app::Screen::Stops { .. }, _, Some(want)) => {
            let want = lower(want);
            Ok(app
                .rows()
                .iter()
                .position(|r| lower(&r.primary()).contains(&want)))
        }
        // A direction is picked by which one actually serves the stop, so this
        // costs a query per direction rather than a label match.
        (app::Screen::Directions { .. }, _, Some(want)) => serves(app, &lower(want)),
        _ => Ok(None),
    }
}

/// Index of the first direction whose stop list contains `want`.
///
/// Steps into each direction and back out rather than querying around the app,
/// so it sees exactly the stops the browser would show.
fn serves(app: &mut App, want: &str) -> Result<Option<usize>> {
    for i in 0..app.rows().len() {
        app.state.select(Some(i));
        app.enter()?;
        let hit = app
            .rows()
            .iter()
            .any(|r| r.primary().to_lowercase().contains(want));
        app.back()?;
        if hit {
            return Ok(Some(i));
        }
    }
    Ok(None)
}

/// One row of the render buffer, with the styles turned back into escapes.
fn render_line(buf: &Buffer, y: u16, w: u16) -> String {
    let mut line = String::new();
    let (mut fg, mut bg) = (Color::Reset, Color::Reset);
    for x in 0..w {
        let cell = &buf[(x, y)];
        if cell.fg != fg || cell.bg != bg {
            line.push_str("\x1b[0m");
            line.push_str(&ansi(cell.fg, false));
            line.push_str(&ansi(cell.bg, true));
            fg = cell.fg;
            bg = cell.bg;
        }
        line.push_str(cell.symbol());
    }
    line.push_str("\x1b[0m");
    line
}

/// ratatui Color -> an ANSI truecolor escape, so `screenshot` shows real colours.
fn ansi(c: Color, background: bool) -> String {
    let base = if background { 48 } else { 38 };
    match c {
        Color::Rgb(r, g, b) => format!("\x1b[{base};2;{r};{g};{b}m"),
        // Everything else, Reset included, is "no escape": this tool only ever
        // renders the truecolour styles the UI actually uses.
        _ => String::new(),
    }
}

/// Report on the live realtime feed, through the parser the app actually uses.
///
/// The endpoint has `beta` in its URL, and a shape change is silent: `parse`
/// returns zero arrivals, every row reads `sched`, and that looks exactly like
/// "no buses are running". This answers the question that matters — does *our*
/// parser still cope — rather than describing the payload from scratch.
pub fn probe(app: &App) -> Result<()> {
    let Some(key) = crate::rt::find_key() else {
        anyhow::bail!("no subscription key; see .env.example");
    };

    let started = std::time::Instant::now();
    let bytes = crate::rt::fetch_raw(&key)?;
    let fetch_ms = started.elapsed().as_millis();
    println!(
        "transport   {:.0} KB in {fetch_ms} ms",
        bytes.len() as f64 / 1024.0
    );

    // Structural view first: if `parse` returns nothing, this says whether the
    // feed was empty or the shape moved.
    let raw: serde_json::Value = serde_json::from_slice(&bytes)?;
    let entities = raw
        .get("Entity")
        .and_then(|e| e.as_array())
        .map(Vec::len)
        .unwrap_or(0);
    println!("payload     {entities} entities");
    if entities == 0 {
        println!("            ^ no Entity array: the payload shape has changed");
    }

    let rt = crate::rt::parse(&bytes)?;
    let age = rt.age(crate::rt::now_epoch());
    println!("parsed      {} trips, feed built {age}s ago", rt.trips);
    if rt.trips == 0 && entities > 0 {
        println!("            ^ entities present but none parsed: FIELD NAMES MOVED");
    }

    // Cross-reference against the cache: how much of the feed can we resolve?
    let mut with_time = 0usize;
    let mut known_trip = 0usize;
    let mut checked = 0usize;
    let mut rail = 0usize;
    for e in raw
        .get("Entity")
        .and_then(|e| e.as_array())
        .unwrap_or(&vec![])
    {
        let Some(trip) = e
            .pointer("/TripUpdate/Trip/TripId")
            .and_then(|t| t.as_str())
        else {
            continue;
        };
        checked += 1;
        if let Some(stus) = e
            .pointer("/TripUpdate/StopTimeUpdate")
            .and_then(|s| s.as_array())
            && let Some(stop) = stus
                .first()
                .and_then(|s| s.get("StopId"))
                .and_then(|s| s.as_str())
            && rt.arrival(trip, stop).is_some()
        {
            with_time += 1;
        }
        // One lookup answers both questions: the route_type is None when the
        // trip is absent from the cache, and 0 when it is rail.
        let route_type = app.route_type_of(trip)?;
        if let Some(rt) = route_type {
            known_trip += 1;
            if rt == 0 {
                rail += 1;
            }
        }
    }
    let pct = |n: usize| {
        if checked == 0 {
            0.0
        } else {
            100.0 * n as f64 / checked as f64
        }
    };
    println!(
        "predictions {with_time}/{checked} first stops resolved ({:.0}%)",
        pct(with_time)
    );
    println!(
        "static join {known_trip}/{checked} trip_ids in the cache ({:.0}%)",
        pct(known_trip)
    );
    if pct(known_trip) < 90.0 {
        println!("            ^ the cache is stale: run `otransit update`");
    }
    println!("O-Train     {rail} trips (expected 0; rail has no realtime)");

    // The updates feed is a CMS emitting RSS, not a specified format. If it
    // stops tagging routes the way it does today, every screen quietly reports
    // no detours, which looks exactly like a calm week. This is where that
    // becomes visible.
    app.block_on_alerts(10);
    let n = app.alert_count();
    println!("alerts      {n} detours and route changes");
    if n == 0 {
        println!("            ^ none parsed: the updates feed may have changed shape");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestGtfs;
    use chrono::NaiveDate;

    /// A cache with one bus route and one stop, and that stop pinned.
    fn pinned_app(dir: &std::path::Path) -> App {
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("pins"), "S1\t1902\tBANK / SOMERSET W\n").unwrap();
        let g = TestGtfs::new()
            .route("7", "7", 3, "0057B8")
            .always("S")
            .trip("t1", "7", "S", "St-Laurent")
            .stop("S1", "1902", "BANK / SOMERSET W")
            .stop_time("t1", "S1", 1, "10:00:00");
        App::offline(
            g.into_conn(),
            NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
            9 * 3600,
            Some(dir.join("pins")),
        )
        .unwrap()
    }

    #[test]
    fn the_walk_finds_the_modes_below_a_pin_rather_than_at_a_fixed_row() {
        // Both tools used to select row 0 for "Bus" and row 1 for "O-Train".
        // Since pins were added, row 0 is a pin, so on any machine with one
        // the walk entered a departures board and printed every frame below it
        // under a label it no longer matched -- "3. directions" over a board.
        let dir = std::env::temp_dir().join("otransit-dev-mode");
        let app = pinned_app(&dir);

        assert_eq!(
            mode_row(&app, app::Mode::Bus),
            Some(1),
            "one pin sits above the modes, so Bus is not row 0: {:?}",
            app.rows().iter().map(app::Row::primary).collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_walk_reaches_the_route_list_and_not_a_board() {
        // The consequence the test above only implies: what the first `enter`
        // actually lands on.
        let dir = std::env::temp_dir().join("otransit-dev-walk");
        let mut app = pinned_app(&dir);

        app.state.select(mode_row(&app, app::Mode::Bus));
        app.enter().unwrap();
        assert!(
            matches!(app.screen, app::Screen::Routes { .. }),
            "entered a board instead of the route list"
        );
    }
}
