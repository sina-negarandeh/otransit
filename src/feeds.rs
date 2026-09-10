//! The two feeds fetched once at launch: published detours, and the weather.
//!
//! Both are the same arrangement — a thread, a fetch that may fail, and a slot
//! the browser reads once a frame without ever waiting on it. Held together
//! rather than as two pairs of fields on `App`, for the reason `Pins` is:
//! `App`'s subject is the drill-down, and neither of these is about it.
//!
//! Realtime is deliberately not here. It refreshes on a cadence, backs off on
//! failure and has states these do not, so it keeps `RtState` and its own
//! thread. These two are fetched once and never again: a detour lasts days,
//! conditions change by the hour, and a browser session lasts a minute.

use std::sync::{Arc, Mutex};

/// A background fetch's slot: whether it has finished, and what it produced.
///
/// `Pending` is the state anything waiting must be able to see. Separating it
/// from "finished with nothing" is what stops a caller sitting out a whole
/// timeout over a week with no detours in it, or over a weather fetch that
/// failed in the first second.
#[derive(Default)]
enum Fetched<T> {
    #[default]
    Pending,
    Done(Option<T>),
}

impl<T> Fetched<T> {
    fn get(&self) -> Option<&T> {
        match self {
            Fetched::Done(found) => found.as_ref(),
            Fetched::Pending => None,
        }
    }

    fn settled(&self) -> bool {
        matches!(self, Fetched::Done(_))
    }
}

/// The slots, and the threads filling them.
pub struct Feeds {
    alerts: Arc<Mutex<Fetched<crate::alerts::Alerts>>>,
    weather: Arc<Mutex<Fetched<crate::weather::Weather>>>,
}

impl Feeds {
    /// Start both fetches and return immediately. The browser draws its first
    /// screen without either of them.
    pub fn start() -> Self {
        Self {
            alerts: spawn(|| crate::alerts::fetch().ok()),
            weather: spawn(|| crate::weather::fetch().ok()),
        }
    }

    /// Both settled with nothing, and no thread.
    ///
    /// For tests, and for the same reason `App::offline` exists: a test that
    /// reached these would be reading whatever was published this morning.
    /// Settled rather than pending, because nothing is coming — a caller that
    /// waited would otherwise wait out the whole timeout.
    #[cfg(test)]
    pub fn none() -> Self {
        Self {
            alerts: Arc::new(Mutex::new(Fetched::Done(None))),
            weather: Arc::new(Mutex::new(Fetched::Done(None))),
        }
    }

    /// What is published about this route, if anything.
    ///
    /// The lock is held only long enough to copy the headline: this is asked
    /// once a frame while a background thread may be filling the slot.
    pub fn alert(&self, short_name: &str) -> Option<String> {
        Some(
            self.alerts
                .lock()
                .ok()?
                .get()?
                .for_route(short_name)?
                .title
                .clone(),
        )
    }

    /// How many alerts landed, for `probe`. A feed that quietly stops naming
    /// routes reads as "no detours anywhere", which is worth being able to see.
    pub fn alert_count(&self) -> usize {
        self.alerts
            .lock()
            .map(|g| g.get().map_or(0, crate::alerts::Alerts::len))
            .unwrap_or(0)
    }

    /// The one line of weather, if it has landed.
    pub fn weather(&self) -> Option<String> {
        Some(self.weather.lock().ok()?.get()?.label())
    }

    /// Whether the detours have finished arriving, however few there were.
    pub fn alerts_settled(&self) -> bool {
        self.alerts.lock().is_ok_and(|g| g.settled())
    }

    /// Whether both have finished.
    pub fn settled(&self) -> bool {
        self.alerts_settled() && self.weather.lock().is_ok_and(|g| g.settled())
    }

    /// Put alerts on screen without a network call. `Feeds::none` never
    /// fetches, so this is the only way a test reaches the code that draws
    /// them.
    #[cfg(test)]
    pub fn set_alerts(&self, alerts: crate::alerts::Alerts) {
        *self
            .alerts
            .lock()
            .expect("no other thread holds this in a test") = Fetched::Done(Some(alerts));
    }

    /// The same, for the rule the weather sits on.
    #[cfg(test)]
    pub fn set_weather(&self, w: crate::weather::Weather) {
        *self
            .weather
            .lock()
            .expect("no other thread holds this in a test") = Fetched::Done(Some(w));
    }
}

/// Start a background fetch and hand back the slot it will fill.
///
/// The closure says what failure means for that feed, which is the only thing
/// that differs between the two. The slot settles either way, so that anything
/// waiting knows it is over.
fn spawn<T: Send + 'static>(
    produce: impl FnOnce() -> Option<T> + Send + 'static,
) -> Arc<Mutex<Fetched<T>>> {
    let slot: Arc<Mutex<Fetched<T>>> = Arc::default();
    let fill = Arc::clone(&slot);
    std::thread::spawn(move || {
        let found = produce();
        if let Ok(mut g) = fill.lock() {
            *g = Fetched::Done(found);
        }
    });
    slot
}
