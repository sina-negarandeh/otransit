//! Stateless drill-down: Mode -> Route -> Direction -> Stop -> Departures.
//!
//! Every launch starts at the top, and pinned stops are the one exception:
//! a short list of stops you check often, sitting above the modes so the
//! cursor lands on the answer. They are the only thing here that outlives the
//! process, and they live in their own file rather than in the cache, which
//! `update` replaces wholesale.

use crate::db::{self, Departure, ServiceDay, StopRow};
use anyhow::Result;
use chrono::{Local, NaiveDate};
use ratatui::widgets::ListState;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

mod clock;
mod model;
mod poll;

pub use clock::{Clock, WAIT_W, fmt_hm, fmt_wait, lateness, mins_until};
pub use model::{Board, Crumb, Mode, Pinned, Row, Screen};
pub use poll::{Poll, RtState, apply_attempt};

/// What the current screen is showing.
///
/// A list of selectable rows or a departures board, never both — which is why
/// it is one field rather than two that have to be kept from going stale
/// against each other.
pub enum Contents {
    List(Vec<Row>),
    Board(Vec<Departure>),
}

impl Contents {
    fn rows(&self) -> &[Row] {
        match self {
            Contents::List(rows) => rows,
            Contents::Board(_) => &[],
        }
    }

    pub fn board(&self) -> Option<&[Departure]> {
        match self {
            Contents::Board(deps) => Some(deps),
            Contents::List(_) => None,
        }
    }
}

/// Everything on screen: where you are, what that screen lists, and the shared
/// slot the realtime thread fills in.
pub struct App {
    /// The cache. Private: everything outside `App` asks a question through a
    /// method rather than running its own SQL against the schedule.
    conn: Connection,
    pub rt: Arc<Mutex<RtState>>,
    today: Vec<String>,
    yesterday: Vec<String>,
    pub screen: Screen,
    pub state: ListState,

    /// What this screen shows, before any typed text narrows it. Loaded in one
    /// place (`load`) so entering and going back land on the same contents.
    pub contents: Contents,

    /// What time it is, which service day that falls in, and the zone both are
    /// read in. Its own type because those move together — see `clock::Clock`.
    /// Carried rather than read at the point of use, so a replay's frames do
    /// not change as the day goes on.
    clock: Clock,
    pub quit: bool,
    n_bus: usize,
    n_rail: usize,

    /// Boards pinned to the first screen, in the order they were pinned.
    pins: crate::pins::Pins,

    /// Where `back` goes, when the shape of the screen cannot say.
    ///
    /// The drill-down keeps no history because every screen's parent is
    /// derivable: a stop list belongs to a direction, a direction to a route.
    /// A pin is the one jump in the app — it lands you under a route you never
    /// walked to — so it is the one arrival that has to be remembered. Any
    /// other move through `goto` clears it, because every other move is one
    /// step and its parent is where it came from.
    returning_to: Option<Screen>,
    /// The two feeds fetched once at launch. Their own module, because
    /// neither is about the drill-down this one is named for.
    feeds: crate::feeds::Feeds,
}

impl App {
    pub fn new(conn: Connection, cache_dir: std::path::PathBuf) -> Result<Self> {
        let pins = crate::pins::path();
        // Kick the realtime fetch off now — by the time anyone reaches the
        // departures board several keystrokes later it is usually done — and
        // then keep it fresh, so "live" keeps meaning live while you watch.
        Self::build(
            conn,
            poll::start(cache_dir),
            Clock::here(Local::now().date_naive()),
            pins,
            crate::feeds::Feeds::start(),
        )
    }

    /// An app with every input supplied: no thread, no clock, no network.
    ///
    /// What `replay` is built on. The only difference between a recorded
    /// session and a live one should be where the inputs came from, so both
    /// arrive at the same `build`.
    pub fn fixed(
        conn: Connection,
        clock: Clock,
        pins: Option<std::path::PathBuf>,
        rt: RtState,
        feeds: crate::feeds::Feeds,
    ) -> Result<Self> {
        Self::build(conn, Arc::new(Mutex::new(rt)), clock, pins, feeds)
    }

