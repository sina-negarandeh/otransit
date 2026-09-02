//! The colours, and the arithmetic that picks between them.
//!
//! Kept apart from layout and rendering because it is the one part of the UI
//! with no notion of rows, widths or frames: given a colour, it answers
//! whether it can be read.

use ratatui::style::{Color, Modifier, Style};

pub(super) const DIM: Color = Color::Rgb(0x6d, 0x6e, 0x70);

/// The body text, as channels: the gutter has to weigh a route's colour
/// against it, and that needs numbers rather than a `Color`.
const FG_RGB: (u8, u8, u8) = (0xe6, 0xe6, 0xe6);
pub(super) const FG: Color = Color::Rgb(FG_RGB.0, FG_RGB.1, FG_RGB.2);

/// Brand red. Marks the current choice and the active filter.
pub(super) const ACCENT: Color = Color::Rgb(0xDA, 0x38, 0x39);

/// Dimmer than DIM — for rules and separators that should recede entirely.
/// Exposed as channels because `logo.rs` paints its ground line with it: the
/// two rules bracketing the app have to match, and a second literal could not
/// be made to.
pub const RULE_RGB: (u8, u8, u8) = (0x3a, 0x3b, 0x3d);
pub(super) const RULE: Color = Color::Rgb(RULE_RGB.0, RULE_RGB.1, RULE_RGB.2);

pub(super) const INK: Color = Color::Rgb(0x0c, 0x0c, 0x0c);

/// The traffic light. Green is fine, amber is off-nominal, red is wrong.
///
/// Named once because three places read them: how a trip is doing, how soon it
/// leaves, and whether something is published about the route. They were
/// previously locals in one function and repeated literals in another, which
/// is how a fourth caller ends up inventing a fifth warm colour.
///
/// `RED` is not `ACCENT`. The brand red marks the cursor and means "you are
/// here"; this one is lighter and means "this is wrong". Two reds, because
/// they answer different questions and both have to be legible at once.
pub(super) const GREEN: Color = Color::Rgb(0x5c, 0xd6, 0x8a);
pub(super) const AMBER: Color = Color::Rgb(0xff, 0xc1, 0x07);
pub(super) const RED: Color = Color::Rgb(0xff, 0x6b, 0x6b);

