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

use crate::app::{Board, Mode};
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
    /// The ones that can be drawn, in the order they were pinned, each rebuilt
    /// into the board it was pinned from. Resolved once: the cache does not
    /// change while the app runs.
    ///
    /// A board knows which departures belong to it, so the pin row, the screen
    /// `enter` opens, and the check for whether this board is already pinned
    /// all ask the same value rather than three reconstructions of it.
    live: Vec<Board>,
    /// `None` when this app does not persist them: tests, and platforms with
    /// no config directory.
    path: Option<PathBuf>,
    /// How many may be drawn. Supplied by the caller because it is a fact
    /// about the layout, which this module has no business knowing.
    cap: usize,
}

impl Pins {
    /// Read the file and rebuild each pin into the board it was made from.
    ///
    /// `routes` is every route running today, by mode, which `App` has already
    /// queried for its own counts. A pin naming a route that is not in it is
    /// hidden, exactly as one naming a missing stop is: the stop or the route
    /// can come back with the next export, and the line stays in the file.
    pub fn open(
        path: Option<PathBuf>,
        conn: &Connection,
        cap: usize,
        routes: &[(Mode, Vec<db::Route>)],
    ) -> Result<Self> {
        let stored = path.as_deref().map(load).unwrap_or_default();
        let ids: Vec<String> = stored.iter().map(|p| p.stop_id.clone()).collect();
        let stops = db::stops_by_id(conn, &ids)?;
        let live = stored
            .iter()
            .filter_map(|p| p.board(&stops, routes))
            .collect();
        Ok(Self {
            live,
            stored,
            path,
            cap,
        })
    }

    /// The boards that can be drawn.
    pub fn live(&self) -> &[Board] {
        &self.live
    }

    /// Whether this board is pinned, and whether another would fit.
    ///
    /// Counted against what is drawn rather than against the file: pins the
    /// cache cannot resolve are not shown, and counting them would report
    /// "pins full" over a list with room in it and no way to see why.
    pub fn state(&self, board: &Board) -> PinState {
        let want = Pin::of(board);
        if self.live.iter().any(|b| Pin::of(b).key() == want.key()) {
            PinState::Pinned
        } else if self.live.len() >= self.cap {
            PinState::Full
        } else {
            PinState::Unpinned
        }
    }

    /// Pin this board, or unpin it if it is already pinned.
    ///
    /// A pin that cannot be written still works for this session: a read-only
    /// config directory is not worth ending one over, and nothing else here
    /// treats an unusable side channel as fatal.
    pub fn toggle(&mut self, board: &Board) {
        let want = Pin::of(board);
        match self.state(board) {
            PinState::Pinned => {
                self.stored.retain(|p| p.key() != want.key());
                self.live.retain(|b| Pin::of(b).key() != want.key());
            }
            PinState::Full => return,
            PinState::Unpinned => {
                self.stored.push(want);
                // Pinnable means on screen, which means it already resolves.
                self.live.push(board.clone());
            }
        }
        if let Some(path) = &self.path {
            let _ = save(path, &self.stored);
        }
    }
}

/// One pinned board, as stored.
///
/// `code` and `name` are never read back — resolution goes through `stop_id`
/// against the current cache, so the displayed name is always the feed's. They
/// are written because the file is meant to be opened and edited by a person,
/// and a column of bare ids would tell them nothing about what they pinned.
///
/// `route` and `headsign` are both set or both absent. A pin made by drilling
/// carries them; one made from a search board does not, because you never said
/// where you were going, and "everything calling here" is the honest answer to
/// a question you did not ask.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Pin {
    pub stop_id: String,
    pub code: String,
    pub name: String,
    /// The route's short name, not its ids. Ids repeat across booking periods
    /// (`7` and `7-1`) and `update` replaces them wholesale, so the short name
    /// is the half that survives an export.
    pub route: Option<String>,
    pub headsign: Option<String>,
}

impl Pin {
    /// The pin this board would make.
    fn of(board: &Board) -> Self {
        let stop = board.stop();
        let (route, headsign) = match board {
            Board::Route { route, dir, .. } => {
                (Some(route.short_name.clone()), Some(dir.headsign.clone()))
            }
            Board::Stop { .. } => (None, None),
        };
        Self {
            stop_id: stop.stop_id.clone(),
            code: stop.code.clone(),
            name: stop.name.clone(),
            route,
            headsign,
        }
    }

    /// This pin as a board, against today's cache, or `None` if it no longer
    /// resolves.
    ///
    /// Both halves go through the current export. A `stop_id` can vanish, and
    /// so can a route: `update` replaces the database wholesale, and a route
    /// that is not running today is not in `routes` at all. Either way the row
    /// is hidden and the line kept, because both can come back.
    fn board(&self, stops: &[StopRow], routes: &[(Mode, Vec<db::Route>)]) -> Option<Board> {
        let stop = stops.iter().find(|s| s.stop_id == self.stop_id)?.clone();
        let (Some(want), Some(headsign)) = (&self.route, &self.headsign) else {
            return Some(Board::Stop { stop });
        };
        let (mode, route) = routes.iter().find_map(|(mode, rs)| {
            rs.iter()
                .find(|r| r.short_name == *want)
                .map(|r| (*mode, r.clone()))
        })?;
        Some(Board::Route {
            mode,
            route,
            // `trips` counts a direction's trips for the directions screen. A
            // pin never draws that number, and inventing one here would be a
            // fact this file does not have.
            dir: db::Direction {
                headsign: headsign.clone(),
                trips: 0,
            },
            stop,
        })
    }

