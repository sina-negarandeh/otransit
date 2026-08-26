//! Stateless drill-down: Mode -> Route -> Direction -> Stop -> Departures.
//! Nothing is remembered between runs; every launch starts at the top.

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
pub use model::{Board, Crumb, Mode, Row, Screen};
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

    fn board_mut(&mut self) -> Option<&mut Vec<Departure>> {
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
}

impl App {
    pub fn new(conn: Connection, cache_dir: std::path::PathBuf) -> Result<Self> {
        // Kick the realtime fetch off now — by the time anyone reaches the
        // departures board several keystrokes later it is usually done — and
        // then keep it fresh, so "live" keeps meaning live while you watch.
        let today = Local::now().date_naive();
        Self::build(conn, poll::start(cache_dir), today, now_secs(today))
    }

    /// An app with no realtime thread and a fixed clock.
    ///
    /// Tests must not reach the network — and a `.env` sitting in the working
    /// directory means `new` would find a real key and start polling. Pinning
    /// the date also makes every schedule assertion reproducible.
    #[cfg(test)]
    pub fn offline(conn: Connection, today: NaiveDate, now: i32) -> Result<Self> {
        Self::build(conn, Arc::new(Mutex::new(RtState::Off)), today, now)
    }

    fn build(
        conn: Connection,
        rt_state: Arc<Mutex<RtState>>,
        today_date: NaiveDate,
        now: i32,
    ) -> Result<Self> {
        let yday_date = today_date.pred_opt().unwrap_or(today_date);
        let today = db::active_services(&conn, today_date)?;
        let yesterday = db::active_services(&conn, yday_date)?;
        let count = |m: Mode| db::routes_for_type(&conn, m.route_type(), &today).map(|r| r.len());
        let n_bus = count(Mode::Bus)?;
        let n_rail = count(Mode::Train)?;

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
        };
        app.contents = app.load(&Screen::Mode)?;
        Ok(app)
    }

    /// What a screen shows. The single place any screen's contents come from,
    /// so arriving forwards, arriving backwards and refreshing in place cannot
    /// disagree about what belongs there.
    fn load(&self, screen: &Screen) -> Result<Contents> {
        let rows: Vec<Row> = match screen {
            Screen::Mode => vec![
                Row::Mode(Mode::Bus, self.n_bus),
                Row::Mode(Mode::Train, self.n_rail),
            ],
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
    pub fn refresh_board(&mut self) -> Result<()> {
        let gone = self
            .contents
            .board()
            .and_then(<[Departure]>::first)
            .is_some_and(|d| d.when() < self.now);
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
        let Some(stop_id) = self.screen.board().map(|b| b.stop().stop_id.clone()) else {
            return;
        };
        let date = self.service_date;
        // Clone the handle so the guard does not borrow `self`, which the
        // board below needs mutably.
        let slot = Arc::clone(&self.rt);
        let Ok(guard) = slot.lock() else { return };
        let RtState::Ready(rt) = &*guard else { return };
        let Some(deps) = self.contents.board_mut() else {
            return;
        };
        for d in deps.iter_mut() {
            d.canceled = rt.is_canceled(&d.trip_id);
            d.live = rt
                .arrival(&d.trip_id, &stop_id)
                .and_then(|e| epoch_to_service_secs(e, date));
        }
        // The board is ordered by when a bus actually arrives, not when it was
        // meant to. Without this a trip running late sorts ahead of one on time.
        db::sort_by_actual_arrival(deps);
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
mod tests {
    use super::*;
    use crate::testing::TestGtfs;

    /// Two routes, two directions on one of them, three stops.
    fn app() -> App {
        let g = TestGtfs::new()
            .route("5", "5", 3, "0057B8")
            .route("7", "7", 3, "6D6E70")
            .route("1", "1", 0, "D62408")
            .always("A")
            .trip("t5", "5", "A", "Elmvale")
            .trip("t5b", "5", "A", "Barrhaven")
            .trip("t7", "7", "A", "St-Laurent")
            .trip("t1", "1", "A", "Blair")
            .stop("s1", "0001", "BANK / SOMERSET W")
            .stop("s2", "0002", "BANK / GLADSTONE")
            .stop_time("t5", "s1", 1, "10:00:00")
            .stop_time("t5", "s2", 2, "10:05:00")
            .stop_time("t5b", "s1", 1, "11:00:00")
            .stop_time("t7", "s1", 1, "10:30:00")
            .stop_time("t1", "s2", 1, "10:45:00");
        App::offline(
            g.into_conn(),
            NaiveDate::from_ymd_opt(2026, 8, 21).unwrap(),
            9 * 3600,
        )
        .unwrap()
    }

    /// The board on screen. Tests read it the way the renderer does.
    fn deps(app: &App) -> &[crate::db::Departure] {
        app.contents.board().expect("this screen shows a board")
    }

    /// Just the scheduled times, for asserting on order.
    fn board(app: &App) -> Vec<i32> {
        deps(app).iter().map(|d| d.secs).collect()
    }

    fn labels(app: &App) -> Vec<String> {
        app.rows().iter().map(Row::primary).collect()
    }

    // ---------- navigation ----------

    #[test]
    fn going_back_shows_the_same_rows_you_came_from() {
        // Each screen's rows used to be whatever a previous transition happened
        // to leave in a field. They are loaded from the screen itself now, so
        // arriving backwards has to produce exactly what arriving forwards did.
        let mut app = app();
        let mut down = vec![labels(&app)];
        for _ in 0..3 {
            app.enter().unwrap();
            down.push(labels(&app));
        }
        assert_eq!(
            down[1],
            vec!["5", "7"],
            "bus routes, sorted numerically: {down:?}"
        );

        let mut up = vec![labels(&app)];
        for _ in 0..3 {
            app.back().unwrap();
            up.push(labels(&app));
        }
        up.reverse();
        assert_eq!(up, down, "the way back does not match the way down");
    }

    #[test]
    fn the_cursor_lands_on_the_row_you_chose_not_the_one_you_typed_past() {
        // Filtering renumbers the visible rows, so entering must act on the
        // row's payload rather than on the cursor's index into the full list.
        let mut app = app();
        app.enter().unwrap(); // routes
        app.push_filter('7').unwrap();
        assert_eq!(labels(&app), vec!["7"], "filtered to one route");
        app.enter().unwrap();
        assert_eq!(
            app.screen.route().map(|r| r.short_name.clone()),
            Some("7".to_string())
        );
    }

    #[test]
    fn a_filter_matches_what_you_would_type_not_the_dim_detail() {
        // "toward Elmvale / 1 trips today": typing 1 must not match the count.
        let mut app = app();
        app.enter().unwrap(); // routes
        app.enter().unwrap(); // directions of route 5
        assert_eq!(labels(&app).len(), 2, "route 5 runs both ways");
        app.push_filter('1').unwrap();
        assert!(
            app.rows().is_empty(),
            "the trip count is not searchable: {:?}",
            labels(&app)
        );
    }

    #[test]
    fn a_board_you_drilled_down_to_shows_only_that_route_and_direction() {
        // Which departures a board carries is decided by how it was reached.
        // The same stop, opened from search, is a different board (below).
        let mut app = app();
        for _ in 0..4 {
            app.enter().unwrap(); // bus -> route 5 -> a direction -> first stop
        }
        let Screen::Departures(Board::Route { dir, stop, .. }) = &app.screen else {
            panic!("expected a drilled-down board, got {:?}", app.screen);
        };
        assert_eq!(stop.name, "BANK / SOMERSET W");
        // Route 7 also calls here and route 5 also runs the other way, so a
        // board that ignored how it was reached would carry three rows.
        assert_eq!(board(&app).len(), 1, "{:?}", board(&app));
        assert_eq!(deps(&app)[0].route_short, "5");
        assert_eq!(deps(&app)[0].headsign, dir.headsign);
    }

    #[test]
    fn a_board_reached_by_search_shows_every_route_calling_there() {
        let mut app = app();
        for c in "somerset".chars() {
            app.push_filter(c).unwrap();
        }
        app.enter().unwrap();
        let mut trips: Vec<&str> = deps(&app).iter().map(|d| d.trip_id.as_str()).collect();
        trips.sort_unstable();
        assert_eq!(trips, vec!["t5", "t5b", "t7"], "the whole stop, both ways");
    }

    #[test]
    fn the_first_screen_offers_bus_and_train_with_their_counts() {
        let app = app();
        assert_eq!(labels(&app), vec!["Bus", "O-Train"]);
        assert_eq!(app.rows()[0].secondary(), "2 routes running today");
        assert_eq!(
            app.rows()[1].secondary(),
            "1 lines · scheduled times only",
            "rail is counted separately"
        );
    }

    #[test]
    fn a_feed_failure_is_reported_without_being_cut_at_a_url() {
        // The note used to split on ':' as well as newline, to turn
        // "http status: 401" into "http status" — which also threw away the
        // 401, and cut every transport error at the scheme of its URL.
        let app = app();
        *app.rt.lock().unwrap() =
            RtState::Failed("error sending request for url (https://api.example/x)".into());
        let note = app
            .rt_note()
            .expect("a failed feed still has something to say");
        assert!(
            note.contains("example"),
            "the note stops at the scheme: {note:?}"
        );
        assert!(note.starts_with("live: "), "{note:?}");
    }

    #[test]
    fn a_feed_failure_note_keeps_only_its_first_line() {
        let app = app();
        *app.rt.lock().unwrap() = RtState::Failed("timed out\nwhile connecting".into());
        assert_eq!(app.rt_note().unwrap(), "live: timed out");
    }

    // ---------- typing ----------

    #[test]
    fn typing_at_the_first_screen_opens_the_search() {
        // The search is a screen, not a mode of the first one: it asks a
        // different question and lists a different thing.
        let mut app = app();
        assert_eq!(app.title(), "What are you taking?");
        app.push_filter('b').unwrap();
        assert!(
            matches!(app.screen, Screen::Search { .. }),
            "{:?}",
            app.screen
        );
        assert_eq!(app.title(), "Transit stops");
        assert_eq!(app.screen.typed(), "b");
        assert!(app.rows().iter().all(|r| matches!(r, Row::Hit(_))));
    }

    #[test]
    fn deleting_the_last_of_the_query_leaves_the_search() {
        let mut app = app();
        app.push_filter('b').unwrap();
        assert!(app.pop_filter().unwrap());
        assert!(matches!(app.screen, Screen::Mode), "{:?}", app.screen);
        assert_eq!(labels(&app), vec!["Bus", "O-Train"]);
    }

    #[test]
    fn a_filter_belongs_to_the_screen_that_took_it() {
        // The filter used to be one field on App, so it had to be cleared by
        // hand on every move. Each screen owns its own now.
        let mut app = app();
        app.enter().unwrap(); // routes
        app.push_filter('7').unwrap();
        assert_eq!(app.screen.typed(), "7");
        app.enter().unwrap(); // directions of route 7
        assert_eq!(app.screen.typed(), "", "the next screen inherited a filter");
        app.back().unwrap();
        assert_eq!(app.screen.typed(), "", "going back restored a stale filter");
        assert_eq!(labels(&app), vec!["5", "7"]);
    }

    #[test]
    fn a_screen_shows_a_list_or_a_board_and_never_both() {
        let mut app = app();
        assert!(app.contents.board().is_none(), "the first screen is a list");
        for _ in 0..4 {
            app.enter().unwrap();
        }
        assert!(app.contents.board().is_some(), "a board");
        assert!(app.rows().is_empty(), "a board has no selectable rows");
        app.back().unwrap();
        assert!(app.contents.board().is_none(), "back to a list");
    }

    // ---------- the board over time ----------

    #[test]
    fn a_board_left_open_refills_as_departures_go() {
        // The board is queried once, and `tick` then moves the clock under it.
        // Without a refetch the visible rows are all buses that already left,
        // while the ones still to come sit unshown further down `deps`.
        let mut app = app();
        for _ in 0..4 {
            app.enter().unwrap(); // bus -> 5 -> a direction -> first stop
        }
        assert_eq!(deps(&app).len(), 1, "one departure on this route at s1");
        let gone = deps(&app)[0].secs;

        app.now = gone + 60; // the bus has left
        app.refresh_board().unwrap();
        assert!(
            deps(&app).iter().all(|d| d.secs > app.now),
            "a departed bus is still on the board: {:?}",
            board(&app)
        );
    }

    #[test]
    fn a_board_is_left_alone_while_nothing_has_gone() {
        let mut app = app();
        for _ in 0..4 {
            app.enter().unwrap();
        }
        let before = board(&app);
        app.refresh_board().unwrap();
        assert_eq!(
            before,
            board(&app),
            "nothing has left, so nothing should have been re-queried"
        );
    }

    #[test]
    fn typing_on_a_board_does_not_shadow_quit_and_back() {
        // A board has no list to narrow, so a keystroke there only had the
        // effect of making `typing` true, which disabled q and turned esc into
        // "clear the filter" with nothing to clear.
        let mut app = app();
        for _ in 0..4 {
            app.enter().unwrap();
        }
        app.push_filter('a').unwrap();
        assert!(
            app.screen.typed().is_empty(),
            "a board swallowed a keystroke"
        );
    }

    #[test]
    fn a_failed_step_back_leaves_you_where_you_were() {
        // back() used to move the screen out before loading the one behind it,
        // so a query failure dropped the real screen and left Mode in its
        // place. Nothing observes that today only because the error exits.
        let mut app = app();
        for _ in 0..4 {
            app.enter().unwrap();
        }
        let before = format!("{:?}", app.screen);
        app.conn.execute("DROP TABLE stop_times", []).unwrap();
        assert!(app.back().is_err(), "the load should have failed");
        assert_eq!(
            format!("{:?}", app.screen),
            before,
            "a failed load stranded the app on another screen"
        );
    }

    // ---------- stop names ----------

    #[test]
    fn the_otrain_direction_suffix_is_dropped_from_station_names() {
        assert_eq!(tidy_stop_name("RIDEAU O-TRAIN EAST / EST"), "RIDEAU");
        assert_eq!(tidy_stop_name("BAYVIEW A"), "BAYVIEW A", "left alone");
    }
}
