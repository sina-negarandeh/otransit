//! The recorded-session language: what a script may say, and how it is read.
//!
//! Text in, a world and a list of steps out. Nothing here knows that a replay
//! exists -- no app, no terminal, no fixture directory -- which is what lets a
//! reader answer "what does `feed ok` mean" without walking an event loop.

use crate::app::Clock;
use anyhow::{Context, Result, bail};
use chrono::{FixedOffset, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

/// The schedule a fixture reads when its script names no other.
pub(super) const CACHE: &str = "cache.db";

/// The longest a single `wait` may be: one day. A service day is under 48
/// hours, and the clock counting seconds into one is an `i32`, so an unbounded
/// wait overflows it rather than reaching anything a fixture wants to show.
const MAX_WAIT_SECS: i32 = 86_400;

/// One step of a recorded session.
#[derive(Debug)]
pub(super) enum Step {
    Key(KeyEvent),
    /// Each character in turn, as the filter receives them.
    Type(String),
    /// Seconds of nothing happening. The clock moves, the board is refreshed,
    /// and the realtime cadence gets the chance to come round.
    Wait(i32),
}

impl Step {
    pub(super) fn label(&self) -> String {
        match self {
            Step::Key(k) => match k.code {
                KeyCode::Char(c) => format!("key {c}"),
                other => format!("{other:?}").to_lowercase(),
            },
            Step::Type(t) => format!("type {t}"),
            Step::Wait(secs) => format!("wait {secs}s"),
        }
    }
}

/// What the realtime feed answers with, the next time the app asks.
///
/// The script names the answers. It does not name the moments: those come from
/// the app's own cadence, and the cadence is what this layer is here to compare.
/// An outcome is consumed when a request happens, so a port that polls twice as
/// often runs out of them and a port that never polls leaves them unused. Both
/// show up in the frames.
#[derive(Clone, Debug)]
pub(super) enum Outcome {
    /// A body, from a file in the fixture.
    Ok(PathBuf),
    /// A refusal, with the message the transport would have given.
    Failed(String),
}

/// What the feed answered: a payload, or the refusal it gave.
///
/// Deliberately not the same error as the one `attempt` returns. This one is
/// data the app must cope with, and a fixture exists to state it. That one is
/// the fixture itself being unreadable, which nothing should cope with -- a
/// missing body is an authoring mistake, and turning it into a refusal would
/// hide it as a feed outage the script never wrote.
pub(super) type Answer = std::result::Result<crate::rt::Realtime, anyhow::Error>;

impl Outcome {
    /// What the feed would have handed back, read out of the fixture.
    ///
    /// Here rather than in the loop that schedules attempts: that loop is about
    /// when the app asks, and this is about what it hears.
    pub(super) fn attempt(&self, dir: &std::path::Path) -> Result<Answer> {
        Ok(match self {
            Outcome::Ok(file) => {
                let path = dir.join(file);
                let bytes =
                    std::fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
                Ok(crate::rt::parse(&bytes)
                    .with_context(|| format!("parsing {}", file.display()))?)
            }
            Outcome::Failed(why) => Err(anyhow::anyhow!(why.clone())),
        })
    }
}

/// The world a script sets before its first step.
#[derive(Debug)]
pub(super) struct Setup {
    /// Where the schedule comes from, relative to the fixture. Named so that
    /// many fixtures share one real slice: the cache is the largest thing here
    /// and the least worth copying, and a suite whose worlds differ only in
    /// their feeds and their keys should differ only in those files.
    pub(super) cache: PathBuf,
    pub(super) date: NaiveDate,
    pub(super) now: i32,
    /// East of UTC. Named by the script rather than taken from the machine,
    /// because a realtime epoch read in the reader's timezone would put a
    /// different arrival on the board in Ottawa than in Berlin.
    pub(super) offset: FixedOffset,
    pub(super) width: u16,
    pub(super) height: u16,
    /// What the realtime answers with, in order. Part of the world rather than
    /// a step, because a script does not get to say when the app asks.
    pub(super) feed: Vec<Outcome>,
}

impl Setup {
    /// The clock this world is held at.
    pub(super) fn clock(&self) -> Clock {
        Clock::held(self.date, self.now, self.offset)
    }
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            cache: PathBuf::from(CACHE),
            date: NaiveDate::from_ymd_opt(2026, 8, 21).unwrap_or_default(),
            now: 9 * 3600,
            // Ottawa in summer. A fixture that says nothing gets the zone the
            // feed it is made of is published in -- never the machine's, which
            // is the one thing a replay must not read.
            offset: east(-4 * 60).expect("-04:00 is a real offset"),
            width: 100,
            // The viewport, plus the rows of scrollback above it that a frame
            // should show: enough to see that nothing was drawn up there.
            height: crate::ui::VIEWPORT_H + 3,
            feed: Vec::new(),
        }
    }
}

