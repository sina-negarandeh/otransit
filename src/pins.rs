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

use anyhow::Result;
use std::path::{Path, PathBuf};

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
    text.lines()
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
