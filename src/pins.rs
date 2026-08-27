//! Pinned stops: the small file that survives between runs.
//!
//! The rest of the app is stateless by design — every launch starts at the top.
//! This is the one exception, and it is deliberately the smallest one that
//! works: a handful of lines naming stops you check often, so the cursor lands
//! on the answer instead of on the first question.
//!
//! It lives beside the config, not beside the cache. The cache is safe to
//! delete and deleting it is a documented way to recover from a bad ingest;
//! these are the only thing here a person cannot get back.

use crate::db::{self, StopRow};
use anyhow::Result;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

/// Whether the board on screen can be pinned, and whether it already is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PinState {
    Pinned,
    Unpinned,
    /// Not pinned, and there is no room. Said out loud rather than letting the
    /// key look broken.
    Full,
}

/// The pinned stops: what the file remembers, what the cache can still resolve,
/// and where changes are written back.
///
/// One type rather than three fields on `App`, because the first two have to
/// move together. Every add and every removal touches both, and the invariant
/// that they agree is what makes a pin drawable, unpinnable, and countable
/// against the cap. Held here, a caller cannot get it half right.
pub struct Pins {
    /// Every pin in the file, resolvable or not. A stop that vanishes in one
    /// export and returns in the next brings its pin back, so nothing is
    /// dropped just because today's cache cannot find it.
    stored: Vec<Pin>,
    /// The ones that can be drawn, in the order they were pinned. Resolved
    /// once: the cache does not change while the app runs.
    live: Vec<StopRow>,
    /// `None` when this app does not persist them: tests, and platforms with
    /// no config directory.
    path: Option<PathBuf>,
    /// How many may be drawn. Supplied by the caller because it is a fact
    /// about the layout, which this module has no business knowing.
    cap: usize,
}

impl Pins {
    pub fn open(path: Option<PathBuf>, conn: &Connection, cap: usize) -> Result<Self> {
        let stored = path.as_deref().map(load).unwrap_or_default();
        let ids: Vec<String> = stored.iter().map(|p| p.stop_id.clone()).collect();
        Ok(Self {
            live: db::stops_by_id(conn, &ids)?,
            stored,
            path,
            cap,
        })
    }

    /// The pins that can be drawn.
    pub fn live(&self) -> &[StopRow] {
        &self.live
    }

    /// Whether this stop is pinned, and whether another would fit.
    ///
    /// Counted against what is drawn rather than against the file: pins the
    /// cache cannot resolve are not shown, and counting them would report
    /// "pins full" over a list with room in it and no way to see why.
    pub fn state(&self, stop_id: &str) -> PinState {
        if self.live.iter().any(|s| s.stop_id == stop_id) {
            PinState::Pinned
        } else if self.live.len() >= self.cap {
            PinState::Full
        } else {
            PinState::Unpinned
        }
    }

    /// Pin this stop, or unpin it if it is already pinned.
    ///
    /// A pin that cannot be written still works for this session: a read-only
    /// config directory is not worth ending one over, and nothing else here
    /// treats an unusable side channel as fatal.
    pub fn toggle(&mut self, stop: &StopRow) {
        match self.state(&stop.stop_id) {
            PinState::Pinned => {
                self.stored.retain(|p| p.stop_id != stop.stop_id);
                self.live.retain(|s| s.stop_id != stop.stop_id);
            }
            PinState::Full => return,
            PinState::Unpinned => {
                self.stored.push(Pin {
                    stop_id: stop.stop_id.clone(),
                    code: stop.code.clone(),
                    name: stop.name.clone(),
                });
                // Pinnable means on screen, which means the cache resolves it.
                self.live.push(stop.clone());
            }
        }
        if let Some(path) = &self.path {
            let _ = save(path, &self.stored);
        }
    }
}

/// One pinned stop, as stored.
///
/// `code` and `name` are never read back — resolution goes through `stop_id`
/// against the current cache, so the displayed name is always the feed's. They
/// are written because the file is meant to be opened and edited by a person,
/// and a column of bare ids would tell them nothing about what they pinned.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Pin {
    pub stop_id: String,
    pub code: String,
    pub name: String,
}

/// Where the pins live. `None` when the platform has no config directory.
pub fn path() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.config_dir().join("otransit").join("pins"))
}

