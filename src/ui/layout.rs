//! One row, turned into a line: the column widths, and the spans that fill them.
//!
//! Nothing here draws a frame. A row is handed a set of widths and reports what
//! it looks like; the one thing that asks the screen a question is the gutter,
//! whose whole subject is which route the rows below it belong to.

use super::palette::{ACCENT, Cells, DIM, FG, NOTE_W, RULE, badge, cells, hex, reads_as_a_rule};
use crate::app::{Row, Screen, WAIT_W, tidy_stop_name as tidy};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

/// Width of a route badge, in cells. OC Transpo route names are 1-3 characters,
/// so 5 leaves one space on each side of the longest.
pub(super) const BADGE_W: usize = 5;

/// Centre `name` in a fixed-width badge. When the padding can't split evenly
/// the spare cell goes on the left, so a two-digit number sits a hair right of
/// centre rather than left of it — the direction that reads as centred.
pub(super) fn badge_label(name: &str) -> String {
    let pad = BADGE_W.saturating_sub(name.chars().count());
    let right = pad / 2;
    let left = pad - right;
    format!("{}{}{}", " ".repeat(left), name, " ".repeat(right))
}

/// Cut a string to `max` display cells, with an ellipsis if it had to give.
pub(super) fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    match max {
        0 => String::new(),
        1 => "…".into(),
        _ => s.chars().take(max - 1).collect::<String>() + "…",
    }
}

pub(super) const MARKER_W: usize = 3;

/// The screens that sit below exactly one route: the ways it runs, and the
/// stops along one of them.
///
/// One predicate rather than two matches, because two things are drawn on
/// exactly these screens for exactly this reason — the gutter, which says
/// which route you are under, and the detour, which is what is published
/// about that route. A screen above a route has no single answer for either,
/// and a board mixes routes. Kept together so they cannot drift apart.
pub(super) fn below_a_route(screen: &Screen) -> bool {
    matches!(screen, Screen::Stops { .. } | Screen::Directions { .. })
}

/// A rule down the left, in the route's own colour.
///
/// The stop list is in travel order and nothing else on screen says so, and
/// neither list says which route it belongs to. Two cells, no rows, and it
/// holds at any length — unlike a route diagram, whose termini are off screen
/// for most of the directions in this feed and which degrades to decoration
/// once they are.
///
/// Both screens below a route belong to it: the ways it runs, and the stops
/// along one of them. The rule says so once, down the side, instead of a badge
/// repeating the same number on every row.
pub(super) fn gutter(screen: &Screen) -> Option<Span<'static>> {
    if !below_a_route(screen) {
        return None;
    }
    let colour = screen
        .route()
        .and_then(|r| hex(&r.color))
        .filter(|&c| reads_as_a_rule(c))
        .map_or(RULE, |(r, g, b)| Color::Rgb(r, g, b));
    Some(Span::styled("│ ", Style::default().fg(colour)))
}

/// The cursor, in the brand red, or the blank column that keeps every other
/// row aligned with it.
pub(super) fn marker(selected: bool) -> Span<'static> {
    Span::styled(
        if selected { " ❯ " } else { "   " },
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
    )
}

/// The badge, plus the gap that puts the detail column at a fixed place.
const BADGE_COL: usize = BADGE_W + 3;

/// Label column on the plain screens, chosen so stop names rarely truncate.
const LABEL_COL: usize = 34;

/// Two columns: a label and one dim detail at a fixed place. Every screen but
/// the search results.
///
/// The marker (" ❯ ") is three cells wide on every row, so both widths here are
/// measured after it.
pub(super) struct Plain {
    /// The label: a stop name, or the badge and the gap that follows it.
    label: usize,
    detail: usize,
}

/// Four columns, for search results, which have to stay put whatever any
/// individual name or destination is.
pub(super) struct Wide {
    name: usize,
    code: usize,
    toward: usize,
    routes: usize,
}

/// Six columns for a pinned stop: which stop, the pole on its sign, the next
/// bus, where that bus is going, how long, and whether anything is tracking it.
///
/// The direction is the column a route number cannot do without. At a stop
/// served both ways, "the 7 in 4 min" is no use if it is the 7 going the other
/// way, which is the same reason a stop board carries a headsign column.
///
/// No clock time: it and the wait are the same fact given the hour, and only
/// one of them answers "should I leave now". The board shows both because it
/// is a table to scan; a pin is a single answer.
pub(super) struct PinCols {
    name: usize,
    toward: usize,
}

impl PinCols {
    fn new(width: usize) -> Self {
        // Everything but the two variable columns: marker, pole, badge, wait,
        // status and the gaps. Derived so the widths cannot drift from what is
        // actually drawn.
        let fixed = MARKER_W + POLE_W + BADGE_W + WAIT_W + NOTE_W + 6;
        let share = width.saturating_sub(fixed);
        // The name identifies the pin, so it wins the wider half.
        let name = (share * 58 / 100).clamp(10, 28);
        Self {
            name,
            toward: share.saturating_sub(name).clamp(6, 20),
        }
    }
}

/// Width of a pole number column, `#` included.
const POLE_W: usize = 6;

/// Both column sets for one frame.
///
/// Two usizes and four: cheaper to compute both than to thread the choice
/// through the row loop, and it means neither type ever carries a field that
/// does not apply to it.
pub(super) struct Cols {
    plain: Plain,
    wide: Wide,
    pin: PinCols,
}

