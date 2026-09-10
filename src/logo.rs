//! Startup mark: the circle-on-a-pole you look for at an O-Train entrance,
//! with the name set beside it.
//!
//! The rule the pole stands on is not drawn here. It is the top edge of the
//! viewport, so it repaints, carries the weather, and stays on screen after
//! the mark itself has scrolled away.
//!
//! Cells are painted as *background colour on a space*, never as block glyphs.
//! SF Mono's U+2588 doesn't fill the cell and it has no shade characters at
//! all, so macOS falls back to a font with different metrics and the art
//! shears. Background paint is drawn by the terminal, not the font.
//!
//! Printed to stdout before the inline viewport exists, so it lands in
//! scrollback instead of being repainted every frame.

/// Rows in the ring. Columns follow from ASPECT.
const RING_ROWS: usize = 7;
/// Ring width as a fraction of the radius.
const THICKNESS: f32 = 0.42;
const POLE_ROWS: usize = 2;
const POLE_W: usize = 2;
/// Terminal cells are about twice as tall as they are wide, so the horizontal
/// radius has to be doubled for the circle to actually look round.
const ASPECT: f32 = 2.0;

type Rgb = (u8, u8, u8);
const RED: Rgb = (0xDA, 0x38, 0x39);
const POLE: Rgb = (0x6d, 0x6e, 0x70);
const MUTED: Rgb = (0x6d, 0x6e, 0x70);
const GROUND: Rgb = crate::ui::RULE_RGB;
const NAME: &str = "otransit";
const TAGLINE: &str = "OC Transpo schedules in your terminal";
/// What the one-line mark says. Shorter on purpose: that path exists because
/// the terminal is too narrow for the full mark, so it is too narrow for the
/// full tagline too.
const TAGLINE_SHORT: &str = "OC Transpo schedules";

#[derive(Clone, Copy, PartialEq)]
enum Cell {
    Empty,
    Ring,
    Pole,
}

/// The mark as a grid of cells: ring on top, pole hanging below it.
fn build() -> Vec<Vec<Cell>> {
    let ry = RING_ROWS as f32 / 2.0;
    let rx = ry * ASPECT;
    let width = (rx * 2.0).round() as usize;
    let (cy, cx) = ((RING_ROWS - 1) as f32 / 2.0, (width - 1) as f32 / 2.0);

    let mut grid: Vec<Vec<Cell>> = (0..RING_ROWS)
        .map(|y| {
            (0..width)
                .map(|x| {
                    let dx = (x as f32 - cx) / rx;
                    let dy = (y as f32 - cy) / ry;
                    let d = (dx * dx + dy * dy).sqrt();
                    if (1.0 - THICKNESS..=1.0).contains(&d) {
                        Cell::Ring
                    } else {
                        Cell::Empty
                    }
                })
                .collect()
        })
        .collect();

    let start = (cx - POLE_W as f32 / 2.0 + 0.5) as usize;
    for _ in 0..POLE_ROWS {
        let mut row = vec![Cell::Empty; width];
        let end = (start + POLE_W).min(width);
        row[start..end].fill(Cell::Pole);
        grid.push(row);
    }
    grid
}

fn fg(c: Rgb) -> String {
    format!("\x1b[38;2;{};{};{}m", c.0, c.1, c.2)
}

fn bg(c: Rgb) -> String {
    format!("\x1b[48;2;{};{};{}m", c.0, c.1, c.2)
}

/// Terminal capability probe. Block art depends on the terminal painting
/// adjacent cells with no seam horizontally *and* no gap vertically; different
/// terminals and line-height settings break it in different ways, so print the
/// cases separately and let the eye decide which one fails.
pub fn selftest() {
    let red = bg(RED);
    println!("\n  A · one painted row (background colour on spaces)");
    println!("    {red}                    \x1b[0m");

    println!("\n  B · four stacked painted rows, should be one solid block,");
    println!("      with no horizontal lines between the rows");
    for _ in 0..4 {
        println!("    {red}                    \x1b[0m");
    }

    println!("\n  C · the same block drawn with U+2588 glyphs instead");
    for _ in 0..4 {
        println!("    {}{}\x1b[0m", fg(RED), "\u{2588}".repeat(20));
    }

    println!("\n  D · shade glyphs, often missing from the font entirely");
    println!(
        "    {}{}  {}  {}\x1b[0m",
        fg(RED),
        "\u{2591}".repeat(6),
        "\u{2592}".repeat(6),
        "\u{2593}".repeat(6)
    );

    println!("\n  E · a ring, painted (what the big logo uses)");
    print_alone(200);

    println!("  If B has lines through it, your terminal's line height is above");
    println!("  1.0. Block art cannot be solid until that is 1.0.");
    println!("  If C is broken but B is fine, it is the font's block glyph.\n");
}

/// The circle-and-pole mark, for the banner above the viewport.
///
/// No rule: the viewport's top edge is what the pole stands on, and it draws
/// itself directly under this.
pub fn print(term_width: u16) {
    print!("{}", render(term_width));
}

/// The mark with nothing after it, so it draws the rule itself.
///
/// `otransit logo` and the self-test open no viewport, so nothing else will
/// draw the line. Without this the pylon ends in mid-air, which is the whole
/// design failing quietly on the one command whose only job is to show it.
pub fn print_alone(term_width: u16) {
    print!("{}", render_alone(term_width));
}

