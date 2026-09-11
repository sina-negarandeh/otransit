//! Replay a recorded session against fixed inputs and print every frame.
//!
//! The browser's output depends on four things it does not control: the cache,
//! the three live feeds, the wall clock, and which keys were pressed. Supply
//! all four from a directory and the output becomes a pure function of that
//! directory. Two builds of this app — or two implementations of it — can then
//! be compared with `diff`.
//!
//! That is the point. `screenshot` reads the real cache and fetches live, so it
//! shows you what the app looks like today. This shows whether two things agree.
//!
//! Every key goes through the same `handle_key` the event loop uses. A replay
//! that dispatched keys its own way would be an oracle for a program nobody
//! runs.

use crate::app::App;
use anyhow::{Context, Result, bail};
use crossterm::event::KeyCode;
use ratatui::{Terminal, backend::TestBackend};
use std::path::Path;

mod artifact;
mod script;
mod wire;
mod world;

use artifact::{style_row, text_row};
use script::{Step, key, parse};
use wire::Wire;
use world::World;

/// Every fixture directory holds one of these, and it is what marks a
/// directory as a fixture at all.
const SCRIPT: &str = "script";

/// One frame, and the step that produced it.
///
/// Three observations of one moment, compared independently. None outranks
/// another as a contract: `semantic` is reported first only because a
/// difference there usually explains the other two.
pub struct Frame {
    pub label: String,
    /// What a person sees. Plain text, no escapes.
    pub rows: Vec<String>,
    /// The colours, as `row  start..end style` lines, for the rows that have
    /// any. Kept apart from the text because a port gets the text right long
    /// before the colours, and because escapes would encode how a writer emits
    /// a run rather than which colour was chosen.
    pub styles: Vec<String>,
    /// What the app decided, as the vocabulary in `semantic`.
    pub semantic: serde_json::Value,
}

impl Frame {
    /// Draw one frame and observe it three ways.
    ///
    /// On `Frame` rather than inside the loop that produced it, because what a
    /// frame *is* belongs to the type the doc above describes. The three are
    /// taken from one draw of one app, so they cannot describe different
    /// moments -- which is the property the semantic layer's first rule exists
    /// to protect.
    fn capture(
        term: &mut Terminal<TestBackend>,
        app: &mut App,
        wire: &Wire,
        label: String,
    ) -> Result<Self> {
        let buf = crate::dev::capture(term, app)?;
        let (w, h) = (buf.area.width, buf.area.height);
        Ok(Frame {
            label,
            rows: (0..h).map(|y| text_row(&buf, y, w)).collect(),
            styles: (0..h)
                .filter_map(|y| style_row(&buf, y, w).map(|runs| format!("{y:>3}  {runs}")))
                .collect(),
            semantic: crate::semantic::snapshot(app, wire.observed()),
        })
    }

    /// The frame after `q`. Nothing is drawn, so there is nothing to observe,
    /// and saying that once is better than three empty literals at the call.
    fn quit() -> Self {
        Frame {
            label: "quit".into(),
            rows: vec![],
            styles: vec![],
            semantic: serde_json::Value::Null,
        }
    }
}

/// Which observations to print. The text always; the others on request, so
/// each can be diffed on its own.
pub struct Show {
    pub styles: bool,
    pub semantic: bool,
}

/// Read the directory, replay the script, and print every frame.
///
/// `dir` is either one fixture, or a directory of them: a folder with no
/// `script` of its own is walked in name order and every fixture under it is
/// replayed. One command, one artifact, however many worlds it covers.
pub fn run(dir: &Path, show: &Show) -> Result<()> {
    let found = fixtures(dir)?;
    let many = found.len() > 1 || found.first().is_some_and(|f| f != dir);
    for fixture in &found {
        if many {
            let name = fixture.file_name().unwrap_or(fixture.as_os_str());
            println!("\n======== {} ========", name.to_string_lossy());
        }
        for (n, f) in frames(fixture)?.iter().enumerate() {
            println!("\n--- {n}. {} ---", f.label);
            for row in &f.rows {
                println!("{row}");
            }
            if show.styles && !f.styles.is_empty() {
                println!("styles");
                for line in &f.styles {
                    println!("{line}");
                }
            }
            if show.semantic && !f.semantic.is_null() {
                // Pretty, because one value per line is what makes a diff point
                // at the decision that changed rather than at the whole frame.
                println!("semantic");
                println!("{:#}", f.semantic);
            }
        }
    }
    Ok(())
}