impl Cols {
    /// The line for one row. Which layout applies is the row's business, so
    /// the caller never has to reach in and choose.
    pub(super) fn line(&self, row: &Row, now: i32) -> Line<'static> {
        match row {
            Row::Hit(h) => hit_line(h, &self.wide),
            Row::Pin(p) => pin_line(p, &self.pin, now),
            other => plain_line(other, &self.plain),
        }
    }

    /// `gutter_w` is measured from the gutter span rather than named as a
    /// constant, so the cells reserved cannot drift from the glyph drawn.
    pub(super) fn new(rows: &[Row], width: usize, gutter_w: usize) -> Self {
        Self {
            // Only the plain layout pays for the gutter. Wide draws search
            // results, which are never under a route.
            plain: Plain::new(
                width.saturating_sub(gutter_w),
                rows.first().is_some_and(|r| r.badge().is_some()),
            ),
            wide: Wide::new(width),
            pin: PinCols::new(width),
        }
    }
}

/// A pinned stop and the next bus from it.
fn pin_line(p: &crate::app::Pinned, c: &PinCols, now: i32) -> Line<'static> {
    let mut spans = vec![
        Span::styled(
            format!("{:<w$}", truncate(&tidy(&p.stop.name), c.name), w = c.name),
            Style::default().fg(FG),
        ),
        Span::raw(" "),
        Span::styled(
            format!("{:<w$}", format!("#{}", p.stop.code), w = POLE_W),
            Style::default().fg(RULE),
        ),
    ];
    let Some(d) = p.next() else {
        spans.push(Span::styled(" none left today", Style::default().fg(RULE)));
        return Line::from(spans);
    };
    let mins = crate::app::mins_until(d.when(), now);
    let (bg, fg) = badge(&d.route_color);
    // The clock time is the one cell a pin does not draw.
    let Cells {
        wait: (countdown, countdown_style),
        note: (note, note_style),
        ..
    } = cells(d, mins);
    spans.extend([
        Span::styled(
            badge_label(&d.route_short),
            Style::default().bg(bg).fg(fg).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            format!("{:<w$}", truncate(&d.headsign, c.toward), w = c.toward),
            Style::default().fg(DIM),
        ),
        Span::raw(" "),
        Span::styled(format!("{countdown:>WAIT_W$}"), countdown_style),
        Span::raw("  "),
        Span::styled(note, note_style),
    ]);
    Line::from(spans)
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
pub(super) fn hit_line(h: &crate::db::StopHit, c: &Wide) -> Line<'static> {
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
pub(super) fn plain_line(row: &Row, c: &Plain) -> Line<'static> {
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
    let spans = vec![
        Span::styled(label, style),
        Span::raw(" ".repeat(gap)),
        Span::styled(
            truncate(&row.secondary(), c.detail),
            Style::default().fg(DIM),
        ),
    ];
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Mode;
    use crate::db::{Direction, Route};

    fn route(colour: &str) -> Route {
        Route {
            short_name: "75".into(),
            long_name: "x".into(),
            color: colour.into(),
            route_ids: vec!["75".into()],
        }
    }

    fn directions(route: Route) -> Screen {
        Screen::Directions {
            mode: Mode::Bus,
            route,
            filter: String::new(),
        }
    }

    #[test]
    fn only_the_screens_below_a_route_carry_a_gutter() {
        // It says "these belong to the route you picked". The first screens
        // are above any route, and a board draws its own per-row badges.
        let r = route("0057B8");
        assert!(gutter(&Screen::Mode).is_none(), "mode");
        assert!(
            gutter(&Screen::Routes {
                mode: Mode::Bus,
                filter: String::new(),
            })
            .is_none(),
            "routes"
        );
        assert!(gutter(&directions(r.clone())).is_some(), "directions");
        assert!(
            gutter(&Screen::Stops {
                mode: Mode::Bus,
                route: r,
                dir: Direction {
                    headsign: "Elmvale".into(),
                    trips: 7,
                },
                filter: String::new(),
            })
            .is_some(),
            "stops"
        );
    }

    #[test]
    fn the_gutter_takes_the_line_colour_only_when_it_reads_as_a_rule() {
        for (colour, want) in [
            ("0057B8", Color::Rgb(0x00, 0x57, 0xB8)),
            ("6D6E70", RULE), // the exact grey the pole numbers use
            ("FFFFFF", RULE), // outshines the stop names
            ("nonsense", RULE),
        ] {
            let screen = directions(route(colour));
            let span = gutter(&screen).expect("directions carries a gutter");
            assert_eq!(span.style.fg, Some(want), "route colour {colour}");
        }
    }

    #[test]
    fn the_layout_gives_the_gutter_its_own_cells() {
        // The gutter is drawn before the label, so the detail column has to be
        // told it has that much less room. Asserted on the arithmetic: every
        // secondary on these screens ("#1902", "70 trips today") is far shorter
        // than the cap, so a rendered row cannot show the miscalculation.
        let rows = [Row::Direction(Direction {
            headsign: "Elmvale".into(),
            trips: 70,
        })];
        let without = Cols::new(&rows, 74, 0);
        let with = Cols::new(&rows, 74, 2);
        assert_eq!(
            without.plain.detail - with.plain.detail,
            2,
            "the detail column did not give up the gutter's cells"
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
