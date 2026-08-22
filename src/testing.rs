//! Fixture builders for tests.
//!
//! See TESTING.md. The rule these exist to serve: a test controls every input.
//! `TestGtfs` gives a query the exact three stops it cares about in an
//! in-memory database; `TestRt` gives the parser a payload with the exact
//! anomaly under test. Neither touches the real cache, the network, or a clock.

use rusqlite::{Connection, params};
use serde_json::{Value, json};

/// A tiny GTFS cache, built in memory, with the production schema.
///
/// Declare only what the test needs — a test about after-midnight times wants
/// one trip, not a synthetic city.
/// A service-id list, the shape every query takes for "what runs today".
pub fn svc(ids: &[&str]) -> Vec<String> {
    ids.iter().map(std::string::ToString::to_string).collect()
}

pub struct TestGtfs {
    conn: Connection,
}

impl TestGtfs {
    pub fn new() -> Self {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        crate::gtfs::create_schema(&conn).expect("schema");
        Self { conn }
    }

    /// `route_type`: 3 = bus, 0 = O-Train.
    pub fn route(self, route_id: &str, short_name: &str, route_type: i64, color: &str) -> Self {
        self.route_named(route_id, short_name, short_name, route_type, color)
    }

    /// A route whose long name differs — that name is the secondary column on
    /// the route list, so it's what tests of truncation need.
    pub fn route_named(
        self,
        route_id: &str,
        short_name: &str,
        long_name: &str,
        route_type: i64,
        color: &str,
    ) -> Self {
        self.conn
            .execute(
                "INSERT INTO routes VALUES (?1,?2,?3,?4,?5,'FFFFFF',0)",
                params![route_id, short_name, long_name, route_type, color],
            )
            .expect("insert route");
        self
    }

    /// `days` is seven characters, Monday first: "1111100" is weekdays.
    pub fn service(self, service_id: &str, days: &str, start: &str, end: &str) -> Self {
        let d: Vec<i64> = days.chars().map(|c| i64::from(c == '1')).collect();
        assert_eq!(d.len(), 7, "days must be 7 chars, Monday first");
        self.conn
            .execute(
                "INSERT INTO calendar VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    service_id, d[0], d[1], d[2], d[3], d[4], d[5], d[6], start, end
                ],
            )
            .expect("insert calendar");
        self
    }

    /// Runs every day of a wide window. For tests where the calendar isn't the
    /// subject.
    pub fn always(self, service_id: &str) -> Self {
        self.service(service_id, "1111111", "20200101", "20991231")
    }

    /// `exception`: 1 = added on that date, 2 = removed.
    pub fn service_exception(self, service_id: &str, date: &str, exception: i64) -> Self {
        self.conn
            .execute(
                "INSERT INTO calendar_dates VALUES (?1,?2,?3)",
                params![service_id, date, exception],
            )
            .expect("insert calendar_date");
        self
    }

    pub fn trip(self, trip_id: &str, route_id: &str, service_id: &str, headsign: &str) -> Self {
        self.conn
            .execute(
                "INSERT INTO trips VALUES (?1,?2,?3,?4,0)",
                params![trip_id, route_id, service_id, headsign],
            )
            .expect("insert trip");
        self
    }

    pub fn stop(self, stop_id: &str, code: &str, name: &str) -> Self {
        self.stop_on_platform(stop_id, code, name, "")
    }

    pub fn stop_on_platform(self, stop_id: &str, code: &str, name: &str, platform: &str) -> Self {
        self.conn
            .execute(
                "INSERT INTO stops VALUES (?1,?2,?3,0.0,0.0,0,'',?4)",
                params![stop_id, code, name, platform],
            )
            .expect("insert stop");
        self
    }

    /// `time` is "HH:MM:SS" and may exceed 24:00:00, exactly as the feed does.
    pub fn stop_time(self, trip_id: &str, stop_id: &str, seq: i64, time: &str) -> Self {
        let secs = crate::gtfs::parse_hms(time).expect("HH:MM:SS");
        self.conn
            .execute(
                "INSERT INTO stop_times VALUES (?1,?2,?3,?4,?4)",
                params![trip_id, stop_id, seq, secs],
            )
            .expect("insert stop_time");
        self
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Hand the connection over, for tests that need to build an `App`.
    pub fn into_conn(self) -> Connection {
        self.conn
    }
}

/// A directory of GTFS CSV files, for testing the ingest itself.
///
/// Every file starts as a minimal valid default; a test overrides only the one
/// it is about. Ingest opens all six, so none can simply be absent.
pub struct TestFeed {
    dir: tempfile::TempDir,
}

