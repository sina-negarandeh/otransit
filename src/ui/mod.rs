//! Rendering.
//!
//! The UI is bottom-anchored and inline: it claims only the rows it needs at
//! the bottom of the terminal rather than taking over the screen, so whatever
//! you were reading stays where it was. Committed choices are pushed up into
//! scrollback by `main`, leaving the trail of what you picked visible above.

mod layout;
mod palette;

pub use palette::RULE_RGB;

use crate::app::{App, Board, Crumb, PinState, Row, Screen, WAIT_W, fmt_hm, fmt_wait};
use layout::{Cols, badge_label, gutter, marker, truncate};
use palette::{ACCENT, DIM, FG, RULE, badge, hex, status, urgency};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
};

/// Rows of list to show at most, before the list starts scrolling.
///
/// This also fixes the inline viewport height, and ratatui can't resize an
/// inline viewport after creation — so every row here is reserved on screen
/// even when a screen only needs two. Keep it as small as the busiest screen
/// can tolerate.
const MAX_ROWS: u16 = 8;

/// Tallest the inline viewport ever needs to be: options + rule + status bar.
pub const VIEWPORT_H: u16 = MAX_ROWS + 2;

/// How many rows this screen wants. `draw` computes the row list itself and
/// calls `height_for` directly; this exists so tests can ask the question
/// without rendering a frame.
#[cfg(test)]
pub fn desired_height(app: &App) -> u16 {
    height_for(
        app.contents
            .board()
            .map_or_else(|| app.rows().len(), <[crate::db::Departure]>::len),
    )
}

/// Every screen is bounded by MAX_ROWS: that is what the viewport reserves, and
/// asking for more just gets silently clipped.
fn height_for(rows: usize) -> u16 {
    u16::try_from(rows).unwrap_or(MAX_ROWS).clamp(1, MAX_ROWS) + 2 // rule + status
}

/// Departures fetched per pin, of which one is drawn.
///
/// Headroom for the re-sort, for the same reason the board has it: a late trip
/// can be overtaken by one scheduled after it, and with a single row fetched
/// there is nothing to promote.
pub const PIN_FETCH: usize = 8;

/// Pins the first screen can hold: whatever fits above the two modes without
/// the list starting to scroll.
///
/// Derived rather than chosen. A pin list you have to scroll has lost the
/// property that makes it worth having -- that the cursor is already on the
/// answer -- so the layout decides the cap.
pub const MAX_PINS: usize = MAX_ROWS as usize - 2;

/// Stop search results to fetch. Also the point past which the count in the
/// status bar stops being a total and becomes "at least this many".
pub const SEARCH_LIMIT: usize = 25;

/// Departures the board draws. The area clips at `MAX_ROWS` regardless, but
/// building the rest of the rows only to throw them away is wasted work, and a
/// named cap says what the board shows instead of leaving it to the layout.
const BOARD_LIMIT: usize = MAX_ROWS as usize;

/// How many to fetch. `departures` can only order by the timetable, so a trip
/// running late may truly arrive before one scheduled after it. Fetching extra
/// gives the live re-sort something to promote; without headroom the board can
/// omit the next bus you could actually catch.
pub const BOARD_FETCH: usize = BOARD_LIMIT * 3;

