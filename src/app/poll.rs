//! The background realtime fetch: one thread, a shared slot, and a backoff.

use crate::rt::{self, Realtime};
use std::sync::{Arc, Mutex};

/// Where the realtime fetch has got to. Filled in by a background thread so
/// the drill-down never blocks on the network.
pub enum RtState {
    /// No subscription key on this machine.
    Off,
    Loading,
    Ready(Realtime),
    Failed(String),
}

/// Start the background fetch, or report that there is no key to fetch with.
///
/// The thread owns everything it touches, which is what `'static` on `spawn`
/// requires, so this is where those values are handed over.
pub(super) fn start(dir: std::path::PathBuf) -> Arc<Mutex<RtState>> {
    let Some(key) = rt::find_key() else {
        return Arc::new(Mutex::new(RtState::Off));
    };
    let slot = Arc::new(Mutex::new(RtState::Loading));
    let thread_slot = Arc::clone(&slot);
    std::thread::spawn(move || poll(&thread_slot, &dir, &key));
    slot
}

/// Refresh the realtime feed forever, on the agency's suggested cadence.
///
/// On failure the previous board is kept: stale predictions carrying an honest
/// age beat no predictions at all. Repeated failures back off so a feed outage
/// doesn't turn into a request storm.
fn poll(slot: &Mutex<RtState>, dir: &std::path::Path, key: &str) {
    let mut backoff = 0u64;
    // The first pass may reuse a warm on-disk copy; after that the loop owns
    // the cadence and always goes to the network.
    let mut first = true;
    loop {
        let attempt = if first {
            rt::load(dir, key)
        } else {
            rt::refresh(dir, key)
        };
        first = false;
        // Read from the attempt, not from the state: a failure that correctly
        // preserved the previous board still leaves the state Ready.
        let succeeded = attempt.is_ok();
        match attempt {
            Ok(fresh) => with_state(slot, |s| *s = RtState::Ready(fresh)),
            Err(e) => with_state(slot, |s| {
                if replaces_on_failure(s) {
                    *s = RtState::Failed(e.to_string());
                }
            }),
        }
        let wait;
        (wait, backoff) = next_poll(succeeded, backoff);
        std::thread::sleep(std::time::Duration::from_secs(wait));
    }
}

/// Take the lock, change the state, release it — all inside this call.
///
/// Written as a helper so the guard never shares a scope with anything else
/// that has a destructor. `if let Ok(g) = m.lock()` holds the guard for the
/// whole `if let`, including its `else`, which is a well-known way to hold a
/// lock far longer than intended.
///
/// A poisoned lock is ignored: the only writer is this loop, and losing one
/// refresh is better than killing the polling thread.
fn with_state(slot: &Mutex<RtState>, f: impl FnOnce(&mut RtState)) {
    if let Ok(mut guard) = slot.lock() {
        f(&mut guard);
    }
}

/// A first failure waits a minute; the outage has to be brief for that to be
/// wasted work. Doubling from there caps at five minutes, which keeps a long
/// outage to roughly a dozen requests an hour.
const MIN_BACKOFF_SECS: u64 = 60;
const MAX_BACKOFF_SECS: u64 = 300;

/// How long to wait before the next realtime fetch, and the backoff to carry
/// into the attempt after that.
///
/// Pure: the loop supplies the outcome, this decides the cadence.
pub fn next_poll(succeeded: bool, backoff: u64) -> (u64, u64) {
    if succeeded {
        (rt::TTL_SECS as u64, 0)
    } else {
        let next = (backoff * 2).clamp(MIN_BACKOFF_SECS, MAX_BACKOFF_SECS);
        (next, next)
    }
}

/// Whether a failed refresh should overwrite what's on screen.
///
/// It must not when we already have a board: stale predictions carrying an
/// honest age beat no predictions at all, and the age is already displayed.
pub fn replaces_on_failure(current: &RtState) -> bool {
    !matches!(current, RtState::Ready(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------- poll cadence ----------

    #[test]
    fn a_successful_fetch_polls_again_on_the_normal_cadence() {
        let (wait, backoff) = next_poll(true, 0);
        assert_eq!(wait, rt::TTL_SECS as u64);
        assert_eq!(backoff, 0);
    }

    #[test]
    fn success_clears_any_accumulated_backoff() {
        let (wait, backoff) = next_poll(true, MAX_BACKOFF_SECS);
        assert_eq!(wait, rt::TTL_SECS as u64, "recovery is immediate");
        assert_eq!(backoff, 0);
    }

    #[test]
    fn repeated_failures_back_off_and_then_hold_at_the_cap() {
        // A feed outage must not become a request storm.
        let mut backoff = 0;
        let mut waits = vec![];
        for _ in 0..6 {
            let w;
            (w, backoff) = next_poll(false, backoff);
            waits.push(w);
        }
        assert_eq!(waits, vec![60, 120, 240, 300, 300, 300]);
    }

    #[test]
    fn the_first_failure_never_retries_faster_than_the_normal_cadence() {
        let (wait, _) = next_poll(false, 0);
        assert!(
            wait >= rt::TTL_SECS as u64,
            "backing off must slow down, not speed up"
        );
    }

    // ---------- keeping a good board through a failure ----------

    #[test]
    fn a_failure_does_not_wipe_a_board_we_already_have() {
        // Stale predictions with an honest age beat none; the age is on screen.
        assert!(!replaces_on_failure(&RtState::Ready(Realtime::default())));
    }

    #[test]
    fn a_failure_before_any_board_is_reported_to_the_user() {
        assert!(replaces_on_failure(&RtState::Loading));
        assert!(replaces_on_failure(&RtState::Off));
        assert!(replaces_on_failure(&RtState::Failed("boom".into())));
    }
}
