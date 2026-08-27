//! Stateless drill-down: Mode -> Route -> Direction -> Stop -> Departures.
//!
//! Every launch starts at the top, and pinned stops are the one exception:
//! a short list of stops you check often, sitting above the modes so the
//! cursor lands on the answer. They are the only thing here that outlives the
//! process, and they live in their own file rather than in the cache, which
//! `update` replaces wholesale.

use crate::db::{self, Departure, Direction, ServiceDay, StopRow};
use crate::rt;
use anyhow::Result;
use chrono::{Local, NaiveDate};
use ratatui::widgets::ListState;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

mod clock;
mod model;
mod poll;

use clock::now_secs;
pub use clock::{WAIT_W, epoch_to_service_secs, fmt_hm, fmt_wait, lateness, mins_until};
pub use model::{Board, Crumb, Mode, Pinned, Row, Screen};
pub use poll::RtState;

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

    /// Seconds into the service day. Written only by `tick`, so it is private
    /// and read through `now()`.
    now: i32,
    pub service_date: NaiveDate,
    pub quit: bool,
    n_bus: usize,
    n_rail: usize,

    /// Stops pinned to the first screen, in the order they were pinned.
    pins: crate::pins::Pins,
}

impl App {
    pub fn new(conn: Connection, cache_dir: std::path::PathBuf) -> Result<Self> {
        let pins = crate::pins::path();
        // Kick the realtime fetch off now — by the time anyone reaches the
        // departures board several keystrokes later it is usually done — and
        // then keep it fresh, so "live" keeps meaning live while you watch.
        let today = Local::now().date_naive();
        Self::build(conn, poll::start(cache_dir), today, now_secs(today), pins)
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
        Self::build(conn, Arc::new(Mutex::new(RtState::Off)), today, now, pins)
    }

