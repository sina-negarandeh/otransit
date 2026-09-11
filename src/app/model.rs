//! What a screen is, and what a row on it stands for.
//!
//! Pure data and the questions you can ask of it. `App` in the parent module
//! is what moves between these states; nothing here touches the database, the
//! clock or the network.

use super::tidy_stop_name;
use crate::db::{self, Direction, Route, StopHit, StopRow};

/// One selectable line, carrying the value it stands for.
#[derive(Clone, Debug)]
pub enum Row {
    /// Bus or train, with how many routes or lines run today.
    Mode(Mode, usize),
    Route(Route),
    Direction(Direction),
    Stop(StopRow),
    /// A stop search result.
    Hit(StopHit),
    /// A pinned stop, on the first screen above the modes, carrying the next
    /// bus so the answer is on screen before anything is pressed.
    Pin(Pinned),
}

/// A pinned stop and the departures it might show.
///
/// More than one is fetched even though one is drawn. `departures` can only
/// order by the timetable, so the scheduled-earliest bus is not always the one
/// that arrives first: a trip running late is overtaken by one scheduled after
/// it. Taking `LIMIT 1` here would show a bus that is not next, which is the
/// defect the board already carries a test for.
///
/// The list is re-sorted by actual arrival once realtime lands, and `next`
/// reads the front of it.
#[derive(Clone, Debug)]
pub struct Pinned {
    /// The board this pin was made from, rebuilt against today's cache.
    pub board: Board,
    pub upcoming: Vec<crate::db::Departure>,
}

impl Pinned {
    /// The stop this pin is at. A shorthand for the reach through the board,
    /// which six call sites were spelling out.
    pub fn stop(&self) -> &StopRow {
        self.board.stop()
    }

    /// The one departure this pin shows: whichever arrives first.
    pub fn next(&self) -> Option<&crate::db::Departure> {
        self.upcoming.first()
    }
}

impl Row {
    /// The bold left-hand label.
    pub fn primary(&self) -> String {
        match self {
            Row::Mode(m, _) => m.label().to_string(),
            Row::Route(r) => r.short_name.clone(),
            Row::Direction(d) => format!("toward {}", d.headsign),
            Row::Stop(s) => tidy_stop_name(&s.name),
            Row::Pin(p) => tidy_stop_name(&p.stop().name),
            Row::Hit(h) => crate::db::strip_platform(&h.name, &h.platform),
        }
    }

    /// The dim right-hand detail.
    pub fn secondary(&self) -> String {
        match self {
            Row::Mode(Mode::Bus, n) => format!("{n} routes running today"),
            Row::Mode(Mode::Train, n) => format!("{n} lines · scheduled times only"),
            Row::Route(r) => r.long_name.clone(),
            Row::Direction(d) => format!("{} trips today", d.trips),
            Row::Stop(s) => format!("#{}", s.code),
            Row::Pin(p) => format!("#{}", p.stop().code),
            Row::Hit(h) => h.routes.join(", "),
        }
    }

    /// What type-to-filter searches. Not the same as what is displayed: you
    /// would type a route number or a stop name at a list, never "18 trips".
    ///
    /// Only the three list screens narrow in place. The first screen's filter
    /// is a database search, so its rows — modes, or the hits that replace
    /// them — are already the answer and never reach this.
    pub(super) fn matches(&self, needle: &str) -> bool {
        let hit = |s: &str| s.to_lowercase().contains(needle);
        match self {
            Row::Route(r) => hit(&r.short_name) || hit(&r.long_name),
            Row::Direction(d) => hit(&d.headsign),
            Row::Stop(s) => hit(&s.name) || hit(&s.code),
            Row::Pin(p) => hit(&p.stop().name) || hit(&p.stop().code),
            Row::Mode(..) | Row::Hit(_) => true,
        }
    }

    /// GTFS colour for rows drawn as a route badge.
    pub fn badge(&self) -> Option<&str> {
        match self {
            Row::Route(r) => Some(&r.color),
            _ => None,
        }
    }
}

/// One step of the trail in the status bar: a plain word, or a route badge.
#[derive(Clone, Debug)]
pub enum Crumb {
    Plain(String),
    /// A route or line number, drawn as a colour badge.
    Route {
        name: String,
        color: String,
    },
}

impl Crumb {
    pub(super) fn route(r: &Route) -> Self {
        Crumb::Route {
            name: r.short_name.clone(),
            color: r.color.clone(),
        }
    }
}

/// Bus or rail. Replaces a bare `route_type` integer so the two GTFS magic
/// numbers appear exactly once.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Bus,
    Train,
}

impl Mode {
    /// GTFS route_type: 3 is bus, 0 is tram/light rail.
    pub fn route_type(self) -> i64 {
        match self {
            Mode::Bus => 3,
            Mode::Train => 0,
        }
    }

    /// The inverse, for a route looked up by name rather than by mode.
    ///
    /// `None` for a type this app does not browse, which is every other value
    /// GTFS defines. A pin naming one hides rather than guessing a mode.
    pub fn from_route_type(route_type: i64) -> Option<Self> {
        match route_type {
            3 => Some(Mode::Bus),
            0 => Some(Mode::Train),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Bus => "Bus",
            Mode::Train => "O-Train",
        }
    }
}

