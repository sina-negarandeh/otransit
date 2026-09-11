//! What a fixture directory holds, and what a missing file in it means.

use super::script::Setup;
use crate::app::RtState;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Everything the directory supplies, before a key is pressed.
///
/// Its own step because reading a fixture and replaying one are separate jobs:
/// this one is about what each file means and what a missing one means, and
/// says so once rather than inside the loop that presses keys.
pub(super) struct World {
    pub(super) conn: rusqlite::Connection,
    pub(super) rt: RtState,
    pub(super) feeds: crate::feeds::Feeds,
    pub(super) pins: ScratchPins,
}

impl World {
    pub(super) fn open(dir: &Path, setup: &Setup) -> Result<Self> {
        let cache = dir.join(&setup.cache);
        if !cache.exists() {
            bail!("{} has no {}", dir.display(), setup.cache.display());
        }
        let conn = rusqlite::Connection::open(&cache)
            .with_context(|| format!("opening {}", cache.display()))?;

        // A file that is absent means that feed had nothing to say, which is a
        // state the app must handle anyway. A file that is present and does not
        // parse is an authoring mistake, and a fixture must say so: the app is
        // right to be quiet about a feed it could not read, but the whole claim
        // of this directory is that it decides the output.
        let text = |name: &str| std::fs::read_to_string(dir.join(name)).ok();
        let alerts = match text("updates.xml") {
            Some(s) => {
                let found = crate::alerts::parse(&s);
                if found.len() == 0 {
                    bail!("updates.xml parsed to no detours; omit the file to mean none");
                }
                Some(found)
            }
            None => None,
        };
        let weather = match text("weather.json") {
            Some(s) => Some(crate::weather::parse(&s).context("weather.json did not parse")?),
            None => None,
        };
        // A fixture that queues answers owns its realtime through the queue,
        // and its first answer lands before the first frame is drawn. Loading
        // `rt.json` as well would give the opening board two sources, with the
        // queue silently winning -- and a fixture whose file and first answer
        // differed would read as though the file applied.
        let rt = if setup.feed.is_empty() {
            match std::fs::read(dir.join("rt.json")) {
                Ok(bytes) => RtState::Ready(crate::rt::parse(&bytes).context("parsing rt.json")?),
                Err(_) => RtState::Off,
            }
        } else {
            RtState::Loading
        };

        Ok(World {
            conn,
            rt,
            feeds: crate::feeds::Feeds::ready(alerts, weather),
            pins: ScratchPins::copy(dir)?,
        })
    }
}

/// The fixture's pins, on a copy this run owns and removes when it ends.
///
/// Never the fixture's own file. `p` is a key this app has, and a session that
/// pressed it would rewrite the directory it is replaying -- so the same script
/// would stop producing the same frames, and the checked-in fixture would be
/// quietly edited.
///
/// One directory per call rather than per process, because the suite replays
/// this fixture from several tests at once: a shared copy would have one test
/// writing the pins another is reading, and would have the first to finish
/// delete them underneath the rest.
pub(super) struct ScratchPins(Option<PathBuf>);

impl ScratchPins {
    pub(super) fn copy(dir: &Path) -> Result<Self> {
        static NTH: AtomicUsize = AtomicUsize::new(0);

        let from = dir.join("pins");
        if !from.exists() {
            return Ok(ScratchPins(None));
        }
        let scratch = std::env::temp_dir().join(format!(
            "otransit-replay-{}-{}",
            std::process::id(),
            NTH.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&scratch)?;
        let to = scratch.join("pins");
        std::fs::copy(&from, &to).with_context(|| format!("copying {}", from.display()))?;
        Ok(ScratchPins(Some(to)))
    }

    pub(super) fn path(&self) -> Option<PathBuf> {
        self.0.clone()
    }
}

impl Drop for ScratchPins {
    fn drop(&mut self) {
        if let Some(file) = self.0.take()
            && let Some(scratch) = file.parent()
        {
            // Nothing useful to do if it fails: the replay is over, and the
            // frames it produced are not less true for a file left in /tmp.
            let _ = std::fs::remove_dir_all(scratch);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drilldown() -> PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("conformance/drilldown")
    }

    #[test]
    fn the_pins_copy_is_this_run_s_own_and_goes_when_it_does() {
        // Never the fixture's file: `p` would rewrite the directory being
        // replayed. And never left behind: a copy per invocation that outlived
        // the invocation would fill the temp directory with them.
        let dir = drilldown();
        let pins = ScratchPins::copy(&dir).unwrap();
        let path = pins.path().expect("the fixture has pins");
        assert!(path.exists());
        assert_ne!(
            path,
            dir.join("pins"),
            "the replay would edit its own input"
        );
        drop(pins);
        assert!(!path.exists(), "a replay left its scratch pins behind");
    }

    #[test]
    fn two_replays_at_once_do_not_share_one_pins_file() {
        // The suite replays this fixture from several tests at the same time.
        // Keyed on the process alone, one test wrote the file another was
        // reading, and the first to finish deleted it under the rest.
        let dir = drilldown();
        let (a, b) = (
            ScratchPins::copy(&dir).unwrap(),
            ScratchPins::copy(&dir).unwrap(),
        );
        assert_ne!(a.path(), b.path());
        drop(a);
        assert!(
            b.path().is_some_and(|p| p.exists()),
            "one replay's cleanup took another's pins"
        );
    }
}