    /// An app with no realtime thread and a fixed clock.
    ///
    /// Tests must not reach the network — and a `.env` sitting in the working
    /// directory means `new` would find a real key and start polling. Pinning
    /// the date also makes every schedule assertion reproducible.
    #[cfg(test)]
    pub fn offline(
        conn: Connection,
        today: NaiveDate,
        now: i32,
        pins: Option<std::path::PathBuf>,
    ) -> Result<Self> {
        // No fetch: a test that reached the updates feed would be reading
        // whatever OC Transpo published this morning. Settled rather than
        // pending, because nothing is coming -- a caller that waited on this
        // would otherwise wait for the whole timeout.
        Self::fixed(
            conn,
            // UTC, not the machine's zone. A test that read the machine would be
            // asserting something about where the suite runs, which is a defect
            // this codebase has already had once.
            Clock::held(
                today,
                now,
                chrono::FixedOffset::east_opt(0).expect("UTC is a real offset"),
            ),
            pins,
            RtState::Off,
            crate::feeds::Feeds::none(),
        )
    }

    fn build(
        conn: Connection,
        rt_state: Arc<Mutex<RtState>>,
        clock: Clock,
        pins_path: Option<std::path::PathBuf>,
        feeds: crate::feeds::Feeds,
    ) -> Result<Self> {
        let today_date = clock.date();
        let yday_date = today_date.pred_opt().unwrap_or(today_date);
        let today = db::active_services(&conn, today_date)?;
        let yesterday = db::active_services(&conn, yday_date)?;
        let count = |m: Mode| db::routes_for_type(&conn, m.route_type(), &today).map(|r| r.len());
        let n_bus = count(Mode::Bus)?;
        let n_rail = count(Mode::Train)?;
        let pins = crate::pins::Pins::open(pins_path, &conn, crate::ui::MAX_PINS, &today)?;

        let mut state = ListState::default();
        state.select(Some(0));
        let mut app = App {
            conn,
            rt: rt_state,
            today,
            yesterday,
            screen: Screen::Mode,
            state,
            contents: Contents::List(vec![]),
            clock,
            quit: false,
            n_bus,
            n_rail,
            pins,
            returning_to: None,
            feeds,
        };
        app.contents = app.load(&Screen::Mode)?;
        Ok(app)
    }

    /// What a screen shows. The single place any screen's contents come from,
    /// so arriving forwards, arriving backwards and refreshing in place cannot
    /// disagree about what belongs there.
    fn load(&self, screen: &Screen) -> Result<Contents> {
        let rows: Vec<Row> = match screen {
            Screen::Mode => {
                // Above the modes, so the cursor lands on the answer rather
                // than on the first question. Pins the cache can no longer
                // resolve are left out: a row that cannot be opened is worse
                // than no row, and the stored line survives to come back with
                // the stop or the route.
                let mut rows = Vec::new();
                for board in self.pins.live().to_vec() {
                    // One row each, so the answer is on screen before anything
                    // is pressed. Six of these measure ~12ms against the real
                    // cache, which is what makes showing them affordable at all.
                    //
                    // The board says which departures are its own. A pin made
                    // by drilling shows that route in that direction, because
                    // two routes to one terminus are not two routes to one
                    // place; a pin made from a search shows everything calling
                    // here, because you never said where you were going.
                    let upcoming = db::departures(
                        &self.conn,
                        &board.stop().stop_id,
                        board.narrow(),
                        &self.service_day(),
                        crate::ui::PIN_FETCH,
                    )?;
                    rows.push(Row::Pin(Pinned { board, upcoming }));
                }
                rows.extend([
                    Row::Mode(Mode::Bus, self.n_bus),
                    Row::Mode(Mode::Train, self.n_rail),
                ]);
                rows
            }
            Screen::Search { query } => {
                db::search_stops(&self.conn, query, &self.today, crate::ui::SEARCH_LIMIT)?
                    .into_iter()
                    .map(Row::Hit)
                    .collect()
            }
            Screen::Routes { mode, .. } => {
                db::routes_for_type(&self.conn, mode.route_type(), &self.today)?
                    .into_iter()
                    .map(Row::Route)
                    .collect()
            }
            Screen::Directions { route, .. } => {
                db::directions_for_route(&self.conn, &route.route_ids, &self.today)?
                    .into_iter()
                    .map(Row::Direction)
                    .collect()
            }
            Screen::Stops {
                route, headsign, ..
            } => db::stops_for_direction(&self.conn, &route.route_ids, &self.today, headsign)?
                .into_iter()
                .map(Row::Stop)
                .collect(),
            Screen::Departures(board) => {
                return Ok(Contents::Board(db::departures(
                    &self.conn,
                    &board.stop().stop_id,
                    board.narrow(),
                    &self.service_day(),
                    crate::ui::BOARD_FETCH,
                )?));
            }
        };
        Ok(Contents::List(rows))
    }

