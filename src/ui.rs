//! Rendering.
//!
//! The UI is bottom-anchored and inline: it claims only the rows it needs at the
//! bottom of the terminal, the way Claude Code and other Ink-based CLIs do,
//! rather than taking over the screen. Committed choices are pushed up into
//! scrollback by `main`, so the trail of what you picked stays visible above.

use crate::app::{App, Board, Crumb, Row, Screen, WAIT_W, fmt_hm, fmt_wait, lateness};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
};

const DIM: Color = Color::Rgb(0x6d, 0x6e, 0x70);
const FG: Color = Color::Rgb(0xe6, 0xe6, 0xe6);
/// Brand red. Marks the current choice and the active filter.
const ACCENT: Color = Color::Rgb(0xDA, 0x38, 0x39);
/// Dimmer than DIM — for rules and separators that should recede entirely.
const RULE: Color = Color::Rgb(0x3a, 0x3b, 0x3d);
const INK: Color = Color::Rgb(0x0c, 0x0c, 0x0c);

/// Rows of list to show at most, before the list starts scrolling.
///
/// This also fixes the inline viewport height, and ratatui can't resize an
/// inline viewport after creation — so every row here is reserved on screen
/// even when a screen only needs two. Keep it as small as the busiest screen
/// can tolerate.
const MAX_ROWS: u16 = 8;

/// Tallest the inline viewport ever needs to be: options + rule + status bar.
pub const VIEWPORT_H: u16 = MAX_ROWS + 2;