/// Where you are, carrying everything you chose to get here.
///
/// The data lives in the variant rather than in parallel `Option` fields, so
/// states like "on the stops list with no route chosen" cannot be built. Every
/// consumer reads what it needs out of the variant instead of re-deriving which
/// fields happen to be populated.
#[derive(Clone, Debug)]
pub enum Screen {
    Mode,
    /// Typing at the first screen searches stops. That is a screen of its own,
    /// not a mode of the first one: it asks a different question, lists a
    /// different thing, and its text is a query rather than a narrowing.
    Search {
        query: String,
    },
    Routes {
        mode: Mode,
        filter: String,
    },
    Directions {
        mode: Mode,
        route: Route,
        filter: String,
    },
    Stops {
        mode: Mode,
        route: Route,
        headsign: String,
        filter: String,
    },
    Departures(Board),
}

/// A departures board, and how it was reached. This is what used to be encoded
/// as `sel_route.is_none()` in three separate files.
#[derive(Clone, Debug)]
pub enum Board {
    /// Drilled down: one route in one direction.
    ///
    /// The direction is a headsign, not a `Direction`. That type also carries a
    /// trip count, which belongs to the directions list and which a pin rebuilt
    /// from a file has no way to know -- so carrying it here meant inventing a
    /// number, and a field named `trips` holding an invented number is the kind
    /// of confidently-wrong fact this app is built to avoid.
    Route {
        mode: Mode,
        route: Route,
        headsign: String,
        stop: StopRow,
    },
    /// Searched: everything calling at the stop.
    Stop { stop: StopRow },
}

impl Board {
    pub fn stop(&self) -> &StopRow {
        match self {
            Board::Route { stop, .. } | Board::Stop { stop } => stop,
        }
    }

    /// What makes two boards the same board: the stop, and the route and
    /// direction narrowing it.
    ///
    /// Borrowed rather than owned because `board_pin` asks this once a frame,
    /// against every pin, and building a value to answer it allocated a handful
    /// of strings each time for a comparison that needs none.
    pub fn key(&self) -> (&str, Option<&str>, Option<&str>) {
        match self {
            Board::Route {
                route,
                headsign,
                stop,
                ..
            } => (
                &stop.stop_id,
                Some(&route.short_name),
                Some(headsign.as_str()),
            ),
            Board::Stop { stop } => (&stop.stop_id, None, None),
        }
    }

    /// Which departures belong on this board: one route and direction when you
    /// drilled down to it, everything calling at the stop when you searched.
    pub(super) fn narrow(&self) -> db::Narrow<'_> {
        match self {
            Board::Route {
                route, headsign, ..
            } => db::Narrow::Route {
                route_ids: &route.route_ids,
                headsign,
            },
            Board::Stop { .. } => db::Narrow::Everything,
        }
    }
}

impl Screen {
    pub(super) fn routes(mode: Mode) -> Self {
        Screen::Routes {
            mode,
            filter: String::new(),
        }
    }

    pub(super) fn directions(mode: Mode, route: Route) -> Self {
        Screen::Directions {
            mode,
            route,
            filter: String::new(),
        }
    }

    pub(super) fn stops(mode: Mode, route: Route, headsign: String) -> Self {
        Screen::Stops {
            mode,
            route,
            headsign,
            filter: String::new(),
        }
    }

    /// Whether this screen sits under exactly one route.
    ///
    /// Two things follow from it and must follow together: the gutter down the
    /// left in the route's colour, and the detour published for that route. It
    /// is one predicate because it is one fact about a screen. Asked in two
    /// places, the two answers drifted: the snapshot of what the app decided
    /// reported a detour on the departures board, which draws none.
    pub fn below_a_route(&self) -> bool {
        matches!(self, Screen::Stops { .. } | Screen::Directions { .. })
    }

    /// The text typed at this screen, if it takes any.
    ///
    /// Three screens narrow a list in place, one *is* a query, and two take no
    /// text at all. Answering that here is what keeps the question from being
    /// re-derived at every site that cares.
    pub fn typed(&self) -> &str {
        match self {
            Screen::Search { query } => query,
            Screen::Routes { filter, .. }
            | Screen::Directions { filter, .. }
            | Screen::Stops { filter, .. } => filter,
            Screen::Mode | Screen::Departures(_) => "",
        }
    }

    pub(super) fn typed_mut(&mut self) -> Option<&mut String> {
        match self {
            Screen::Search { query } => Some(query),
            Screen::Routes { filter, .. }
            | Screen::Directions { filter, .. }
            | Screen::Stops { filter, .. } => Some(filter),
            Screen::Mode | Screen::Departures(_) => None,
        }
    }

    /// The board being shown, if this screen shows one.
    pub fn board(&self) -> Option<&Board> {
        match self {
            Screen::Departures(b) => Some(b),
            _ => None,
        }
    }

    /// Bus or train, once that has been chosen.
    pub fn mode(&self) -> Option<Mode> {
        match self {
            Screen::Routes { mode, .. }
            | Screen::Directions { mode, .. }
            | Screen::Stops { mode, .. }
            | Screen::Departures(Board::Route { mode, .. }) => Some(*mode),
            Screen::Mode | Screen::Search { .. } | Screen::Departures(Board::Stop { .. }) => None,
        }
    }

    /// The route chosen so far, if one has been.
    pub fn route(&self) -> Option<&Route> {
        match self {
            Screen::Directions { route, .. }
            | Screen::Stops { route, .. }
            | Screen::Departures(Board::Route { route, .. }) => Some(route),
            _ => None,
        }
    }
}