    /// Rows visible right now: what the screen holds, narrowed by the filter.
    ///
    /// Each row carries the value it stands for, so `enter` acts on the payload
    /// directly and the renderer asks the row how to draw itself. Neither has
    /// to know which screen produced it.
    pub fn rows(&self) -> Vec<Row> {
        let needle = self.screen.typed().to_lowercase();
        if needle.is_empty() {
            return self.contents.rows().to_vec();
        }
        self.contents
            .rows()
            .iter()
            .filter(|r| r.matches(&needle))
            .cloned()
            .collect()
    }

    /// The row under the cursor, if there is one.
    fn selected_row(&self) -> Option<Row> {
        self.rows().into_iter().nth(self.state.selected()?)
    }

    pub fn move_by(&mut self, delta: isize) {
        let n = self.rows().len();
        if n == 0 {
            return;
        }
        let cur = self.state.selected().unwrap_or(0) as isize;
        let next = (cur + delta).rem_euclid(n as isize);
        self.state.select(Some(next as usize));
    }

    /// Whether any service runs today at all. An empty schedule means the cache
    /// needs rebuilding, which is worth saying before the browser opens.
    pub fn has_service_today(&self) -> bool {
        !self.today.is_empty()
    }

    /// Seconds into the service day, as of the last `tick`.
    pub fn now(&self) -> i32 {
        self.clock.now()
    }

    /// Which service day the app is showing.
    pub fn service_date(&self) -> NaiveDate {
        self.clock.date()
    }

    /// Move a held clock on, for a test that needs a bus to have gone or a
    /// script that says time passed. The real clock never enters either: this
    /// is `tick` without the machine, and it does nothing to a live clock.
    pub fn advance_to(&mut self, secs: i32) {
        self.clock.advance_to(secs);
    }

    /// The epoch this app's clock is currently reading.
    pub fn epoch(&self) -> i64 {
        self.clock.epoch()
    }

    /// The instant at `secs` into the service day this app is showing.
    ///
    /// For a test that has to state a realtime prediction. Asked of the app
    /// rather than built from `Local`, so the prediction is stated in the same
    /// zone it will be read back in.
    #[cfg(test)]
    pub fn epoch_of(&self, secs: i32) -> i64 {
        self.clock.epoch_of(secs)
    }

    /// The GTFS route_type of a trip, or None if the cache has never heard of
    /// it. `probe` asks this to tell a stale cache from a moved feed.
    pub fn route_type_of(&self, trip_id: &str) -> Result<Option<i64>> {
        use rusqlite::OptionalExtension;
        Ok(self
            .conn
            .query_row(
                "SELECT r.route_type FROM trips t JOIN routes r ON r.route_id = t.route_id
                  WHERE t.trip_id = ?1",
                [trip_id],
                |r| r.get(0),
            )
            .optional()?)
    }

    /// The etag of the export this cache was built from.
    ///
    /// App owns the open connection on the browse path, so questions about the
    /// cache go through it rather than opening the file a second time on the
    /// path the app exists to keep instant. That is the whole licence: this
    /// answers about the cache App holds, not about anything else, and a second
    /// accessor of this shape means the connection should be shared explicitly
    /// instead.
    pub fn cache_etag(&self) -> Option<String> {
        crate::gtfs::stored_etag(&self.conn)
    }