/// The mark and its rule, as they will appear. Separate from `print_alone` for
/// the same reason `render` is separate from `print`: this goes to stdout, so
/// nothing else can see it.
fn render_alone(term_width: u16) -> String {
    format!("{}{}", render(term_width), ground(term_width))
}

/// The rule the pole stands on, full width.
///
/// The same colour as the two rules the app draws, taken from there rather
/// than restated: a mark that stood on a different grey would not look like it
/// was standing on the app.
fn ground(width: u16) -> String {
    if width == 0 {
        return String::new();
    }
    format!("{}{}\x1b[0m\n", fg(GROUND), "─".repeat(width as usize))
}

/// The mark as it will appear, escapes and all.
///
/// Separate from `print` so the shape can be asserted on: this goes to stdout
/// before the viewport exists, which means nothing else can see it.
fn render(term_width: u16) -> String {
    let grid = build();
    let mark_w = grid[0].len();
    let gutter = 3;
    let needed = mark_w + gutter + TAGLINE.chars().count() + 4;

    if (term_width as usize) < needed {
        return format!(
            "\n{}\x1b[1m{NAME}\x1b[0m  {}{TAGLINE_SHORT}\x1b[0m\n",
            fg(RED),
            fg(MUTED)
        );
    }

    // Set the text against the middle of the ring, ignoring the pole.
    let name_row = RING_ROWS / 2 - 1;

    let mut out = String::from("\n");
    for (y, row) in grid.iter().enumerate() {
        let mut line = String::from("  ");
        let mut cur: Option<Rgb> = None;
        for cell in row {
            let want = match cell {
                Cell::Empty => None,
                Cell::Ring => Some(RED),
                Cell::Pole => Some(POLE),
            };
            if want != cur {
                line.push_str(&match want {
                    Some(c) => bg(c),
                    None => "\x1b[0m".to_string(),
                });
                cur = want;
            }
            line.push(' ');
        }
        line.push_str("\x1b[0m");
        line.push_str(&" ".repeat(gutter));

        if y == name_row {
            line.push_str(&format!("{}\x1b[1m{NAME}\x1b[0m", fg(RED)));
        } else if y == name_row + 2 {
            line.push_str(&format!("{}{TAGLINE}\x1b[0m", fg(MUTED)));
        }
        out.push_str(&line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mark_on_its_own_draws_the_rule_it_stands_on() {
        // `otransit logo` and the self-test open no viewport, so nothing else
        // will draw the line. Moving the rule into the viewport once left this
        // command showing a pylon standing on nothing, which is the whole
        // design failing on the one command whose only job is to show it.
        for width in [40u16, 200] {
            let out = render_alone(width);
            let rule = format!("{}\x1b[0m\n", "─".repeat(width as usize));
            assert!(
                out.ends_with(&rule),
                "width {width}: the mark does not end on its rule"
            );
            // And the banner form must not, or the viewport would draw a second.
            assert!(
                !render(width).ends_with(&rule),
                "width {width}: the banner drew a rule the viewport also draws"
            );
        }
    }

    #[test]
    fn the_mark_ends_flush_so_the_app_opens_against_it() {
        // The pole still stands on a rule, but the rule is the viewport's top
        // edge now, and the viewport opens at the cursor. So the half the mark
        // still owns is this one: end on a painted row, because a blank row
        // here would put a gap under the pylon and break the bracket.
        //
        // Checked on the raw string. The ring is background colour painted on
        // spaces, so a view with the escapes stripped cannot tell a mark row
        // from an empty one.
        for width in [40u16, 70, 200] {
            let out = render(width);
            assert!(
                out.ends_with('\n'),
                "width {width}: nothing to open against"
            );
            assert!(
                !out.ends_with("\n\n"),
                "width {width}: a blank row sits between the mark and the app"
            );
        }
    }

    #[test]
    fn the_narrow_mark_fits_the_terminals_that_ask_for_it() {
        // This path exists because the terminal is too narrow for the ring, so
        // a line that overflows it defeats the point. Reusing the full TAGLINE
        // here needs 47 cells and wraps on every terminal that gets here.
        for width in 30u16..59 {
            let line = render(width)
                .lines()
                .find(|l| l.contains(NAME))
                .expect("the narrow mark still names the program")
                .to_string();
            let plain: String = {
                let mut out = String::new();
                let mut chars = line.chars();
                while let Some(c) = chars.next() {
                    if c == '\x1b' {
                        for c in chars.by_ref() {
                            if c == 'm' {
                                break;
                            }
                        }
                    } else {
                        out.push(c);
                    }
                }
                out
            };
            assert!(
                plain.chars().count() <= width as usize,
                "width {width}: the mark is {} cells: {plain:?}",
                plain.chars().count()
            );
        }
    }

    #[test]
    fn the_ring_is_painted_with_background_colour_not_written_as_text() {
        // The mark is blocks of background colour on spaces. It was once
        // swapped for plain text, which renders as nothing at all.
        let ring = bg(RED);
        assert!(render(200).contains(&ring), "the wide mark lost its ring");
        assert!(
            !render(40).contains(&ring),
            "the narrow fallback must not try to paint one"
        );
    }

    #[test]
    fn both_widths_still_say_what_the_program_is() {
        for width in [40u16, 200] {
            assert!(render(width).contains(NAME), "width {width}");
        }
        assert!(
            render(200).contains(TAGLINE),
            "the wide mark carries the tagline"
        );
    }
}