/// Options above, a rule, then the question and trail pinned to the very
/// bottom. The status bar never moves as the list above it changes length.
pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    // Built once: this was previously computed by desired_height and again by
    // list(), on every frame of a loop that redraws four times a second.
    let rows = app.rows();
    // Asked of the contents rather than derived from the screen, so the two
    // cannot disagree about what is on display.
    let on_board = app.contents.board().is_some();
    let count = app
        .contents
        .board()
        .map_or_else(|| rows.len(), <[crate::db::Departure]>::len);
    // A split menu reaches for the whole viewport: the pins sit against the
    // ground line the logo stands on, and the ways in stay where they were,
    // just above the rule. The viewport reserves these rows on every screen
    // anyway, so using the top of it costs nothing.
    let split = (!on_board).then(|| pinned(&rows)).flatten();
    let want = if split.is_some() {
        area.height
    } else {
        height_for(count).min(area.height)
    };

    // Everything hugs the bottom; the space above is left to scrollback.
    let [_, panel] = Layout::vertical([Constraint::Min(0), Constraint::Length(want)]).areas(area);
    let [body, rule, status] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(panel);

    match split {
        _ if on_board => departures(f, body, app),
        Some(pins) => menu(f, body, app, &rows, pins),
        None => list(f, body, app, &rows),
    }
    f.render_widget(
        Paragraph::new(Span::styled(
            "─".repeat(rule.width as usize),
            Style::default().fg(RULE),
        )),
        rule,
    );
    status_bar(f, status, app, &rows);
}

