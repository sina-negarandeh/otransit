//! Rendering.
//!
//! The UI is bottom-anchored and inline: it claims only the rows it needs at
//! the bottom of the terminal rather than taking over the screen, so whatever
//! you were reading stays where it was. Committed choices are pushed up into
//! scrollback by `main`, leaving the trail of what you picked visible above.

mod layout;
mod palette;

pub use palette::RULE_RGB;

use crate::app::{App, Board, Crumb, Row, Screen, WAIT_W, fmt_hm};
use crate::pins::PinState;
use layout::{Cols, badge_label, gutter, marker, truncate};
use palette::{ACCENT, AMBER, Cells, DIM, FG, RULE, badge, cells, hex};
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

/// Tallest the inline viewport ever needs to be: two rules, the options
/// between them, and the status bar.
///
/// The top rule is the one the logo's pole stands on. It used to be printed
/// with the logo, into scrollback, where it could not be repainted and scrolled
/// away after two screens. Drawn here it brackets the app on every screen and
/// has somewhere to put the weather, at the cost of one terminal row.
pub const VIEWPORT_H: u16 = MAX_ROWS + 3;

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

/// How many rows a list of this many items actually draws.
///
/// Every screen is bounded by MAX_ROWS: that is what the viewport reserves, and
/// asking for more just gets silently clipped.
fn rows_for(items: usize) -> u16 {
    u16::try_from(items).unwrap_or(MAX_ROWS).clamp(1, MAX_ROWS)
}

/// Those rows plus the chrome under them.
///
/// Split from `rows_for` because a caller that wants the rows alone would
/// otherwise ask for the height and subtract the chrome back off, leaving the
/// same 2 written twice with opposite signs and nothing tying them together.
fn height_for(rows: usize) -> u16 {
    rows_for(rows) + 3 // two rules + status
}

/// Two groups with the space between them: one against the top of `area`, one
/// against the bottom.
///
/// Both screens that use it are saying the same thing — what you are being
/// told is not what you are choosing between — and the gap is what says it.
/// The pinned first screen puts the stops you asked for over the ways in; a
/// route's screen puts its detour over the ways through it.
fn two_groups(area: Rect, top: usize, bottom: usize) -> (Rect, Rect) {
    let [top, _gap, bottom] = Layout::vertical([
        Constraint::Length(u16::try_from(top).unwrap_or(u16::MAX)),
        Constraint::Min(0),
        Constraint::Length(u16::try_from(bottom).unwrap_or(u16::MAX)),
    ])
    .areas(area);
    (top, bottom)
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
///
/// Three rows go to the modes and to the gap between the two groups. The gap
/// is what says they are different kinds of thing, so the cap has to leave
/// room for it: at `MAX_ROWS - 2` the pins and the modes met and the
/// separation disappeared exactly when the feature was fully used.
pub const MAX_PINS: usize = MAX_ROWS as usize - 3;

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
    // A detour for the route being chosen, shown above the choices. The same
    // screens the gutter marks, for the same reason: they sit under exactly
    // one route, so there is exactly one message to show -- and it stays up
    // while you pick a stop, which is when a detour decides where you walk.
    let notice = app.route_alert();
    let want = if split.is_some() || notice.is_some() {
        area.height
    } else {
        height_for(count).min(area.height)
    };

    let Panel {
        ground,
        body,
        rule,
        status,
    } = panel(area, want);

    // The message takes the top of the body and the list keeps the bottom,
    // where it sits on every other screen. Only the message moves.
    let body = match &notice {
        Some(text) => {
            // Wrapped once, here: the height reserved and the lines drawn are
            // the same list, so they cannot disagree about how tall this is.
            let lines = wrap(text, body.width.saturating_sub(4) as usize, 2);
            let (top, list) = two_groups(body, lines.len(), usize::from(rows_for(count)));
            alert(f, top, &lines);
            list
        }
        None => body,
    };

    match split {
        _ if on_board => departures(f, body, app),
        Some(pins) => menu(f, body, app, &rows, pins),
        None => list(f, body, app, &rows),
    }
    // The top one carries the weather, held against its right end. The left is
    // where every row's content starts, so the ambient column is the right --
    // the same side the status bar keeps its key hints on.
    rule_line(f, ground, app.weather().as_deref());
    rule_line(f, rule, None);
    status_bar(f, status, app, &rows);
}

/// The four bands every screen draws into, top to bottom.
struct Panel {
    /// The rule the logo's pole stands on, and where the weather sits.
    ground: Rect,
    body: Rect,
    rule: Rect,
    status: Rect,
}

/// Split the viewport into them.
///
/// The top rule belongs to the frame, not to the panel: it is what the logo's
/// pole stands on, so it stays at the top of the viewport however few rows the
/// screen below it needs. Inside it, everything still hugs the bottom and the
/// space above is left to scrollback.
fn panel(area: Rect, want: u16) -> Panel {
    let [ground, rest] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    let [_, filled] = Layout::vertical([
        Constraint::Min(0),
        Constraint::Length(want.saturating_sub(1)),
    ])
    .areas(rest);
    let [body, rule, status] = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(filled);
    Panel {
        ground,
        body,
        rule,
        status,
    }
}