/// The fixtures under `dir`: itself if it holds a script, otherwise every
/// subdirectory that holds one, in name order so the artifact is stable.
///
/// Shared with `semantic`, whose tests read every fixture too. A second walk
/// there had its own `read_dir`, its own `"script"` literal, and no sort.
pub(crate) fn fixtures(dir: &Path) -> Result<Vec<std::path::PathBuf>> {
    if dir.join(SCRIPT).exists() {
        return Ok(vec![dir.to_path_buf()]);
    }
    let mut found: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join(SCRIPT).exists())
        .collect();
    found.sort();
    if found.is_empty() {
        bail!(
            "{} holds no {SCRIPT}, and no directory under it does",
            dir.display()
        );
    }
    Ok(found)
}

/// The frames themselves.
///
/// Separate from `run` for the reason `render` is separate from `print`: this
/// goes to stdout, so nothing else could look at it.
pub fn frames(dir: &Path) -> Result<Vec<Frame>> {
    let script = std::fs::read_to_string(dir.join(SCRIPT))
        .with_context(|| format!("reading {}", dir.join(SCRIPT).display()))?;
    let (setup, steps) = parse(&script)?;

    // `pins` is held, not dropped: it owns the copy the app writes to, and it
    // removes that copy when this returns.
    let World {
        conn,
        rt,
        feeds,
        pins,
    } = World::open(dir, &setup)?;
    let mut app = App::fixed(conn, setup.clock(), pins.path(), rt, feeds)?;

    // The app's own polling policy, on the script's clock. A fixture that names
    // no outcomes never polls: it was recorded before this existed, and its
    // realtime is whatever `rt.json` said at the start.
    let mut wire = Wire::new(dir, &setup, app.epoch());
    let mut term = Terminal::new(TestBackend::new(setup.width, setup.height))?;

    wire.serve(&app)?;
    let mut out = vec![Frame::capture(&mut term, &mut app, &wire, "start".into())?];
    for step in &steps {
        match step {
            Step::Key(k) => crate::handle_key(&mut app, *k)?,
            Step::Type(t) => {
                for c in t.chars() {
                    crate::handle_key(&mut app, key(KeyCode::Char(c)))?;
                }
            }
            // The one step that is not a keypress. Time passing is an input
            // like any other, and the app does things with it: a departure goes
            // by, a countdown moves, and the realtime cadence comes round.
            Step::Wait(secs) => app.advance_to(app.now() + secs),
        }
        // After the step, because a key can open a board that wants predictions
        // and because time only moves on a `wait`.
        wire.serve(&app)?;
        out.push(Frame::capture(&mut term, &mut app, &wire, step.label())?);
        if app.quit {
            out.push(Frame::quit());
            break;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::conformance::{fixture, suite};

    /// The recorded session this module's own assertions use.
    fn recorded() -> Vec<Frame> {
        fixture("drilldown")
    }

    #[test]
    fn every_fixture_in_the_suite_replays() {
        // The suite is one artifact. A directory that stopped replaying would
        // otherwise be noticed only by whoever next ran the command by hand.
        let found = fixtures(&suite()).expect("the suite has no fixtures");
        assert!(found.len() >= 13, "only {} fixtures", found.len());
        for dir in &found {
            let name = dir.file_name().unwrap_or_default().to_string_lossy();
            let frames = frames(dir).unwrap_or_else(|e| panic!("{name} did not replay: {e:#}"));
            assert!(!frames.is_empty(), "{name} drew nothing");
            // Every fixture shares one slice, so each is a session rather than
            // a copy of the schedule.
            assert!(
                !dir.join(script::CACHE).exists(),
                "{name} carries its own cache instead of sharing the suite's"
            );
        }
    }

    #[test]
    fn the_pin_is_live_before_anything_is_pressed() {
        // `build` loads the first screen off the timetable, and the realtime is
        // already in hand, so the frame is only right because `capture` applies
        // it. Drawn without that the pin reads `sched` and four minutes early
        // while the board for the same stop, one keypress later, reads late.
        let frames = recorded();
        let pin = frames
            .first()
            .expect("a replay draws before it presses anything")
            .rows
            .iter()
            .find(|r| r.contains("TRANSITWAY / TERMINAL"))
            .expect("the fixture pins TRANSITWAY / TERMINAL");
        assert!(
            !pin.contains("sched"),
            "the pin opened on the timetable with a prediction in hand: {pin:?}"
        );
    }

    #[test]
    fn replaying_twice_gives_the_same_frames() {
        // The whole point. If this drifts, the fixture is not a fixed world and
        // nothing can be compared against it.
        let (a, b) = (recorded(), recorded());
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.rows, y.rows, "frame {:?} changed between runs", x.label);
        }
    }
}