    /// Which service days are live, and what time it is now.
    pub fn service_day(&self) -> ServiceDay<'_> {
        ServiceDay {
            today: &self.today,
            yesterday: &self.yesterday,
            now: self.clock.now(),
        }
    }

    /// Discard what was typed here. On the search screen there is nothing left
    /// once the query goes, so that is a step back rather than a clear.
    pub fn clear_filter(&mut self) -> Result<()> {
        if matches!(self.screen, Screen::Search { .. }) {
            return self.goto(Screen::Mode);
        }
        if let Some(text) = self.screen.typed_mut() {
            text.clear();
            self.state.select(Some(0));
        }
        Ok(())
    }

    pub fn push_filter(&mut self, c: char) -> Result<()> {
        // Typing at the first screen opens the search; a board takes no text at
        // all. Both fall out of the model rather than being tested for.
        let Some(text) = self.screen.typed_mut() else {
            return match self.screen {
                Screen::Mode => self.goto(Screen::Search {
                    query: c.to_string(),
                }),
                _ => Ok(()),
            };
        };
        text.push(c);
        self.retype()
    }

    /// Returns false when there was nothing left to delete.
    pub fn pop_filter(&mut self) -> Result<bool> {
        let Some(text) = self.screen.typed_mut() else {
            return Ok(false);
        };
        if text.pop().is_none() {
            return Ok(false);
        }
        // A search with nothing typed is the screen before it.
        if text.is_empty() && matches!(self.screen, Screen::Search { .. }) {
            self.goto(Screen::Mode)?;
        } else {
            self.retype()?;
        }
        Ok(true)
    }

    /// After the typed text changes: cursor back to the top, and re-run the
    /// query if the text *is* the query rather than a narrowing of one.
    fn retype(&mut self) -> Result<()> {
        self.state.select(Some(0));
        if matches!(self.screen, Screen::Search { .. }) {
            let screen = self.screen.clone();
            self.contents = self.load(&screen)?;
        }
        Ok(())
    }

    /// Jump to the stop search from anywhere ("/").
    pub fn focus_search(&mut self) -> Result<()> {
        self.goto(Screen::Mode)
    }

    fn reset_cursor(&mut self) {
        self.state.select(Some(0));
    }

    /// Advance one level, acting on the payload the selected row carries.
    pub fn enter(&mut self) -> Result<()> {
        let Some(row) = self.selected_row() else {
            return Ok(());
        };
        match row {
            // A search result goes straight to the stop's board, skipping route
            // and direction entirely.
            Row::Hit(hit) => self.goto(Screen::Departures(Board::Stop { stop: hit.into() }))?,
            // A pin is a board you saved, reopened without the walk. Which
            // board depends on how you made it -- a route in one direction, or
            // a whole stop.
            Row::Pin(p) => {
                // The one jump. Recorded after `goto`, which clears it, and
                // taken from the screen rather than assumed to be the first
                // one: a pin is only drawn there today, and a claim about
                // where you were should not depend on that staying true.
                let from = self.screen.clone();
                self.goto(Screen::Departures(p.board))?;
                self.returning_to = Some(from);
            }
            Row::Mode(mode, _) => self.goto(Screen::routes(mode))?,
            Row::Route(route) => {
                let Some(mode) = self.screen.mode() else {
                    return Ok(());
                };
                self.goto(Screen::directions(mode, route))?;
            }
            Row::Direction(dir) => {
                let (Some(mode), Some(route)) = (self.screen.mode(), self.screen.route().cloned())
                else {
                    return Ok(());
                };
                self.goto(Screen::stops(mode, route, dir.headsign))?;
            }
            Row::Stop(stop) => {
                let Screen::Stops {
                    mode,
                    route,
                    headsign,
                    ..
                } = self.screen.clone()
                else {
                    return Ok(());
                };
                self.goto(Screen::Departures(Board::Route {
                    mode,
                    route,
                    headsign,
                    stop,
                }))?;
            }
        }
        Ok(())
    }

    /// What is published about the route on screen, if a route is chosen and
    /// anything is published about it.
    ///
    /// The lock is held only long enough to copy the headline: this is asked
    /// once a frame while a background thread may be replacing the list.
    pub fn route_alert(&self) -> Option<String> {
        // Only the screens under one route. A board is under one too, and shows
        // no detour: by then you have chosen, and the message belongs where the
        // choosing happens. Gated here rather than at the one place that draws
        // it, so anything else asking gets the same answer.
        self.screen
            .below_a_route()
            .then(|| self.feeds.alert(&self.screen.route()?.short_name))
            .flatten()
    }

    /// Wait for the alerts fetch to settle, or `secs` to pass.
    ///
    /// Settled, not non-empty: a failed fetch and a week with no detours both
    /// end with nothing to show, and waiting for a count to rise would sit out
    /// the full timeout on both. Only the headless tools wait at all; the
    /// browser draws without them and picks them up on a later frame.
    pub fn block_on_alerts(&self, secs: u64) {
        block_until(secs, || self.feeds.alerts_settled());
    }

    /// How many alerts landed, for `probe`. A feed that quietly stops naming
    /// routes reads as "no detours anywhere", which is worth being able to see.
    pub fn alert_count(&self) -> usize {
        self.feeds.alert_count()
    }

    /// The one line of weather, if it has landed.
    ///
    /// The lock is held only long enough to build the label: this is asked once
    /// a frame while a background thread may be filling the slot.
    pub fn weather(&self) -> Option<String> {
        self.feeds.weather()
    }

    /// Wait for every background fetch to settle, or `secs` to pass.
    ///
    /// One deadline for all three, because all three are already in flight.
    /// Waiting on them one after another turned three timeouts into their sum:
    /// an offline screenshot spent forty-two seconds before it drew a frame.
    pub fn block_on_feeds(&self, secs: u64) {
        block_until(secs, || {
            self.rt.lock().is_ok_and(|g| g.settled()) && self.feeds.settled()
        });
    }

    /// Put weather on screen without a network call. `App::offline` never
    /// fetches, so this is the only way a test reaches the rule that draws it.
    #[cfg(test)]
    pub(crate) fn set_weather(&mut self, w: crate::weather::Weather) {
        self.feeds.set_weather(w);
    }

    /// Put alerts on screen without a network call.
    ///
    /// `App::offline` never fetches, so this is the only way a test reaches
    /// the code that draws them. It takes the parsed shape rather than the
    /// XML so a caller can build one stop-gap alert without a fixture.
    #[cfg(test)]
    pub(crate) fn set_alerts(&mut self, alerts: crate::alerts::Alerts) {
        self.feeds.set_alerts(alerts);
    }

    /// Whether the board on screen is pinned. `None` when this is not a board.
    pub fn board_pin(&self) -> Option<crate::pins::PinState> {
        Some(self.pins.state(self.screen.board()?))
    }

    /// Pin the board on screen, or unpin it if it is already pinned.
    ///
    /// Only from a board, because a board is the only screen where a letter is
    /// free -- everywhere else typing narrows a list -- and because it is the
    /// screen that just showed you whether this is worth keeping.
    ///
    /// The whole board is pinned, not just its stop. Drill to a route and you
    /// pin that route in that direction; press `p` on a search result and you
    /// pin everything calling there, because you never said where you were
    /// going. "Everything calling here" was once the answer for both, and it is
    /// wrong for the first: 44 and 48 both end at Billings Bridge by roads that
    /// never meet, so a pin that offered either would send you to a bus that
    /// does not serve your stop. The status bar names what is pinned, so that
    /// is visible at the moment of pressing.
    pub fn toggle_pin(&mut self) {
        // Taken once, so there is no invariant to assert between two lookups.
        let Some(board) = self.screen.board().cloned() else {
            return;
        };
        self.pins.toggle(&board);
    }

    /// Re-read the wall clock, so a board left open keeps counting down.
    ///
    /// The event loop calls this once a frame, and it is the only place the
    /// real clock enters the app. Tests build an `App` with a pinned time and
    /// never call it, which is what keeps their schedule assertions fixed.
    pub fn tick(&mut self) {
        self.clock.tick();
    }

    /// Move to a screen: load what it shows, put the cursor back at the top,
    /// and fill in any live predictions we already have.
    fn goto(&mut self, screen: Screen) -> Result<()> {
        // One step, so the screen's own shape says where back goes.
        self.returning_to = None;
        self.contents = self.load(&screen)?;
        self.screen = screen;
        self.reset_cursor();
        self.apply_realtime();
        Ok(())
    }

    /// Re-query the board once a departure on it has gone.
    ///
    /// `tick` moves the clock under a board that was loaded once, so without
    /// this the visible rows fill up with buses that already left while the
    /// ones still to come sit below the fold. Guarded on the front row rather
    /// than run every frame: it costs a query per departure, not four a second.
    pub fn refresh(&mut self) -> Result<()> {
        // Pins carry departures too, so they go stale exactly as a board does.
        // Asking only the board left the first screen filling with buses that
        // had already gone, which is the defect the board itself has an entry
        // for in TESTING.md.
        let gone = match &self.contents {
            Contents::Board(deps) => deps.first().is_some_and(|d| d.when() < self.now()),
            Contents::List(rows) => rows.iter().any(|r| match r {
                Row::Pin(p) => p.next().is_some_and(|d| d.when() < self.now()),
                _ => false,
            }),
        };
        if !gone {
            return Ok(());
        }
        let screen = self.screen.clone();
        self.contents = self.load(&screen)?;
        self.apply_realtime();
        Ok(())
    }

    /// Walk back up a level, rebuilding the screen behind this one from what
    /// this one carries.
    ///
    /// Copies rather than moving the current screen out: `goto` can fail on the
    /// query, and taking the screen apart first would leave the app on `Mode`
    /// with the real one already dropped.
    pub fn back(&mut self) -> Result<()> {
        if let Some(screen) = self.returning_to.take() {
            return self.goto(screen);
        }
        let previous = match &self.screen {
            Screen::Mode => {
                self.quit = true;
                return Ok(());
            }
            // A board reached by search has no route or direction behind it,
            // so it returns to the first screen just as the route list does.
            Screen::Routes { .. }
            | Screen::Search { .. }
            | Screen::Departures(Board::Stop { .. }) => Screen::Mode,
            Screen::Directions { mode, .. } => Screen::routes(*mode),
            Screen::Stops { mode, route, .. } => Screen::directions(*mode, route.clone()),
            Screen::Departures(Board::Route {
                mode,
                route,
                headsign,
                ..
            }) => Screen::stops(*mode, route.clone(), headsign.clone()),
        };
        self.goto(previous)
    }

    /// The question for the current screen. Buses have routes, trains have lines.
    pub fn title(&self) -> &'static str {
        match &self.screen {
            Screen::Search { .. } => "Transit stops",
            Screen::Mode => "What are you taking?",
            Screen::Routes {
                mode: Mode::Train, ..
            } => "Which line?",
            Screen::Routes { .. } => "Which route?",
            Screen::Directions { .. } => "Which way?",
            Screen::Stops {
                mode: Mode::Train, ..
            } => "Which station?",
            Screen::Stops { .. } => "Which stop?",
            Screen::Departures(_) => "Departures",
        }
    }

    /// Everything chosen so far, as one flat trail.
    pub fn crumbs(&self) -> Vec<Crumb> {
        let toward = |h: &str| Crumb::Plain(format!("toward {h}"));
        let stop = |s: &StopRow| Crumb::Plain(tidy_stop_name(&s.name));
        match &self.screen {
            Screen::Mode | Screen::Search { .. } => vec![],
            Screen::Routes { mode, .. } => vec![Crumb::Plain(mode.label().into())],
            Screen::Directions { mode, route, .. } => {
                vec![Crumb::Plain(mode.label().into()), Crumb::route(route)]
            }
            Screen::Stops {
                mode,
                route,
                headsign,
                ..
            } => vec![
                Crumb::Plain(mode.label().into()),
                Crumb::route(route),
                toward(headsign),
            ],
            Screen::Departures(Board::Route {
                mode,
                route,
                headsign,
                stop: s,
            }) => vec![
                Crumb::Plain(mode.label().into()),
                Crumb::route(route),
                toward(headsign),
                stop(s),
            ],
            // Reached by search: the pole number and the stop are the whole trail.
            Screen::Departures(Board::Stop { stop: s }) => {
                vec![Crumb::Plain(format!("#{}", s.code)), stop(s)]
            }
        }
    }

    /// Attach live predictions to the current board. Cheap, so it runs on every
    /// tick — that way the board fills in the moment the fetch lands.
    pub fn apply_realtime(&mut self) {
        // Copied out so the guard does not borrow `self`, which the contents
        // below need mutably. One value rather than the two it used to take.
        let clock = self.clock;
        // Clone the handle so the guard does not borrow `self`, which the
        // contents below need mutably.
        let slot = Arc::clone(&self.rt);
        let Ok(guard) = slot.lock() else { return };
        let RtState::Ready(rt) = &*guard else { return };

        // A pin shows the same number the board would, so it goes live the
        // moment the fetch lands rather than sitting on the timetable while
        // the board beside it is current.
        let board_stop = self.screen.board().map(|b| b.stop().stop_id.clone());
        match &mut self.contents {
            Contents::Board(deps) => {
                let Some(stop_id) = board_stop else { return };
                for d in deps.iter_mut() {
                    d.canceled = rt.is_canceled(&d.trip_id);
                    d.live = rt
                        .arrival(&d.trip_id, &stop_id)
                        .and_then(|e| clock.service_secs(e));
                }
                // Ordered by when a bus actually arrives, not when it was meant
                // to. Without this a late trip sorts ahead of one on time.
                db::sort_by_actual_arrival(deps);
            }
            // Only pins carry departures, and `Row::Pin` says which rows those
            // are. Guarding on the screen as well asked two models the same
            // question, to save a discriminant check per row.
            Contents::List(rows) => {
                for row in rows {
                    let Row::Pin(p) = row else { continue };
                    // Taken apart rather than reached through `Pinned::stop`,
                    // which borrows the whole pin and so collides with the
                    // mutable loop over its departures.
                    let Pinned { board, upcoming } = p;
                    for d in upcoming.iter_mut() {
                        d.canceled = rt.is_canceled(&d.trip_id);
                        d.live = rt
                            .arrival(&d.trip_id, &board.stop().stop_id)
                            .and_then(|e| clock.service_secs(e));
                    }
                    // Same reason as the board: without this the pin shows the
                    // scheduled-earliest bus, not the one that arrives first.
                    db::sort_by_actual_arrival(upcoming);
                }
            }
        }
    }

    /// Block until the background fetch settles, or `secs` elapse.
    /// Only used by the headless tools; the TUI never waits.
    pub fn block_on_realtime(&self, secs: u64) {
        block_until(secs, || self.rt.lock().is_ok_and(|g| g.settled()));
    }

    /// A one-line note about the feed, for the status bar.
    pub fn rt_note(&self) -> Option<String> {
        let guard = self.rt.lock().ok()?;
        match &*guard {
            RtState::Off => Some("no key · scheduled only".into()),
            RtState::Loading => Some("live…".into()),
            RtState::Failed(e) => {
                // First line only; the status bar truncates from the left
                // anyway. Not split on ':' as well — that cut every transport
                // error at the scheme of the URL it was reporting, and threw
                // away the status code in "http status: 401".
                let first = e.lines().next().unwrap_or("unavailable");
                Some(format!("live: {first}"))
            }
            RtState::Ready(r) => {
                let age = r.age(self.clock.epoch());
                Some(if age > 120 {
                    format!("live {}m old", age / 60)
                } else {
                    format!("live {age}s")
                })
            }
        }
    }

    /// Colour marking "you are here".
    ///
    /// O-Train lines have strong, legible colour identity and there are only
    /// three, so the trail takes the line's own colour. Bus route_colors encode
    /// service tier, and 134 of 184 of them are white or the exact grey we use
    /// for dimmed text — those would destroy the emphasis, so buses keep the
    /// brand accent.
    pub fn accent_hex(&self) -> Option<&str> {
        (self.screen.mode() == Some(Mode::Train))
            .then(|| self.screen.route().map(|r| r.color.as_str()))
            .flatten()
    }
}

/// Spin until a background fetch has settled, or `secs` runs out.
///
/// The headless tools want an answer before they print; the browser never
/// waits at all. Both sources are asked the same way because both are the same
/// arrangement — a thread filling a slot — and the only thing that differs is
/// how a slot says it is done.
fn block_until(secs: u64, settled: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
    while !settled() {
        if std::time::Instant::now() >= deadline {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Station names repeat the direction we already picked:
/// "RIDEAU O-TRAIN EAST / EST" -> "RIDEAU". Platform letters are kept.
pub fn tidy_stop_name(name: &str) -> String {
    let n = name
        .split(" O-TRAIN ")
        .next()
        .unwrap_or(name)
        .trim()
        .to_string();
    if n.is_empty() { name.to_string() } else { n }
}

#[cfg(test)]
mod tests;