/// The question, the whole trail of choices, and the key hints — one line,
/// pinned to the bottom. The most recent choice is brightest.
fn status_bar(f: &mut Frame, area: Rect, app: &App, rows: &[Row]) {
    let mut left = vec![
        Span::raw(" "),
        Span::styled(
            app.title(),
            Style::default().fg(FG).add_modifier(Modifier::BOLD),
        ),
    ];
    // On the O-Train the trail takes the line's colour; buses keep brand red.
    let here = app
        .accent_hex()
        .and_then(hex)
        .map(|(r, g, b)| Color::Rgb(r, g, b))
        .unwrap_or(ACCENT);

    let crumbs = app.crumbs();
    let last = crumbs.len().saturating_sub(1);
    for (i, c) in crumbs.iter().enumerate() {
        left.push(Span::styled(
            if i == 0 { "   " } else { " › " },
            Style::default().fg(RULE),
        ));
        match c {
            // Route numbers keep their badge, so the colour survives where it
            // would be illegible as plain text.
            Crumb::Route { name, color } => {
                let (bg, fg) = badge(color);
                left.push(Span::styled(
                    format!(" {name} "),
                    Style::default().bg(bg).fg(fg).add_modifier(Modifier::BOLD),
                ));
            }
            Crumb::Plain(t) => {
                let style = if i == last {
                    Style::default().fg(here)
                } else {
                    Style::default().fg(DIM)
                };
                left.push(Span::styled(t.clone(), style));
            }
        }
    }
    let typed = app.screen.typed();
    if !typed.is_empty() {
        left.push(Span::raw("   "));
        left.push(Span::styled(
            format!("/{typed}"),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    }

    let note = app.rt_note().unwrap_or_default();
    let hints = match &app.screen {
        Screen::Departures(_) => {
            // Names the gesture and its effect, so the key is discoverable and
            // a refusal at the cap reads as a state rather than a dead key.
            let pin = match app.board_pin() {
                Some(PinState::Pinned) => "p unpin · ",
                Some(PinState::Unpinned) => "p pin · ",
                Some(PinState::Full) => "pins full · ",
                None => "",
            };
            if note.is_empty() {
                format!("{pin}esc · q ")
            } else {
                format!("{note} · {pin}esc · q ")
            }
        }
        // Nothing on screen advertises the stop search, so the hint must.
        Screen::Mode => "type to find a stop · ↑↓ · ↵ · q ".to_string(),
        // At the cap the count is the limit, not the number of matches, so it
        // must not be reported as a total.
        Screen::Search { .. } if rows.len() >= SEARCH_LIMIT => {
            format!("{}+ found · ↑↓ · ↵ · esc ", rows.len())
        }
        Screen::Search { .. } => format!("{} found · ↑↓ · ↵ · esc ", rows.len()),
        _ => "↑↓ · ↵ · esc · q ".to_string(),
    };

    // Trail can outgrow the line; drop the oldest crumbs before truncating.
    let width = area.width as usize;
    let len = |v: &Vec<Span>| v.iter().map(|s| s.content.chars().count()).sum::<usize>();
    while len(&left) + hints.chars().count() + 2 > width && left.len() > 4 {
        left.drain(2..4);
        // The crumb that is now first shouldn't keep a leading separator.
        if let Some(sep) = left.get_mut(2) {
            *sep = Span::styled("   ", Style::default().fg(RULE));
        }
    }

    // A single crumb can still be too wide to drop; shorten it instead.
    let over = (len(&left) + hints.chars().count() + 1).saturating_sub(width);
    if over > 0
        && let Some(last) = left.last_mut()
    {
        let keep = last.content.chars().count().saturating_sub(over);
        last.content = truncate(&last.content, keep).into();
    }

    let pad = width
        .saturating_sub(len(&left) + hints.chars().count())
        .max(1);
    left.push(Span::raw(" ".repeat(pad)));
    left.push(Span::styled(hints.clone(), Style::default().fg(RULE)));
    f.render_widget(Paragraph::new(Line::from(left)), area);
}

/// One list renderer. The row decides its own shape; the screen is not consulted.
fn list(f: &mut Frame, area: Rect, app: &mut App, rows: &[Row]) {
    if rows.is_empty() {
        let msg = if app.screen.typed().is_empty() {
            "  nothing here"
        } else {
            "  no matches"
        };
        f.render_widget(
            Paragraph::new(Span::styled(msg, Style::default().fg(DIM))),
            area,
        );
        return;
    }

    // Every row on a screen has the same shape, so the first one settles the
    // badge column for all of them.
    let width = area.width as usize;
    // The rule is measured rather than named by a constant, so the cells
    // reserved for it cannot drift from the glyph drawn.
    let rule = gutter(&app.screen);
    let gutter_w = rule.as_ref().map_or(0, |s| s.content.chars().count());
    let cols = Cols::new(rows, width, gutter_w);
    // The marker is drawn into the row rather than handed to the widget:
    // `highlight_symbol` takes a bare &str and is painted by `highlight_style`
    // along with the whole line, so colouring it there would flatten the row's
    // own columns — and on the search screen those columns are the only thing
    // telling two sides of the same corner apart.
    let items = items(
        rows,
        &cols,
        rule.as_ref(),
        app.state.selected(),
        0,
        app.now(),
    );
    let l = List::new(items).highlight_style(Style::default().add_modifier(Modifier::BOLD));
    f.render_stateful_widget(l, area, &mut app.state);
}

/// Rows as drawable items, with the cursor on the one at `selected`.
///
/// `offset` is what this group's first row is numbered in the list as a whole,
/// so a screen drawn as two groups can still have one cursor running through
/// it.
fn items<'a>(
    rows: &[Row],
    cols: &Cols,
    rule: Option<&Span<'static>>,
    selected: Option<usize>,
    offset: usize,
    now: i32,
) -> Vec<ListItem<'a>> {
    rows.iter()
        .enumerate()
        .map(|(i, row)| {
            // Built in the order they appear: the cursor column, the route's
            // rule if there is one, then the row itself.
            let mut line = cols.line(row, now);
            let mut spans = vec![marker(Some(i + offset) == selected)];
            spans.extend(rule.cloned());
            spans.append(&mut line.spans);
            line.spans = spans;
            ListItem::new(line)
        })
        .collect()
}

/// How many rows at the front are pins, when the screen is a mixed menu.
///
/// Derived from the rows rather than asked of the screen, like every other
/// layout decision in this file: the rows know what they are.
fn pinned(rows: &[Row]) -> Option<usize> {
    let n = rows.iter().take_while(|r| matches!(r, Row::Pin(_))).count();
    (n > 0 && n < rows.len()).then_some(n)
}