pub(super) fn hex(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some(((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// WCAG relative luminance of an sRGB colour.
pub(super) fn luminance(r: u8, g: u8, b: u8) -> f32 {
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

/// Width of the status column: the longest word it holds is "cancelled".
pub(super) const NOTE_W: usize = 9;

/// Whether anything is tracking this bus, and how it is doing.
///
/// Shared by the board and by a pinned stop, which is a board one row long.
/// Written once because the distinction it draws is load-bearing: `sched` means
/// nothing is tracking this trip, and a row that hid that would be confidently
/// wrong in the way this app most wants to avoid.
pub(super) fn status(d: &crate::db::Departure) -> (String, Style) {
    if d.canceled {
        return (
            "cancelled".to_string(),
            Style::default().fg(RED).add_modifier(Modifier::BOLD),
        );
    }
    let Some(live) = d.live else {
        return ("sched".to_string(), Style::default().fg(RULE));
    };
    match crate::app::lateness(live, d.secs) {
        -1..=1 => ("on time".into(), Style::default().fg(GREEN)),
        l if l > 0 => (format!("{l} late"), Style::default().fg(AMBER)),
        l => (format!("{} early", -l), Style::default().fg(DIM)),
    }
}

/// The countdown, and the colour it wears.
///
/// Shared by the board and by a pinned stop, for the same reason `status` is.
/// A cancelled trip has no countdown to give, so the em dash says so and the
/// urgency colours must not fire. Amber means "off-nominal but not wrong"
/// everywhere else here, and beside the word "cancelled" it reads as a bus you
/// can still catch — on this board the colour is read before the word.
///
/// The pin had only `status`, so it printed the word and kept the number.
pub(super) fn wait(d: &crate::db::Departure, mins: i32) -> (String, Style) {
    if d.canceled {
        return ("\u{2014}".to_string(), Style::default().fg(DIM));
    }
    (crate::app::fmt_wait(mins), urgency(mins))
}

/// How much to trust the clock time beside a departure.
///
/// The board's column only. A pin has no clock time, because the wait and the
/// time say the same thing given the hour, and only one of them answers
/// "should I leave now".
///
/// It lives here anyway, beside `status` and `wait`, because all three begin by
/// asking whether the trip is cancelled. Kept apart, one of them gets updated
/// and the others keep looking correct — which is the whole story of the bug
/// that produced `wait`.
pub(super) fn time_style(d: &crate::db::Departure) -> Style {
    if d.canceled {
        return Style::default().fg(DIM).add_modifier(Modifier::CROSSED_OUT);
    }
    if d.live.is_some() {
        return Style::default().fg(FG);
    }
    Style::default().fg(DIM)
}

/// Colour by urgency: the board should be readable in peripheral vision.
fn urgency(mins: i32) -> Style {
    match mins {
        m if m < 0 => Style::default().fg(DIM).add_modifier(Modifier::CROSSED_OUT),
        m if m <= 2 => Style::default().fg(RED).add_modifier(Modifier::BOLD),
        m if m <= 6 => Style::default().fg(AMBER).add_modifier(Modifier::BOLD),
        m if m <= 15 => Style::default().fg(GREEN),
        _ => Style::default().fg(DIM),
    }
}

/// Whether a colour can carry a rule beside this palette's text.
///
/// The two it rejects are rejected for colliding with the text next to them,
/// not for being dull: white outshines the stop names, and the dim grey is
/// exactly what the pole numbers use. Brightness alone is the wrong test —
/// an O-Train line's red is darker than that grey and is the colour most
/// worth having.
pub(super) fn reads_as_a_rule(colour: (u8, u8, u8)) -> bool {
    let (r, g, b) = colour;
    let (fr, fg, fb) = FG_RGB;
    luminance(r, g, b) < luminance(fr, fg, fb) && Color::Rgb(r, g, b) != DIM
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A departure four minutes out, running or cancelled.
    ///
    /// Four minutes because that is the amber band. At sixty the urgency
    /// colour is already the same dim grey a cancellation wears, so a test
    /// there could not tell the two apart.
    fn departure(canceled: bool) -> crate::db::Departure {
        crate::db::Departure {
            secs: 9 * 3600 + 4 * 60,
            trip_id: "t5".into(),
            route_short: "5".into(),
            route_color: "0057B8".into(),
            headsign: "Elmvale".into(),
            after_midnight: false,
            live: None,
            canceled,
        }
    }

    #[test]
    fn a_cancelled_trip_has_no_countdown_and_no_urgency_colour() {
        // A pin printed "cancelled" and kept an amber "4 min" beside it. Amber
        // means "off-nominal but not wrong", so the colour said the bus was
        // catchable while the word said it was not.
        let (text, style) = wait(&departure(true), 4);
        assert_eq!(text, "\u{2014}", "a bus that is not coming has no wait");
        assert_eq!(style.fg, Some(DIM), "wore an urgency colour");
    }

    #[test]
    fn a_running_trip_keeps_its_countdown_and_its_urgency_colour() {
        // The other half of the branch: suppressing the countdown for
        // everything would pass the test above and break the whole board.
        let (text, style) = wait(&departure(false), 4);
        assert_eq!(text, crate::app::fmt_wait(4));
        assert_eq!(style.fg, Some(AMBER), "four minutes is the amber band");
    }

    #[test]
    fn a_cancelled_clock_time_is_struck_through() {
        // The third rule a cancelled row follows, and the one that stays on
        // the board because a pin has no clock column to strike.
        assert!(
            time_style(&departure(true))
                .add_modifier
                .contains(Modifier::CROSSED_OUT)
        );
        assert!(
            !time_style(&departure(false))
                .add_modifier
                .contains(Modifier::CROSSED_OUT),
            "struck through a bus that is running"
        );
    }

    #[test]
    fn brightness_is_not_the_test_for_a_gutter_colour() {
        // Line 1's red is *darker* than the dim grey it is preferred over, so
        // a luminance threshold would reject the one colour most worth having.
        assert!(luminance(0xD3, 0x0F, 0x1D) < luminance(0x6D, 0x6E, 0x70));
        assert!(reads_as_a_rule((0xD3, 0x0F, 0x1D)), "an O-Train line's red");
        assert!(
            reads_as_a_rule((0x00, 0x57, 0xB8)),
            "the blue many buses use"
        );
        assert!(
            !reads_as_a_rule((0xFF, 0xFF, 0xFF)),
            "white outshines the names"
        );
        assert!(!reads_as_a_rule((0x6D, 0x6E, 0x70)), "the dim text colour");
        // The bound is "as bright as", not "brighter than": a rule the exact
        // colour of the stop names beside it is not a rule either.
        assert!(!reads_as_a_rule(FG_RGB), "the body-text colour itself");
    }
}