/// A full-width rule, with an optional note held against its right end.
///
/// One function for both rules, so the pair that brackets the app cannot drift
/// apart. The note is dropped rather than truncated when it will not fit: half
/// a temperature is worse than none, and the rule then looks exactly as it did
/// before there was anything to say.
fn rule_line(f: &mut Frame, area: Rect, note: Option<&str>) {
    let width = area.width as usize;
    let note = note.filter(|n| n.chars().count() + 2 <= width);
    let fill = width - note.map_or(0, |n| n.chars().count() + 1);
    let mut spans = vec![Span::styled("─".repeat(fill), Style::default().fg(RULE))];
    if let Some(n) = note {
        // Brighter than the rule it sits on: the line should recede and the
        // reading should not.
        spans.push(Span::styled(format!(" {n}"), Style::default().fg(DIM)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
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

/// Wrap a headline to the width, at spaces, to at most `max` lines.
///
/// The feed's titles run from 43 to 101 columns because they enumerate the
/// routes they affect. Truncating puts the ellipsis over the useful half --
/// "Detour: Routes 19, 42, 44, 48 during Terminal Avenue bridge clos…" -- so
/// they wrap instead. There is room: the directions screen carries two to
/// seven rows in a body of eight.
fn wrap(text: &str, width: usize, max: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        let fits = lines
            .last()
            .is_some_and(|l| l.chars().count() + 1 + word.chars().count() <= width);
        if fits {
            let line = lines.last_mut().expect("fits implies one exists");
            line.push(' ');
            line.push_str(word);
        } else if lines.len() == max {
            // Out of room: the last line gives up its tail for an ellipsis
            // rather than stopping mid-word with no sign it was cut.
            if let Some(last) = lines.last_mut() {
                *last = truncate(last, width.saturating_sub(1)) + "…";
            }
            break;
        } else {
            // A word longer than the whole width still has to fit in it. The
            // `Paragraph` does not wrap, so an unbounded line is one ratatui
            // clips at the buffer edge -- silently, which is the failure this
            // function exists to avoid.
            lines.push(truncate(word, width));
        }
    }
    lines
}

/// The published detour for the route on screen, above the choices.
///
/// Drawn where the gutter is drawn, on the screens that sit under exactly one
/// route -- which is what makes one message the right number to show. A stop
/// board mixes routes and a pin list mixes stops; neither has a single answer
/// to put here.
/// Takes the wrapped lines rather than the headline: the caller has to wrap to
/// know how tall this is, and wrapping again here would be a second answer to
/// a question already asked.
fn alert(f: &mut Frame, area: Rect, lines: &[String]) {
    // Amber, the palette's "off-nominal but not wrong": the same colour a bus
    // running late wears. Neither screen this appears on has a wait column, so
    // it is the only amber on them and does not have to compete with a
    // countdown for the meaning. Not bold -- two lines of bold prose shout,
    // and the colour has already said it.
    let warn = Style::default().fg(AMBER);
    let lines: Vec<Line> = lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            Line::from(vec![
                Span::styled(if i == 0 { " ⚠ " } else { "   " }, warn),
                Span::styled(l.clone(), warn),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), area);
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
    let (top, bottom) = two_groups(area, pins, rows.len() - pins);

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

            let Cells {
                time,
                wait: (countdown, countdown_style),
                note: (note, note_style),
            } = cells(d, m);

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
                Span::styled(fmt_hm(when), time),
                Span::raw("   "),
                Span::styled(format!("{countdown:>WAIT_W$}"), countdown_style),
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
mod tests;

/// Print every symbol the app draws, each between two rails.
///
/// Not a logo test: the mark is one ring, and everything below belongs to the
/// screens this module renders. It lived beside the block-art self-test for a
/// while, which made `logo.rs` import the weather module for the sake of a
/// diagnostic — a dependency with no idea behind it.
///
/// The rails are the point. Every glyph here must occupy one cell, because
/// `truncate` and `rule_line` both measure in `chars`; one that is two cells
/// wide shifts everything after it and nothing else would say so.
pub fn print_symbols() {
    let row = |g: char, what: &str| println!("      |{g}|  {what}");
    println!("  F · every symbol the app draws, each between two rails.");
    println!("      The right rail should line up down the column. One that");
    println!("      sits a column further out is two cells wide, and would");
    println!("      push everything after it out of alignment.\n");
    for (glyph, means) in crate::weather::legend() {
        row(glyph, &format!("weather: {means}"));
    }
    for (glyph, what) in [
        ('\u{26A0}', "a detour on the route you picked"),
        ('\u{276F}', "the cursor"),
        ('\u{2502}', "the gutter, in the route's colour"),
        ('\u{2500}', "the two rules"),
        ('\u{203A}', "the trail of choices"),
        ('\u{00B7}', "the separator"),
        ('\u{2026}', "a label that had to be shortened"),
        ('\u{2014}', "a cancelled bus, where its countdown would be"),
        ('\u{00B0}', "degrees"),
        ('\u{2191}', "up"),
        ('\u{2193}', "down"),
        ('\u{21B5}', "select"),
    ] {
        row(glyph, what);
    }
    println!("\n      These two are known to be two cells wide, and are");
    println!("      deliberately unused. They are here as a control: if they");
    println!("      line up with the rest, your terminal is not measuring");
    println!("      width the way this test assumes.");
    row('\u{26C5}', "sun behind cloud (unused)");
    row('\u{26A1}', "lightning (unused)");
    println!();
}