    fn build(
        conn: Connection,
        rt_state: Arc<Mutex<RtState>>,
        today_date: NaiveDate,
        now: i32,
        pins_path: Option<std::path::PathBuf>,
    ) -> Result<Self> {
        let yday_date = today_date.pred_opt().unwrap_or(today_date);
        let today = db::active_services(&conn, today_date)?;
        let yesterday = db::active_services(&conn, yday_date)?;
        let count = |m: Mode| db::routes_for_type(&conn, m.route_type(), &today).map(|r| r.len());
        let n_bus = count(Mode::Bus)?;
        let n_rail = count(Mode::Train)?;
        let pins = crate::pins::Pins::open(pins_path, &conn, crate::ui::MAX_PINS)?;

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
            now,
            service_date: today_date,
            quit: false,
            n_bus,
            n_rail,
            pins,
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
                // than on the first question. Pins whose stop is no longer in
                // the cache are left out: a row that cannot be opened is worse
                // than no row, and the stored line survives to come back with
                // the stop.
                let mut rows = Vec::new();
                for stop in self.pins.live().to_vec() {
                    // One row each, so the answer is on screen before anything
                    // is pressed. Six of these measure ~12ms against the real
                    // cache, which is what makes showing them affordable at all.
                    let upcoming = db::departures(
                        &self.conn,
                        &stop.stop_id,
                        db::Narrow::Everything,
                        &self.service_day(),
                        crate::ui::PIN_FETCH,
                    )?;
                    rows.push(Row::Pin(Pinned { stop, upcoming }));
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
            Screen::Stops { route, dir, .. } => {
                db::stops_for_direction(&self.conn, &route.route_ids, &self.today, &dir.headsign)?
                    .into_iter()
                    .map(Row::Stop)
                    .collect()
            }
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
        self.now
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
            now: self.now,
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
            // A pin is the same destination as a search result, reached
            // without the search.
            Row::Pin(p) => self.goto(Screen::Departures(Board::Stop { stop: p.stop }))?,
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
                self.goto(Screen::stops(mode, route, dir))?;
            }
            Row::Stop(stop) => {
                let Screen::Stops {
                    mode, route, dir, ..
                } = self.screen.clone()
                else {
                    return Ok(());
                };
                self.goto(Screen::Departures(Board::Route {
                    mode,
                    route,
                    dir,
                    stop,
                }))?;
            }
        }
        Ok(())
    }

    /// Whether the board on screen is pinned. `None` when this is not a board.
    pub fn board_pin(&self) -> Option<crate::pins::PinState> {
        Some(self.pins.state(&self.screen.board()?.stop().stop_id))
    }

    /// Pin the stop on screen, or unpin it if it is already pinned.
    ///
    /// Only from a board, because a board is the only screen where a letter is
    /// free -- everywhere else typing narrows a list -- and because it is the
    /// screen that just showed you whether the stop is worth keeping.
    ///
    /// A drilled-down board is filtered to one route, but what gets pinned is
    /// the stop: it is the durable half, and "everything calling here" is the
    /// better answer to whether to leave now. The status bar names what is
    /// pinned so that is visible at the moment of pressing.
    pub fn toggle_pin(&mut self) {
        // Taken once, so there is no invariant to assert between two lookups.
        let Some(board) = self.screen.board() else {
            return;
        };
        let stop = board.stop().clone();
        self.pins.toggle(&stop);
    }

    /// Re-read the wall clock, so a board left open keeps counting down.
    ///
    /// The event loop calls this once a frame, and it is the only place the
    /// real clock enters the app. Tests build an `App` with a pinned time and
    /// never call it, which is what keeps their schedule assertions fixed.
    pub fn tick(&mut self) {
        self.now = now_secs(self.service_date);
    }

    /// Move to a screen: load what it shows, put the cursor back at the top,
    /// and fill in any live predictions we already have.
    fn goto(&mut self, screen: Screen) -> Result<()> {
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
            Contents::Board(deps) => deps.first().is_some_and(|d| d.when() < self.now),
            Contents::List(rows) => rows.iter().any(|r| match r {
                Row::Pin(p) => p.next().is_some_and(|d| d.when() < self.now),
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
                mode, route, dir, ..
            }) => Screen::stops(*mode, route.clone(), dir.clone()),
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
        let toward = |d: &Direction| Crumb::Plain(format!("toward {}", d.headsign));
        let stop = |s: &StopRow| Crumb::Plain(tidy_stop_name(&s.name));
        match &self.screen {
            Screen::Mode | Screen::Search { .. } => vec![],
            Screen::Routes { mode, .. } => vec![Crumb::Plain(mode.label().into())],
            Screen::Directions { mode, route, .. } => {
                vec![Crumb::Plain(mode.label().into()), Crumb::route(route)]
            }
            Screen::Stops {
                mode, route, dir, ..
            } => vec![
                Crumb::Plain(mode.label().into()),
                Crumb::route(route),
                toward(dir),
            ],
            Screen::Departures(Board::Route {
                mode,
                route,
                dir,
                stop: s,
            }) => vec![
                Crumb::Plain(mode.label().into()),
                Crumb::route(route),
                toward(dir),
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
        let date = self.service_date;
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
                        .and_then(|e| epoch_to_service_secs(e, date));
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
                    for d in &mut p.upcoming {
                        d.canceled = rt.is_canceled(&d.trip_id);
                        d.live = rt
                            .arrival(&d.trip_id, &p.stop.stop_id)
                            .and_then(|e| epoch_to_service_secs(e, date));
                    }
                    // Same reason as the board: without this the pin shows the
                    // scheduled-earliest bus, not the one that arrives first.
                    db::sort_by_actual_arrival(&mut p.upcoming);
                }
            }
        }
    }

    /// Block until the background fetch settles, or `secs` elapse.
    /// Only used by the headless tools; the TUI never waits.
    pub fn block_on_realtime(&self, secs: u64) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(secs);
        loop {
            if let Ok(g) = self.rt.lock()
                && !matches!(*g, RtState::Loading)
            {
                return;
            }
            if std::time::Instant::now() >= deadline {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
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
                let age = r.age(rt::now_epoch());
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