impl TestFeed {
    pub fn new() -> Self {
        let feed = Self {
            dir: tempfile::tempdir().expect("temp dir"),
        };
        feed.write("agency.txt", "agency_id,agency_name\n1,Test\n");
        feed.write("routes.txt", "route_id,route_short_name,route_long_name,route_type,route_color,route_text_color,route_sort_order\n7,7,Carleton,3,0057B8,FFFFFF,0\n");
        feed.write("stops.txt", "stop_id,stop_code,stop_name,stop_lat,stop_lon,location_type,parent_station,platform_code\nS1,3009,RIDEAU A,45.0,-75.0,0,3009_stn,A\n");
        feed.write(
            "trips.txt",
            "route_id,service_id,trip_id,trip_headsign,direction_id\n7,WD,t1,St-Laurent,0\n",
        );
        feed.write("calendar.txt", "service_id,monday,tuesday,wednesday,thursday,friday,saturday,sunday,start_date,end_date\nWD,1,1,1,1,1,0,0,20260101,20261231\n");
        feed.write(
            "calendar_dates.txt",
            "service_id,date,exception_type\nWD,20260704,2\n",
        );
        feed.write("stop_times.txt", "trip_id,arrival_time,departure_time,stop_id,stop_sequence,pickup_type,drop_off_type,timepoint\nt1,10:00:00,10:00:00,S1,1,0,0,1\n");
        feed
    }

    /// Replace one file wholesale.
    pub fn write(&self, name: &str, contents: &str) -> &Self {
        std::fs::write(self.dir.path().join(name), contents).expect("write feed file");
        self
    }

    pub fn remove(&self, name: &str) -> &Self {
        let _ = std::fs::remove_file(self.dir.path().join(name));
        self
    }

    /// Run the real ingest and hand back a connection to the result.
    pub fn ingest(&self) -> anyhow::Result<Connection> {
        let db = self.dir.path().join("out.db");
        crate::gtfs::ingest(self.dir.path(), &db)?;
        Ok(Connection::open(db)?)
    }
}

/// A GTFS-Realtime TripUpdates payload in OC Transpo's shape.
///
/// That shape is a .NET serialisation: PascalCase names with a `HasX` boolean
/// beside every optional `X`. It is *not* the standard GTFS-RT JSON mapping,
/// so building payloads by hand in each test would be both tedious and easy to
/// get subtly wrong.
pub struct TestRt {
    entities: Vec<Value>,
    feed_ts: i64,
}

impl TestRt {
    pub fn new(feed_ts: i64) -> Self {
        Self {
            entities: vec![],
            feed_ts,
        }
    }

    fn push(&mut self, trip_id: &str, relationship: i64, updates: &[Value]) {
        let id = self.entities.len();
        self.entities.push(json!({
            "Id": id.to_string(),
            "TripUpdate": {
                "Trip": {
                    "TripId": trip_id,
                    "HasTripId": true,
                    "RouteId": "r",
                    "ScheduleRelationship": relationship,
                },
                "StopTimeUpdate": updates,
            }
        }));
    }

    /// A normal prediction: arrival at `stop_id` at `epoch`.
    pub fn arrival(mut self, trip_id: &str, stop_id: &str, epoch: i64) -> Self {
        self.push(
            trip_id,
            0,
            &[json!({
                "StopId": stop_id,
                "HasStopId": true,
                "Arrival": { "Time": epoch, "HasTime": true },
            })],
        );
        self
    }

    /// Only a Departure time, no Arrival — a shape the live feed emits.
    pub fn departure_only(mut self, trip_id: &str, stop_id: &str, epoch: i64) -> Self {
        self.push(
            trip_id,
            0,
            &[json!({
                "StopId": stop_id,
                "HasStopId": true,
                "Arrival": { "Time": 0, "HasTime": false },
                "Departure": { "Time": epoch, "HasTime": true },
            })],
        );
        self
    }

    /// `HasTime: false` beside a meaningless `Time`. Reading `Time` without
    /// checking the flag yields a garbage prediction.
    pub fn no_time(mut self, trip_id: &str, stop_id: &str) -> Self {
        self.push(
            trip_id,
            0,
            &[json!({
                "StopId": stop_id,
                "HasStopId": true,
                "Arrival": { "Time": 1, "HasTime": false },
            })],
        );
        self
    }

    /// ScheduleRelationship 3, with no stop updates — how cancellations arrive.
    pub fn canceled(mut self, trip_id: &str) -> Self {
        self.push(trip_id, 3, &[]);
        self
    }

    /// ScheduleRelationship 8: not in the GTFS-RT spec, but the live feed emits
    /// it for added/unscheduled trips, which carry real predictions.
    pub fn unscheduled(mut self, trip_id: &str, stop_id: &str, epoch: i64) -> Self {
        self.push(
            trip_id,
            8,
            &[json!({
                "StopId": stop_id,
                "HasStopId": true,
                "Arrival": { "Time": epoch, "HasTime": true },
            })],
        );
        self
    }

    pub fn build(self) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "Header": { "Timestamp": self.feed_ts, "GtfsRealtimeVersion": "2.0" },
            "Entity": self.entities,
        }))
        .expect("serialise")
    }
}
