//! The colours, and the arithmetic that picks between them.
//!
//! Kept apart from layout and rendering because it is the one part of the UI
//! with no notion of rows, widths or frames: given a colour, it answers
//! whether it can be read.

use ratatui::style::{Color, Modifier, Style};

pub(super) const DIM: Color = Color::Rgb(0x6d, 0x6e, 0x70);

pub(super) const FG: Color = Color::Rgb(0xe6, 0xe6, 0xe6);

/// Brand red. Marks the current choice and the active filter.
pub(super) const ACCENT: Color = Color::Rgb(0xDA, 0x38, 0x39);

/// Dimmer than DIM — for rules and separators that should recede entirely.
/// Exposed as channels because `logo.rs` paints its ground line with it: the
/// two rules bracketing the app have to match, and a second literal could not
/// be made to.
pub const RULE_RGB: (u8, u8, u8) = (0x3a, 0x3b, 0x3d);
pub(super) const RULE: Color = Color::Rgb(RULE_RGB.0, RULE_RGB.1, RULE_RGB.2);

pub(super) const INK: Color = Color::Rgb(0x0c, 0x0c, 0x0c);

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

/// Colour by urgency: the board should be readable in peripheral vision.
pub(super) fn urgency(mins: i32) -> Style {
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
