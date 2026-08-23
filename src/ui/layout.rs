//! One row, turned into a line: the column widths, and the spans that fill them.
//!
//! Nothing here draws a frame or consults a screen. A row is handed a set of
//! widths and reports what it looks like.

use super::palette::{ACCENT, DIM, FG, RULE, badge};
use crate::app::Row;
use ratatui::{
    style::{Modifier, Style},
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

/// Both column sets for one frame.
///
/// Two usizes and four: cheaper to compute both than to thread the choice
/// through the row loop, and it means neither type ever carries a field that
/// does not apply to it.
pub(super) struct Cols {
    plain: Plain,
    wide: Wide,
}

impl Cols {
    /// The line for one row. Which layout applies is the row's business, so
    /// the caller never has to reach in and choose.
    pub(super) fn line(&self, row: &Row) -> Line<'static> {
        match row {
            Row::Hit(h) => hit_line(h, &self.wide),
            other => plain_line(other, &self.plain),
        }
    }

    pub(super) fn new(rows: &[Row], width: usize) -> Self {
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
    Line::from(vec![
        Span::styled(label, style),
        Span::raw(" ".repeat(gap)),
        Span::styled(
            truncate(&row.secondary(), c.detail),
            Style::default().fg(DIM),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

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
