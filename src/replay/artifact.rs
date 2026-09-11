//! How a frame is written down, so that two implementations can be compared.
//!
//! The comparable form, not the readable one. `dev::render_line` turns a row
//! back into escapes for a terminal to show, which is a different job with a
//! different consumer: `screenshot` is for looking at, and this is for
//! diffing. The two live apart because only one of them is a contract.

use ratatui::{
    buffer::Buffer,
    style::{Color, Modifier},
};

/// One row of a frame as plain text.
///
/// What `replay` compares. Escapes are left out on purpose: they encode how
/// `render_line` chooses to emit a run, not what the app decided. A second
/// implementation would emit the same colours as different bytes and diff as
/// broken while being right. The colours are compared by `style_runs`, which
/// has no such freedom.
pub(super) fn text_row(buf: &Buffer, y: u16, w: u16) -> String {
    (0..w).map(|x| buf[(x, y)].symbol()).collect()
}

/// One row's colours, as maximal runs, or nothing if the row is all default.
///
/// `12..24 #da3839+b` is cells 12 up to 24 in brand red, bold. Maximal runs are
/// canonical — merge every neighbour that matches and there is exactly one way
/// to write a row — so this compares two implementations on the colours they
/// chose rather than on when either decided to emit an escape.
///
/// Modifiers are in here because the app leans on them: a cancelled time is dim
/// and struck through, and a bus due in two minutes is red and bold. Comparing
/// foreground alone would let a port drop every one of them and still pass.
pub(super) fn style_row(buf: &Buffer, y: u16, w: u16) -> Option<String> {
    let mut runs: Vec<(u16, u16, String)> = Vec::new();
    for x in 0..w {
        let cell = &buf[(x, y)];
        let style = style_key(cell.fg, cell.bg, cell.modifier);
        match runs.last_mut() {
            Some((_, end, prev)) if *prev == style && *end == x => *end = x + 1,
            _ => runs.push((x, x + 1, style)),
        }
    }
    let painted: Vec<String> = runs
        .iter()
        .filter(|(_, _, s)| s != DEFAULT_STYLE)
        .map(|(a, b, s)| format!("{a}..{b} {s}"))
        .collect();
    (!painted.is_empty()).then(|| painted.join("  "))
}

/// A cell with nothing asked of it. Written out nowhere, so a default run is
/// dropped rather than filling every frame with the colour of empty space.
const DEFAULT_STYLE: &str = "-";

/// `#da3839`, `#e6e6e6/#0c0c0c`, `-`, each with `+` and the modifier letters.
fn style_key(fg: Color, bg: Color, m: Modifier) -> String {
    let mut s = colour(fg);
    if bg != Color::Reset {
        s.push('/');
        s.push_str(&colour(bg));
    }
    // Bit order, so the letters of a style are always in the same sequence.
    let flags = [
        (Modifier::BOLD, 'b'),
        (Modifier::DIM, 'd'),
        (Modifier::ITALIC, 'i'),
        (Modifier::UNDERLINED, 'u'),
        (Modifier::REVERSED, 'r'),
        (Modifier::CROSSED_OUT, 'x'),
    ];
    let letters: String = flags
        .iter()
        .filter(|(bit, _)| m.contains(*bit))
        .map(|(_, c)| *c)
        .collect();
    if !letters.is_empty() {
        s.push('+');
        s.push_str(&letters);
    }
    s
}

fn colour(c: Color) -> String {
    match c {
        Color::Reset => DEFAULT_STYLE.to_string(),
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        // The UI draws truecolour only. A named colour would be a new decision,
        // and naming it here rather than printing a debug shape keeps the
        // artifact something a port can be held to.
        other => format!("{other:?}").to_lowercase(),
    }
}