/// The first screen when it has pins: answers at the top, the ways in at the
/// bottom, one cursor running through both.
///
/// Two groups rather than one list with a blank row between them. A spacer row
/// would be a position the cursor has to be taught to skip; a gap between two
/// areas is nothing at all.
fn menu(f: &mut Frame, area: Rect, app: &mut App, rows: &[Row], pins: usize) {
    let width = area.width as usize;
    let cols = Cols::new(rows, width, 0);
    let selected = app.state.selected();
    let ways = rows.len() - pins;

    let [top, _gap, bottom] = Layout::vertical([
        Constraint::Length(u16::try_from(pins).unwrap_or(u16::MAX)),
        Constraint::Min(0),
        Constraint::Length(u16::try_from(ways).unwrap_or(u16::MAX)),
    ])
    .areas(area);

    for (area, group, offset) in [(top, &rows[..pins], 0), (bottom, &rows[pins..], pins)] {
        f.render_widget(
            List::new(items(group, &cols, None, selected, offset, app.now()))
                .highlight_style(Style::default().add_modifier(Modifier::BOLD)),
            area,
        );
    }
}

fn departures(f: &mut Frame, area: Rect, app: &App) {
    let deps = app.contents.board().unwrap_or_default();
    if deps.is_empty() {
        f.render_widget(
            Paragraph::new(Span::styled(
                "  no more departures today",
                Style::default().fg(DIM),
            )),
            area,
        );
        return;
    }

    // A board reached by search mixes routes, so each row names its direction.
    let multi = matches!(app.screen, Screen::Departures(Board::Stop { .. }));
    // Everything except the headsign: marker, badge, time, wait, note and the
    // gaps between them. Derived from WAIT_W so the two cannot drift apart.
    let fixed_w = 37 + WAIT_W;
    let head_w = (area.width as usize).saturating_sub(fixed_w).clamp(8, 24);

    let lines: Vec<Line> = deps
        .iter()
        .take(BOARD_LIMIT)
        .map(|d| {
            let when = d.when();
            let m = crate::app::mins_until(when, app.now());

            let (note, note_style) = status(d);

            let time_style = if d.canceled {
                Style::default().fg(DIM).add_modifier(Modifier::CROSSED_OUT)
            } else if d.live.is_some() {
                Style::default().fg(FG)
            } else {
                Style::default().fg(DIM)
            };

            let (bg, fg) = badge(&d.route_color);
            let mut spans = vec![
                Span::raw("   "),
                Span::styled(
                    badge_label(&d.route_short),
                    Style::default().bg(bg).fg(fg).add_modifier(Modifier::BOLD),
                ),
                Span::raw("   "),
            ];
            if multi {
                spans.push(Span::styled(
                    format!("{:<width$}", truncate(&d.headsign, head_w), width = head_w),
                    Style::default().fg(DIM),
                ));
                spans.push(Span::raw("  "));
            }
            spans.extend([
                Span::styled(fmt_hm(when), time_style),
                Span::raw("   "),
                Span::styled(
                    format!(
                        "{:>WAIT_W$}",
                        if d.canceled {
                            "\u{2014}".to_string()
                        } else {
                            fmt_wait(m)
                        }
                    ),
                    if d.canceled {
                        Style::default().fg(DIM)
                    } else {
                        urgency(m)
                    },
                ),
                Span::raw("   "),
                Span::styled(format!("{note:<10}"), note_style),
                Span::styled(
                    if d.after_midnight {
                        "after midnight"
                    } else {
                        ""
                    }
                    .to_string(),
                    Style::default().fg(RULE),
                ),
            ]);
            Line::from(spans)
        })
        .collect();
    f.render_widget(Paragraph::new(lines), area);
}

#[cfg(test)]
mod tests {
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
        App::offline_with_pins(
            g.into_conn(),
            NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
            9 * 3600,
            dir.join("pins"),
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
        let mut app = App::offline_with_pins(
            g.into_conn(),
            NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
            9 * 3600,
            dir.join("pins"),
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
}
