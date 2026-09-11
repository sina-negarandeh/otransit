//! The realtime feed, on a script's clock.

use super::script::{Outcome, Setup};
use crate::app::App;
use anyhow::Result;
use std::path::{Path, PathBuf};

/// The realtime feed, on a script's clock.
///
/// Holds the app's own polling policy and the answers the script queued. When
/// the policy says an attempt is due, one answer is taken and applied through
/// the same `poll::apply` the real thread uses.
///
/// The asymmetry is the point. Answers are inputs, named by the script. The
/// moments are observations, decided by the policy. A harness that scheduled
/// the requests itself would be testing its own timetable.
pub(super) struct Wire {
    schedule: crate::app::Poll,
    /// Taken from the front as attempts happen. Running out does not stop the
    /// reporting: an attempt that is due with nothing to answer it is a feed
    /// that went quiet, and `due_in` sitting at zero is how that reads.
    pending: std::collections::VecDeque<Outcome>,
    dir: PathBuf,
    /// Whether this fixture drives the feed at all. A fixture that queued
    /// nothing has no cadence to compare, and reporting zeros would read as
    /// "asked nothing, failed nothing" rather than "nobody was asking".
    scripted: bool,
}

impl Wire {
    pub(super) fn new(dir: &Path, setup: &Setup, now: i64) -> Self {
        Wire {
            schedule: crate::app::Poll::new(now),
            pending: setup.feed.iter().cloned().collect(),
            dir: dir.to_path_buf(),
            scripted: !setup.feed.is_empty(),
        }
    }

    /// Answer every attempt the app has become due for.
    ///
    /// A loop, not a single answer. The clock jumps here -- a `wait` covers
    /// intervals a running app would have polled in -- and each attempt is
    /// recorded at the moment it came due rather than at the moment the jump
    /// was noticed. Serving one per step made a hundred-second wait look like
    /// one attempt on a twenty-five second cadence.
    pub(super) fn serve(&mut self, app: &App) -> Result<()> {
        while self.schedule.due(app.epoch()) {
            // Nothing left to answer with. The attempt stays owed, which is
            // what the frames should show: `due_in` holds at zero and the
            // request count stops climbing. An unscripted fixture reaches this
            // on its first pass and never gets further.
            let Some(next) = self.pending.pop_front() else {
                break;
            };
            // The moment the attempt came due, not the moment the clock jumped
            // past it: a wait covering several intervals is several attempts,
            // each where the app would have made it.
            let at = self.schedule.due_at();
            // The outer `?` is this fixture being unreadable. The inner result
            // is what the feed said, which the app is meant to cope with.
            let answer = next.attempt(&self.dir)?;
            let succeeded = answer.is_ok();
            crate::app::apply_attempt(&app.rt, answer);
            self.schedule.record(succeeded, at);
        }
        Ok(())
    }

    /// What the app did, for the snapshot: how many times it asked, how many of
    /// those failed, and how long until it asks again.
    pub(super) fn observed(&self) -> Option<crate::app::Poll> {
        self.scripted.then_some(self.schedule)
    }
}