/// An offset `mins` east of UTC, or nothing if no zone on earth uses it.
fn east(mins: i32) -> Option<FixedOffset> {
    FixedOffset::east_opt(mins * 60)
}

pub(super) fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

/// A header of `name value` lines, then one step per line.
///
/// Blank lines and `#` comments are skipped, so a script can be annotated with
/// what it is trying to reach.
///
/// The header must come first, and a header line after the first step is an
/// error. It sets the world the whole run happens in, so one written half-way
/// down would read as a change of time or size at that point and be neither.
pub(super) fn parse(script: &str) -> Result<(Setup, Vec<Step>)> {
    let mut setup = Setup::default();
    let mut steps = Vec::new();
    for (n, raw) in script.lines().enumerate() {
        // Comments end a line, except inside a typed string: pole numbers are
        // written `#3009` everywhere in this app, so `type BANK#3009` has to
        // reach the filter whole rather than arriving as `BANK`.
        let line = if raw.trim_start().starts_with("type ") {
            raw.trim()
        } else {
            raw.split('#').next().unwrap_or("").trim()
        };
        if line.is_empty() {
            continue;
        }
        let (word, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();
        if matches!(word, "cache" | "date" | "time" | "offset" | "size" | "feed")
            && !steps.is_empty()
        {
            bail!(
                "line {}: {word:?} sets the world and must come before the first step",
                n + 1
            );
        }
        match word {
            "cache" => setup.cache = PathBuf::from(rest),
            "date" => {
                setup.date = rest
                    .parse()
                    .with_context(|| format!("line {}: date {rest:?}", n + 1))?;
            }
            "time" => setup.now = hhmm(rest).with_context(|| format!("line {}", n + 1))?,
            // One format, `±HH:MM`, which covers the half-hour zones as
            // `+05:30`. Accepting bare minutes as well meant `offset 5` was
            // five minutes east, when everyone writing it means five hours.
            "offset" => {
                let mins = hhmm(rest)
                    .map(|s| s / 60)
                    .with_context(|| format!("line {}: offset wants ±HH:MM", n + 1))?;
                setup.offset = east(mins)
                    .with_context(|| format!("line {}: {rest} is not a real offset", n + 1))?;
            }
            "size" => {
                let (w, h) = rest
                    .split_once(' ')
                    .with_context(|| format!("line {}: size wants width and height", n + 1))?;
                setup.width = w.trim().parse()?;
                setup.height = h.trim().parse()?;
                if setup.width == 0 || setup.height == 0 {
                    bail!("line {}: a frame with no cells shows nothing", n + 1);
                }
            }
            // `feed ok <file>` or `feed fail <message>`. Queued, never timed:
            // the app's own cadence decides when each one is taken.
            "feed" => {
                let (kind, arg) = rest.split_once(' ').unwrap_or((rest, ""));
                setup.feed.push(match kind {
                    "ok" => Outcome::Ok(PathBuf::from(arg.trim())),
                    "fail" => Outcome::Failed(arg.trim().to_string()),
                    other => bail!("line {}: feed wants ok or fail, not {other:?}", n + 1),
                });
            }
            // Seconds, so a cadence measured in seconds can be written plainly.
            // One optional `s`, not any number of them: every other directive
            // here rejects what it does not understand, and `wait 30sss` should
            // not quietly become thirty seconds.
            "wait" => {
                let secs: i32 = rest
                    .strip_suffix('s')
                    .unwrap_or(rest)
                    .parse()
                    .with_context(|| format!("line {}: wait wants seconds", n + 1))?;
                // A service day is under 48 hours and the clock is an i32 of
                // seconds into one. Unbounded, `wait 2147483600` overflowed it:
                // the clock ran backwards in a release build and the checked
                // build panicked.
                if !(1..=MAX_WAIT_SECS).contains(&secs) {
                    bail!(
                        "line {}: wait wants 1 to {MAX_WAIT_SECS} seconds, not {secs}",
                        n + 1
                    );
                }
                steps.push(Step::Wait(secs));
            }
            "up" => steps.push(Step::Key(key(KeyCode::Up))),
            "down" => steps.push(Step::Key(key(KeyCode::Down))),
            "enter" => steps.push(Step::Key(key(KeyCode::Enter))),
            "esc" => steps.push(Step::Key(key(KeyCode::Esc))),
            "backspace" => steps.push(Step::Key(key(KeyCode::Backspace))),
            "key" => {
                let c = rest
                    .chars()
                    .next()
                    .with_context(|| format!("line {}: key wants a character", n + 1))?;
                steps.push(Step::Key(key(KeyCode::Char(c))));
            }
            "type" => steps.push(Step::Type(rest.to_string())),
            other => bail!("line {}: {other:?} is not a step", n + 1),
        }
    }
    Ok((setup, steps))
}

/// `HH:MM` as seconds. Hours may pass 24, because a service day does: a trip
/// at `25:10` is what you catch at 01:10 the next morning. Minutes may not.
fn hhmm(s: &str) -> Result<i32> {
    let (h, m) = s.split_once(':').context("wants HH:MM")?;
    let (h, m) = (h.trim(), m.trim());
    let sign = if h.starts_with('-') { -1 } else { 1 };
    let mins: i32 = m.parse().with_context(|| format!("{m:?} is not minutes"))?;
    if !(0..60).contains(&mins) {
        bail!("{mins} is not a minute of the hour");
    }
    Ok(h.parse::<i32>()? * 3600 + sign * mins * 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_sets_the_world_before_it_presses_anything() {
        let (setup, steps) = parse(
            "# a comment\ndate 2026-01-02\ntime 17:05\nsize 80 12\n\ndown\nenter\ntype rideau\n",
        )
        .unwrap();
        assert_eq!(setup.date, NaiveDate::from_ymd_opt(2026, 1, 2).unwrap());
        assert_eq!(setup.now, 17 * 3600 + 5 * 60);
        assert_eq!((setup.width, setup.height), (80, 12));
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[2].label(), "type rideau");
    }

    #[test]
    fn a_pole_number_survives_the_comment_stripper() {
        // `#3009` is how this app writes a pole number, everywhere. A script
        // that searched for one used to get `BANK` and report success.
        let (_, steps) = parse("type BANK#3009\n").unwrap();
        assert_eq!(steps[0].label(), "type BANK#3009");
        // A comment on any other line still ends it.
        let (_, steps) = parse("down # go down\n").unwrap();
        assert_eq!(steps.len(), 1);
    }

    #[test]
    fn the_zone_is_named_rather_than_taken_from_the_machine() {
        // A realtime epoch read in the reader's timezone puts a different
        // arrival on the board in Ottawa than in Berlin, which would compare
        // two implementations on where they ran.
        //
        // Asserted through the clock, because the offset is only ever used to
        // decide where a service day starts: midnight in the named zone.
        let start = |off: &str| {
            parse(&format!("date 2026-01-02\ntime 00:00\noffset {off}\n"))
                .unwrap()
                .0
                .clock()
                .epoch()
        };
        // Derived from the zone that has no offset, rather than restated as a
        // pair of epochs nobody can check by eye.
        let utc = start("+00:00");
        assert_eq!(start("-04:00"), utc + 4 * 3600, "west of UTC starts later");
        assert_eq!(
            start("+05:30"),
            utc - 5 * 3600 - 30 * 60,
            "the half-hour zones are written ±HH:MM like every other"
        );
    }

    #[test]
    fn a_body_a_fixture_does_not_have_is_an_authoring_mistake() {
        // Two errors live here and must not be confused. A refusal is data the
        // app copes with. A missing body is the fixture being unreadable, and
        // returning it as a refusal would hide an authoring mistake as a feed
        // outage nobody wrote.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance/drilldown");
        let missing = Outcome::Ok(PathBuf::from("nope.json"));
        let err = missing.attempt(&dir).unwrap_err().to_string();
        assert!(err.contains("nope.json"), "{err}");

        // A refusal is not an error of that kind: it comes back as the answer.
        let refused = Outcome::Failed("http status: 503".into());
        let answer = refused
            .attempt(&dir)
            .expect("a refusal is not a broken fixture");
        assert_eq!(answer.unwrap_err().to_string(), "http status: 503");

        // And a body the fixture does have parses.
        assert!(
            Outcome::Ok(PathBuf::from("rt.json"))
                .attempt(&dir)
                .unwrap()
                .is_ok()
        );
    }

    #[test]
    fn a_wait_has_to_fit_the_day_it_moves_through() {
        // The clock is seconds into a service day, as an `i32`. Unbounded, a
        // wait overflowed it: the clock ran backwards in a release build and
        // the checked build panicked.
        assert!(parse("wait 2147483600\n").is_err());
        assert!(parse("wait 86401\n").is_err(), "longer than a day");
        assert!(parse("wait 0\n").is_err(), "waiting nothing is not waiting");
        assert!(parse("wait -30\n").is_err());
        assert_eq!(parse("wait 30s\n").unwrap().1.len(), 1);
        assert_eq!(parse("wait 30\n").unwrap().1.len(), 1, "the s is optional");
    }

    #[test]
    fn a_step_this_parser_half_understands_is_still_an_error() {
        // One optional `s`, not any number of them. Every other directive here
        // rejects what it does not understand, so that a typo cannot quietly
        // produce a world nobody meant.
        for bad in ["wait 30sss", "wait s", "wait thirty", "feed maybe rt.json"] {
            assert!(parse(&format!("{bad}\n")).is_err(), "{bad:?} was accepted");
        }
    }

    #[test]
    fn a_bare_number_is_not_an_offset() {
        // It used to be minutes, resolved by letting the ±HH:MM parse fail
        // first. `offset 5` meant five minutes east; anyone writing it means
        // five hours, and the frames would still compare cleanly against the
        // wrong world.
        assert!(parse("offset 5\n").is_err());
        assert!(parse("offset -330\n").is_err());
        // And an offset no zone on earth uses.
        assert!(parse("offset 40:00\n").is_err());
    }

    #[test]
    fn the_world_has_to_be_set_before_the_first_step() {
        // A `time` line half-way down reads as "and now it is 09:00", which is
        // not what it does: it sets the time the whole run started at. A script
        // that looked like that would replay something nobody wrote.
        let err = parse("down\ntime 09:00\n").unwrap_err().to_string();
        assert!(err.contains("line 2"), "{err}");
        assert!(err.contains("before the first step"), "{err}");
        for header in [
            "date 2026-01-02",
            "offset -04:00",
            "size 80 12",
            "feed ok rt.json",
        ] {
            assert!(
                parse(&format!("enter\n{header}\n")).is_err(),
                "{header:?} was accepted after a step"
            );
        }
    }

    #[test]
    fn a_header_that_makes_no_sense_stops_the_replay() {
        // A typo here produces a world nobody meant, and the frames still
        // compare cleanly -- against the wrong thing.
        for bad in ["time 08:99", "size 0 14", "size 100 0"] {
            assert!(parse(bad).is_err(), "{bad:?} was accepted");
        }
        // Hours past 24 are not a typo: a service day reaches 28:xx.
        assert_eq!(parse("time 25:10").unwrap().0.now, 25 * 3600 + 600);
    }

    #[test]
    fn an_unknown_step_stops_the_replay_rather_than_being_skipped() {
        // A typo that was quietly ignored would make two implementations
        // disagree for a reason neither of them caused.
        let err = parse("down\nwiggle\n").unwrap_err().to_string();
        assert!(err.contains("wiggle"), "{err}");
        assert!(err.contains("line 2"), "{err}");
    }

    #[test]
    fn the_default_world_is_the_one_the_tests_already_use() {
        // So a script that sets nothing still replays somewhere reproducible.
        let (setup, steps) = parse("").unwrap();
        assert!(steps.is_empty());
        assert_eq!(setup.now, 9 * 3600);
    }
}