/// Read the file, ignoring anything that is not a well-formed line.
///
/// A missing file is not an error: it is what "no pins yet" looks like, and a
/// first run must not fail because of it.
pub fn load(path: &Path) -> Vec<Pin> {
    std::fs::read_to_string(path)
        .map(|s| parse(&s))
        .unwrap_or_default()
}

/// Write the file, creating the directory if this is the first pin.
pub fn save(path: &Path, pins: &[Pin]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, render(pins))?;
    Ok(())
}

/// Tab-separated, one pin per line: id, pole number, name.
///
/// Tabs rather than commas because stop names contain commas and slashes but
/// never tabs, so nothing needs escaping and the file stays editable by hand.
fn render(pins: &[Pin]) -> String {
    pins.iter()
        .map(|p| format!("{}\t{}\t{}\n", p.stop_id, p.code, p.name))
        .collect()
}

/// Lines that do not have three fields are skipped rather than failing the
/// read: a hand-edited file with a typo should cost one pin, not all of them.
fn parse(text: &str) -> Vec<Pin> {
    let mut seen = std::collections::HashSet::new();
    text.lines()
        .filter(|line| {
            // Hand-edited files repeat themselves. Two rows for one stop would
            // both draw, and one `p` would remove both, since unpinning matches
            // on the id.
            line.split('\t')
                .next()
                .is_some_and(|id| seen.insert(id.to_string()))
        })
        .filter_map(|line| {
            let mut cols = line.split('\t');
            let (id, code, name) = (cols.next()?, cols.next()?, cols.next()?);
            (!id.is_empty()).then(|| Pin {
                stop_id: id.to_string(),
                code: code.to_string(),
                name: name.to_string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pin(id: &str) -> Pin {
        Pin {
            stop_id: id.into(),
            code: "3009".into(),
            name: "RIDEAU A".into(),
        }
    }

    #[test]
    fn a_pin_survives_a_write_and_a_read() {
        let pins = vec![pin("1000"), pin("7591")];
        assert_eq!(parse(&render(&pins)), pins);
    }

    #[test]
    fn a_stop_name_with_punctuation_survives() {
        // Names carry commas, slashes and accents: "VANTAGE / AD. 303",
        // "Aéroport". Tab-separated so none of them need escaping.
        let odd = Pin {
            stop_id: "1".into(),
            code: "0806".into(),
            name: "TUNNEY'S PASTURE, BAY 4 / AD. 303 ~ Aéroport".into(),
        };
        assert_eq!(parse(&render(std::slice::from_ref(&odd))), vec![odd]);
    }

    #[test]
    fn a_broken_line_costs_one_pin_not_the_file() {
        // The file is meant to be hand-editable, so a typo must not read as
        // "you have no pins".
        let text = "1000\t3009\tRIDEAU A\nnonsense\n\n7591\t7591\tCHAPEL\n";
        let got = parse(text);
        assert_eq!(got.len(), 2, "kept the two well-formed lines: {got:?}");
        assert_eq!(got[1].stop_id, "7591");
    }

    #[test]
    fn no_file_is_no_pins_rather_than_an_error() {
        // The first run has no file, and must not fail because of it.
        assert!(load(Path::new("/nonexistent/otransit/pins")).is_empty());
    }

    #[test]
    fn one_stop_cannot_be_pinned_twice_by_a_hand_edited_file() {
        // Two rows for one stop would both draw, and a single `p` would remove
        // both, since unpinning matches on the id.
        let text = "1000\t3009\tRIDEAU A\n1000\t3009\tRIDEAU A\n7591\t7591\tCHAPEL\n";
        let ids: Vec<String> = parse(text).into_iter().map(|p| p.stop_id).collect();
        assert_eq!(ids, ["1000", "7591"]);
    }

    #[test]
    fn pins_are_written_in_the_order_they_were_pinned() {
        // The list is a menu, not a set: where a row sits is muscle memory.
        let pins = vec![pin("c"), pin("a"), pin("b")];
        let ids: Vec<String> = parse(&render(&pins))
            .into_iter()
            .map(|p| p.stop_id)
            .collect();
        assert_eq!(ids, ["c", "a", "b"]);
    }
}