fn hex(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some(((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// WCAG relative luminance of an sRGB colour.
fn luminance(r: u8, g: u8, b: u8) -> f32 {
    let lin = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// Route badge colours: GTFS route_color as background, with black or white
/// text — whichever has the higher WCAG contrast ratio against it.
pub fn badge(bg_hex: &str) -> (Color, Color) {
    let Some((r, g, b)) = hex(bg_hex) else {
        return (ACCENT, INK);
    };
    let l = luminance(r, g, b);
    let fg = if (l + 0.05) / 0.05 >= 1.05 / (l + 0.05) {
        INK
    } else {
        Color::Rgb(255, 255, 255)
    };
    (Color::Rgb(r, g, b), fg)
}

/// Colour by urgency: the board should be readable in peripheral vision.
fn urgency(mins: i32) -> Style {
    match mins {
        m if m < 0 => Style::default().fg(DIM).add_modifier(Modifier::CROSSED_OUT),
        m if m <= 2 => Style::default()
            .fg(Color::Rgb(0xff, 0x6b, 0x6b))
            .add_modifier(Modifier::BOLD),
        m if m <= 6 => Style::default()
            .fg(Color::Rgb(0xff, 0xc1, 0x07))
            .add_modifier(Modifier::BOLD),
        m if m <= 15 => Style::default().fg(Color::Rgb(0x5c, 0xd6, 0x8a)),
        _ => Style::default().fg(DIM),
    }
}

/// Width of a route badge, in cells. OC Transpo route names are 1-3 characters,
/// so 5 leaves one space on each side of the longest.
const BADGE_W: usize = 5;

/// Centre `name` in a fixed-width badge. When the padding can't split evenly
/// the spare cell goes on the left, so a two-digit number sits a hair right of
/// centre rather than left of it — the direction that reads as centred.
fn badge_label(name: &str) -> String {
    let pad = BADGE_W.saturating_sub(name.chars().count());
    let right = pad / 2;
    let left = pad - right;
    format!("{}{}{}", " ".repeat(left), name, " ".repeat(right))
}

/// Cut a string to `max` display cells, with an ellipsis if it had to give.
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    match max {
        0 => String::new(),
        1 => "…".into(),
        _ => s.chars().take(max - 1).collect::<String>() + "…",
    }
}

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
    let want = height_for(count).min(area.height);

    // Everything hugs the bottom; the space above is left to scrollback.
    let [_, panel] = Layout::vertical([Constraint::Min(0), Constraint::Length(want)]).areas(area);
    let [body, rule, status] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(panel);

    if on_board {
        departures(f, body, app);
    } else {
        list(f, body, app, &rows);
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
            if note.is_empty() {
                "esc · q ".to_string()
            } else {
                format!("{note} · esc · q ")
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
    if over > 0 {
        if let Some(last) = left.last_mut() {
            let keep = last.content.chars().count().saturating_sub(over);
            last.content = truncate(&last.content, keep).into();
        }
    }

    let pad = width
        .saturating_sub(len(&left) + hints.chars().count())
        .max(1);
    left.push(Span::raw(" ".repeat(pad)));
    left.push(Span::styled(hints.clone(), Style::default().fg(RULE)));
    f.render_widget(Paragraph::new(Line::from(left)), area);
}

/// Two columns: a label and one dim detail at a fixed place. Every screen but
/// the search results.
///
/// The marker (" ❯ ") is three cells wide on every row, so both widths here are
/// measured after it.
struct Plain {
    /// The label: a stop name, or the badge and the gap that follows it.
    label: usize,
    detail: usize,
}

/// Four columns, for search results, which have to stay put whatever any
/// individual name or destination is.
struct Wide {
    name: usize,
    code: usize,
    toward: usize,
    routes: usize,
}

const MARKER_W: usize = 3;

/// The cursor, in the brand red, or the blank column that keeps every other
/// row aligned with it.
fn marker(selected: bool) -> Span<'static> {
    Span::styled(
        if selected { " ❯ " } else { "   " },
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    )
}
/// The badge, plus the gap that puts the detail column at a fixed place.
const BADGE_COL: usize = BADGE_W + 3;
/// Label column on the plain screens, chosen so stop names rarely truncate.
const LABEL_COL: usize = 34;

/// Both column sets for one frame.
///
/// Two usizes and four: cheaper to compute both than to thread the choice
/// through the row loop, and it means neither type ever carries a field that
/// does not apply to it.
struct Cols {
    plain: Plain,
    wide: Wide,
}

impl Cols {
    fn new(rows: &[Row], width: usize) -> Self {
        Self {
            plain: Plain::new(width, rows.first().is_some_and(|r| r.badge().is_some())),
            wide: Wide::new(width),
        }
    }
}

impl Plain {
    fn new(width: usize, badges: bool) -> Self {
        let label = if badges { BADGE_COL } else { LABEL_COL };
        Self {
            label,
            detail: width.saturating_sub(MARKER_W + label),
        }
    }
}

impl Wide {
    /// Shares out what is left after the fixed columns, rather than using a
    /// fixed label width as the plain layout does.
    fn new(width: usize) -> Self {
        let code = 6;
        let toward = ((width * 24) / 100).clamp(10, 22);
        let routes = ((width * 22) / 100).clamp(8, 26);
        let gaps = 2 * 3; // between name, code, toward and routes
        let name = width
            .saturating_sub(MARKER_W + gaps + code + toward + routes)
            .clamp(12, 36);
        Self {
            name,
            code,
            toward,
            routes,
        }
    }
}

/// A stop search result: name, platform, pole number, destination, routes.
fn hit_line(h: &crate::db::StopHit, c: &Wide) -> Line<'static> {
    // The platform sits right after the name, and the pair is padded as one
    // field so everything to the right still lines up.
    let plat = &h.platform;
    // Cells, not bytes: every other width here counts chars, and an accented
    // platform code measured in bytes over-reserves and drags the rest right.
    let plat_w = if plat.is_empty() {
        0
    } else {
        plat.chars().count() + 1 // the space before it
    };
    let shown = truncate(
        &crate::db::strip_platform(&h.name, plat),
        c.name.saturating_sub(plat_w),
    );
    let used = shown.chars().count() + plat_w;
    let toward = if h.toward.is_empty() {
        String::new()
    } else {
        format!("→ {}", truncate(&h.toward, c.toward.saturating_sub(2)))
    };

    Line::from(vec![
        Span::styled(shown, Style::default().fg(FG)),
        Span::styled(
            if plat.is_empty() {
                String::new()
            } else {
                format!(" {plat}")
            },
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" ".repeat(c.name.saturating_sub(used) + 2)),
        Span::styled(
            format!("{:<width$}", format!("#{}", h.code), width = c.code),
            Style::default().fg(RULE),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{toward:<width$}", width = c.toward),
            Style::default().fg(DIM),
        ),
        Span::raw("  "),
        Span::styled(
            truncate(&h.routes.join(", "), c.routes),
            Style::default().fg(DIM),
        ),
    ])
}

/// Every other screen: a label and one dim detail at a fixed column.
fn plain_line(row: &Row, c: &Plain) -> Line<'static> {
    // Route numbers get a filled badge in the official route colour.
    let (label, style) = match row.badge() {
        Some(colour) => {
            let (bg, fg) = badge(colour);
            (
                badge_label(&row.primary()),
                Style::default().bg(bg).fg(fg).add_modifier(Modifier::BOLD),
            )
        }
        // Truncate rather than letting a long label shove the detail column right.
        None => (
            truncate(&row.primary(), c.label.saturating_sub(2)),
            Style::default().fg(FG),
        ),
    };

    let gap = c.label.saturating_sub(label.chars().count()).max(1);
    Line::from(vec![
        Span::styled(label, style),
        Span::raw(" ".repeat(gap)),
        Span::styled(
            truncate(&row.secondary(), c.detail),
            Style::default().fg(DIM),
        ),
    ])
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
    let cols = Cols::new(rows, width);
    // The marker is drawn into the row rather than handed to the widget:
    // `highlight_symbol` takes a bare &str and is painted by `highlight_style`
    // along with the whole line, so colouring it there would flatten the row's
    // own columns — and on the search screen those columns are the only thing
    // telling two sides of the same corner apart.
    let selected = app.state.selected();
    let items: Vec<ListItem> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let mut line = match row {
                Row::Hit(h) => hit_line(h, &cols.wide),
                other => plain_line(other, &cols.plain),
            };
            line.spans.insert(0, marker(Some(i) == selected));
            ListItem::new(line)
        })
        .collect();

    let l = List::new(items).highlight_style(Style::default().add_modifier(Modifier::BOLD));
    f.render_stateful_widget(l, area, &mut app.state);
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

    let green = Color::Rgb(0x5c, 0xd6, 0x8a);
    let amber = Color::Rgb(0xff, 0xc1, 0x07);
    let red = Color::Rgb(0xff, 0x6b, 0x6b);
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

            let (note, note_style) = if d.canceled {
                (
                    "cancelled".to_string(),
                    Style::default().fg(red).add_modifier(Modifier::BOLD),
                )
            } else if let Some(live) = d.live {
                match lateness(live, d.secs) {
                    -1..=1 => ("on time".into(), Style::default().fg(green)),
                    l if l > 0 => (format!("{l} late"), Style::default().fg(amber)),
                    l => (format!("{} early", -l), Style::default().fg(DIM)),
                }
            } else {
                ("sched".into(), Style::default().fg(RULE))
            };

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

    #[test]
    fn badges_are_all_the_same_width() {
        for name in ["1", "5", "10", "99", "105", "221"] {
            assert_eq!(
                badge_label(name).chars().count(),
                BADGE_W,
                "badge for {name:?} is not {BADGE_W} cells"
            );
        }
    }

    #[test]
    fn badges_centre_the_number() {
        // Odd padding splits with the spare cell on the left.
        assert_eq!(badge_label("5"), "  5  ");
        assert_eq!(badge_label("10"), "  10 ");
        assert_eq!(badge_label("105"), " 105 ");
    }

    #[test]
    fn badge_never_loses_the_number() {
        // Longer than the badge: degrade to the bare name, don't truncate it.
        assert_eq!(badge_label("SHOP"), " SHOP");
        assert_eq!(badge_label("TOOLONG"), "TOOLONG");
    }
}