    /// What makes two pins the same pin.
    ///
    /// The stop, and the route and direction it was pinned from. Not the name
    /// or the pole number: those are written for a person reading the file, and
    /// the feed rewrites them. Two routes calling at one stop are two pins,
    /// because two buses to one terminus are not two buses to one place — 44
    /// and 48 both end at Billings Bridge and the 48 runs a corridor the 44
    /// never touches.
    fn key(&self) -> (&str, Option<&str>, Option<&str>) {
        (
            &self.stop_id,
            self.route.as_deref(),
            self.headsign.as_deref(),
        )
    }
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

/// Tab-separated, one pin per line: id, pole number, name, then the route and
/// direction if it was pinned from a drilled board.
///
/// Tabs rather than commas because stop names contain commas and slashes but
/// never tabs, so nothing needs escaping and the file stays editable by hand.
///
/// A stop-scoped pin writes three fields, exactly as every pin did before
/// routes were recorded. Files written by the older version read back
/// unchanged and keep behaving as they did.
fn render(pins: &[Pin]) -> String {
    pins.iter()
        .map(|p| match (&p.route, &p.headsign) {
            (Some(route), Some(headsign)) => format!(
                "{}\t{}\t{}\t{}\t{}\n",
                p.stop_id, p.code, p.name, route, headsign
            ),
            _ => format!("{}\t{}\t{}\n", p.stop_id, p.code, p.name),
        })
        .collect()
}

/// Lines without at least three fields are skipped rather than failing the
/// read: a hand-edited file with a typo should cost one pin, not all of them.
fn parse(text: &str) -> Vec<Pin> {
    let mut seen = std::collections::HashSet::new();
    text.lines()
        .filter_map(|line| {
            let mut cols = line.split('\t');
            let (id, code, name) = (cols.next()?, cols.next()?, cols.next()?);
            // A route without a direction, or the reverse, is half a
            // fingerprint. Keep the stop and drop both rather than narrowing
            // on one of them.
            let route = cols.next().filter(|s| !s.is_empty());
            let headsign = cols.next().filter(|s| !s.is_empty());
            let (route, headsign) = match (route, headsign) {
                (Some(r), Some(h)) => (Some(r.to_string()), Some(h.to_string())),
                _ => (None, None),
            };
            (!id.is_empty()).then(|| Pin {
                stop_id: id.to_string(),
                code: code.to_string(),
                name: name.to_string(),
                route,
                headsign,
            })
        })
        // Hand-edited files repeat themselves. Two identical rows would both
        // draw, and one `p` would remove both, since unpinning matches on the
        // fingerprint. Two rows differing by route are two real pins.
        .filter(|p| seen.insert((p.stop_id.clone(), p.route.clone(), p.headsign.clone())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stop-scoped pin, as a search board makes.
    fn pin(id: &str) -> Pin {
        Pin {
            stop_id: id.into(),
            code: "3009".into(),
            name: "RIDEAU A".into(),
            route: None,
            headsign: None,
        }
    }

    /// A pin scoped to one route in one direction, as drilling makes.
    fn scoped(id: &str, route: &str, headsign: &str) -> Pin {
        Pin {
            route: Some(route.into()),
            headsign: Some(headsign.into()),
            ..pin(id)
        }
    }

    #[test]
    fn a_pin_survives_a_write_and_a_read() {
        let pins = vec![pin("1000"), pin("7591")];
        assert_eq!(parse(&render(&pins)), pins);
    }

    #[test]
    fn a_route_scoped_pin_survives_a_write_and_a_read() {
        let pins = vec![
            scoped("1000", "44", "Billings Bridge"),
            scoped("1000", "48", "Billings Bridge"),
        ];
        assert_eq!(parse(&render(&pins)), pins);
    }

    #[test]
    fn a_file_written_before_routes_were_recorded_still_reads() {
        // Three fields is what every pin was until one could name a route.
        // Those lines have to come back as stop-scoped pins, behaving exactly
        // as they did, rather than being skipped as malformed.
        let old = "1000\t3009\tRIDEAU A\n7591\t7591\tCHAPEL\n";
        let got = parse(old);
        assert_eq!(got.len(), 2, "an old file lost pins: {got:?}");
        assert!(
            got.iter().all(|p| p.route.is_none()),
            "an old pin came back scoped to a route it never named"
        );
    }

    #[test]
    fn one_stop_holds_two_pins_when_they_name_different_routes() {
        // The dedup key is the whole fingerprint. Keying on the stop alone is
        // what made pinning the 48 silently remove the 44.
        let text = "1000\t3009\tR\t44\tBillings Bridge\n1000\t3009\tR\t48\tBillings Bridge\n";
        let got: Vec<Option<String>> = parse(text).into_iter().map(|p| p.route).collect();
        assert_eq!(got, [Some("44".into()), Some("48".into())]);
    }

    #[test]
    fn a_line_naming_a_route_without_a_direction_keeps_the_stop() {
        // Half a fingerprint narrows on half a question. A hand-edited line
        // that lost its headsign falls back to the stop rather than filtering
        // to a route in every direction at once.
        let got = parse("1000\t3009\tRIDEAU A\t44\n");
        assert_eq!(got.len(), 1);
        assert!(got[0].route.is_none() && got[0].headsign.is_none());
    }

    #[test]
    fn a_stop_name_with_punctuation_survives() {
        // Names carry commas, slashes and accents: "VANTAGE / AD. 303",
        // "Aéroport". Tab-separated so none of them need escaping.
        let odd = Pin {
            stop_id: "1".into(),
            code: "0806".into(),
            name: "TUNNEY'S PASTURE, BAY 4 / AD. 303 ~ Aéroport".into(),
            route: None,
            headsign: None,
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
