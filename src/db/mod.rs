//! Query layer. Every level of the drill-down is one function here.
//!
//! Split by the question each part answers: [`calendar`] decides which services
//! run today, [`browse`] walks route to direction to stop to board, and
//! [`search`] finds a stop by name or pole number. The types they all return
//! live here.
//!
//! Handles the three GTFS traps we found in the OC Transpo feed:
//!   1. route_ids are duplicated across booking periods (7 and 7-1) -> we group
//!      by short_name and only keep routes with service running on the date.
//!   2. arrival times can exceed 24:00:00 (up to 28:xx) for after-midnight trips,
//!      so late-night departures belong to the *previous* service day.
//!   3. direction_id is meaningless to a human; we surface trip_headsign instead.

mod browse;
mod calendar;
mod search;

pub use browse::{
    Narrow, departures, directions_for_route, routes_for_type, sort_by_actual_arrival, stops_by_id,
    stops_for_direction,
};
pub use calendar::active_services;
pub use search::{search_stops, strip_platform};

#[derive(Debug, Clone)]
pub struct Route {
    pub short_name: String,
    pub long_name: String,
    /// GTFS route_color. We derive the text colour by contrast rather than
    /// trusting route_text_color, which OC Transpo doesn't always get right.
    pub color: String,
    /// All route_ids sharing this short_name that run on the chosen date.
    /// OC Transpo publishes one per booking period ("7" and "7-1").
    pub route_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Direction {
    pub headsign: String,
    pub trips: i64,
}

#[derive(Debug, Clone)]
pub struct StopRow {
    pub stop_id: String,
    pub code: String,
    pub name: String,
}

/// A stop returned by search, with the routes that actually serve it today.
#[derive(Debug, Clone)]
pub struct StopHit {
    pub stop_id: String,
    pub code: String,
    pub name: String,
    /// GTFS platform_code: "A", "C", "1", "3B". Empty for the 96% of stops
    /// that aren't station platforms.
    pub platform: String,
    /// Route short_names serving this platform today, naturally sorted.
    pub routes: Vec<String>,
    /// Where most trips from this platform are headed. Opposite sides of the
    /// same corner share a name and a route list; this is what tells them apart.
    pub toward: String,
}

impl From<StopHit> for StopRow {
    /// Opening a search result's board needs only the stop itself; the routes
    /// and destination were there to tell the results apart.
    fn from(h: StopHit) -> Self {
        StopRow {
            stop_id: h.stop_id,
            code: h.code,
            name: h.name,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Departure {
    /// Scheduled time, seconds since midnight of *today*.
    pub secs: i32,
    /// Join key for GTFS-Realtime TripUpdates.
    pub trip_id: String,
    /// Route identity travels with the row: a stop board mixes routes.
    pub route_short: String,
    pub route_color: String,
    pub headsign: String,
    /// True when this trip belongs to yesterday's service day (a 25:10 departure).
    pub after_midnight: bool,
    /// Live predicted time, same units as `secs`. None = no prediction.
    pub live: Option<i32>,
    pub canceled: bool,
}

impl Departure {
    /// When this bus actually arrives: the prediction if there is one, the
    /// timetable otherwise. The number the board is ordered by, the number it
    /// displays, and the number that decides whether it has already gone —
    /// which is why it is defined once rather than at each of those sites.
    pub fn when(&self) -> i32 {
        self.live.unwrap_or(self.secs)
    }
}

/// Expand a slice into `?,?,?` for an `IN` clause, since SQLite has no array
/// binding. An empty slice yields an empty list, which SQLite accepts and reads
/// as false — callers that return early on empty do it to skip the query, not
/// to avoid an error.
fn placeholders(n: usize) -> String {
    std::iter::repeat_n("?", n).collect::<Vec<_>>().join(",")
}

/// Sort "7" before "10", and put letter-suffixed routes after their number.
fn natural_key(s: &str) -> (i64, String) {
    let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
    (digits.parse::<i64>().unwrap_or(i64::MAX), s.to_string())
}

/// The "when" half of a departure query: which service days are live, and the
/// current time as seconds since local midnight.
pub struct ServiceDay<'a> {
    pub today: &'a [String],
    pub yesterday: &'a [String],
    pub now: i32,
}
