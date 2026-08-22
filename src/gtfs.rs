//! Ingest static GTFS CSV files into a local SQLite cache.
//!
//! The cache is a build artifact, not user state: the app stays stateless from
//! the user's point of view, but we don't re-parse 323MB of stop_times on launch.

use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::collections::HashMap;
use std::path::Path;

/// Bumped whenever the cache layout changes, so `update` knows to rebuild even
/// when the published feed itself is unchanged.
pub const SCHEMA_VERSION: &str = "2";

const SCHEMA: &str = r"
DROP TABLE IF EXISTS routes;
DROP TABLE IF EXISTS trips;
DROP TABLE IF EXISTS stops;
DROP TABLE IF EXISTS stop_times;
DROP TABLE IF EXISTS calendar;
DROP TABLE IF EXISTS calendar_dates;
DROP TABLE IF EXISTS meta;

CREATE TABLE routes (
    route_id TEXT PRIMARY KEY, short_name TEXT, long_name TEXT,
    route_type INTEGER, color TEXT, text_color TEXT, sort_order INTEGER
);
CREATE TABLE trips (
    trip_id TEXT PRIMARY KEY, route_id TEXT, service_id TEXT,
    headsign TEXT, direction_id INTEGER
);
CREATE TABLE stops (
    stop_id TEXT PRIMARY KEY, stop_code TEXT, name TEXT,
    lat REAL, lon REAL, location_type INTEGER, parent TEXT,
    -- Sparse (215 of 5859) but clean where present: A-H, 1, 2, 1A-5A.
    -- Parsing platforms out of stop names is not viable; see search_stops.
    platform TEXT
);
-- arr/dep are seconds since midnight of the service day; may exceed 86400.
CREATE TABLE stop_times (
    trip_id TEXT, stop_id TEXT, seq INTEGER, arr INTEGER, dep INTEGER
);
CREATE TABLE calendar (
    service_id TEXT PRIMARY KEY,
    mon INTEGER, tue INTEGER, wed INTEGER, thu INTEGER,
    fri INTEGER, sat INTEGER, sun INTEGER,
    start_date TEXT, end_date TEXT
);
CREATE TABLE calendar_dates (service_id TEXT, date TEXT, exception INTEGER);
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT);
";

const INDEXES: &str = r"
CREATE INDEX idx_st_stop ON stop_times(stop_id, arr);
CREATE INDEX idx_st_trip ON stop_times(trip_id, seq);
CREATE INDEX idx_trips_route ON trips(route_id, direction_id);
CREATE INDEX idx_trips_service ON trips(service_id);
CREATE INDEX idx_cd_date ON calendar_dates(date);
";

/// Create the cache tables and indexes on a fresh connection.
///
/// Test fixtures build through this too, so a schema change can never leave
/// them testing a layout production doesn't have.
#[cfg(test)]
pub fn create_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;
    conn.execute_batch(INDEXES)?;
    Ok(())
}

/// A CSV row addressed by GTFS column name, so column order doesn't matter.
struct Row<'a> {
    rec: &'a csv::StringRecord,
    idx: &'a HashMap<String, usize>,
}

impl Row<'_> {
    fn s(&self, key: &str) -> &str {
        self.idx
            .get(key)
            .and_then(|i| self.rec.get(*i))
            .unwrap_or("")
    }
    fn i(&self, key: &str, default: i64) -> i64 {
        self.s(key).trim().parse().unwrap_or(default)
    }
    fn f(&self, key: &str) -> f64 {
        self.s(key).trim().parse().unwrap_or(0.0)
    }
}

fn open_csv(dir: &Path, name: &str) -> Result<csv::Reader<std::fs::File>> {
    let path = dir.join(name);
    csv::ReaderBuilder::new()
        .flexible(true)
        .from_path(&path)
        .with_context(|| format!("opening {}", path.display()))
}

/// Index the header row by column name, so column order doesn't matter.
///
/// GTFS files carry a UTF-8 BOM. The `csv` crate strips it already; the trim
/// here is a second line of defence in case that ever changes or the reader is
/// swapped, since the failure mode is silent — every value in the first column
/// comes back empty.
fn header_index(rdr: &mut csv::Reader<std::fs::File>) -> Result<HashMap<String, usize>> {
    Ok(rdr
        .headers()?
        .iter()
        .enumerate()
        .map(|(i, h)| (h.trim_start_matches('\u{feff}').trim().to_string(), i))
        .collect())
}

/// Stream one CSV into one table inside a single transaction.
///
/// `bind` maps a row to the parameters of `insert`, or returns None to skip it.
fn load<F>(conn: &mut Connection, dir: &Path, file: &str, insert: &str, bind: F) -> Result<u64>
where
    F: Fn(&Row) -> Option<Vec<rusqlite::types::Value>>,
{
    let tx = conn.transaction()?;
    let mut n = 0u64;
    {
        let mut rdr = open_csv(dir, file)?;
        let idx = header_index(&mut rdr)?;
        let mut stmt = tx.prepare(insert)?;
        let mut rec = csv::StringRecord::new();
        while rdr.read_record(&mut rec)? {
            if let Some(vals) = bind(&Row {
                rec: &rec,
                idx: &idx,
            }) {
                stmt.execute(rusqlite::params_from_iter(vals))?;
                n += 1;
            }
        }
    }
    tx.commit()?;
    Ok(n)
}

/// "HH:MM:SS" -> seconds since midnight. Hours may be >= 24 for trips that run
/// past midnight on the previous service day (OC Transpo goes up to 28:xx).
pub fn parse_hms(s: &str) -> Option<i32> {
    let mut it = s.trim().split(':');
    let h: i32 = it.next()?.trim().parse().ok()?;
    let m: i32 = it.next()?.trim().parse().ok()?;
    let sec: i32 = it.next().unwrap_or("0").trim().parse().unwrap_or(0);
    Some(h * 3600 + m * 60 + sec)
}

/// Small key/value store in the cache: ETag, ingest date, feed dates.
pub fn get_meta(conn: &Connection, key: &str) -> Result<Option<String>> {
    let mut st = conn.prepare("SELECT value FROM meta WHERE key = ?1")?;
    let mut rows = st.query([key])?;
    match rows.next()? {
        Some(r) => Ok(Some(r.get(0)?)),
        None => Ok(None),
    }
}

pub fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO meta VALUES (?1, ?2)",
        rusqlite::params![key, value],
    )?;
    Ok(())
}

pub fn ingest(gtfs_dir: &Path, db_path: &Path) -> Result<()> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut conn = Connection::open(db_path)?;
    conn.execute_batch(
        "PRAGMA journal_mode = OFF;
         PRAGMA synchronous = OFF;
         PRAGMA cache_size = -200000;
         PRAGMA temp_store = MEMORY;",
    )?;
    conn.execute_batch(SCHEMA)?;

    use rusqlite::types::Value as V;
    let text = |s: &str| V::Text(s.to_string());

    let n = load(
        &mut conn,
        gtfs_dir,
        "routes.txt",
        "INSERT OR REPLACE INTO routes VALUES (?,?,?,?,?,?,?)",
        |r| {
            Some(vec![
                text(r.s("route_id")),
                text(r.s("route_short_name")),
                text(r.s("route_long_name")),
                V::Integer(r.i("route_type", 3)),
                text(r.s("route_color")),
                text(r.s("route_text_color")),
                V::Integer(r.i("route_sort_order", 0)),
            ])
        },
    )?;
    eprintln!("  routes {n}");

    let n = load(
        &mut conn,
        gtfs_dir,
        "stops.txt",
        "INSERT OR REPLACE INTO stops VALUES (?,?,?,?,?,?,?,?)",
        |r| {
            Some(vec![
                text(r.s("stop_id")),
                text(r.s("stop_code")),
                text(r.s("stop_name")),
                V::Real(r.f("stop_lat")),
                V::Real(r.f("stop_lon")),
                V::Integer(r.i("location_type", 0)),
                text(r.s("parent_station")),
                text(r.s("platform_code")),
            ])
        },
    )?;
    eprintln!("  stops {n}");

    let n = load(
        &mut conn,
        gtfs_dir,
        "trips.txt",
        "INSERT OR REPLACE INTO trips VALUES (?,?,?,?,?)",
        |r| {
            Some(vec![
                text(r.s("trip_id")),
                text(r.s("route_id")),
                text(r.s("service_id")),
                text(r.s("trip_headsign")),
                V::Integer(r.i("direction_id", 0)),
            ])
        },
    )?;
    eprintln!("  trips {n}");

    let n = load(
        &mut conn,
        gtfs_dir,
        "calendar.txt",
        "INSERT OR REPLACE INTO calendar VALUES (?,?,?,?,?,?,?,?,?,?)",
        |r| {
            Some(vec![
                text(r.s("service_id")),
                V::Integer(r.i("monday", 0)),
                V::Integer(r.i("tuesday", 0)),
                V::Integer(r.i("wednesday", 0)),
                V::Integer(r.i("thursday", 0)),
                V::Integer(r.i("friday", 0)),
                V::Integer(r.i("saturday", 0)),
                V::Integer(r.i("sunday", 0)),
                text(r.s("start_date")),
                text(r.s("end_date")),
            ])
        },
    )?;
    let m = load(
        &mut conn,
        gtfs_dir,
        "calendar_dates.txt",
        "INSERT INTO calendar_dates VALUES (?,?,?)",
        |r| {
            Some(vec![
                text(r.s("service_id")),
                text(r.s("date")),
                V::Integer(r.i("exception_type", 1)),
            ])
        },
    )?;
    eprintln!("  calendar {n} + {m} exceptions");

    ingest_stop_times(&mut conn, gtfs_dir)?;

    eprintln!("  building indexes...");
    conn.execute_batch(INDEXES)?;
    conn.execute(
        "INSERT OR REPLACE INTO meta VALUES ('ingested_from', ?)",
        params![gtfs_dir.display().to_string()],
    )?;
    conn.execute(
        "INSERT OR REPLACE INTO meta VALUES ('schema_version', ?)",
        params![SCHEMA_VERSION],
    )?;
    conn.execute_batch("PRAGMA optimize;")?;
    Ok(())
}

/// stop_times is ~6.3M rows, so it gets a hand-rolled loop over `ByteRecord`
/// with the column offsets hoisted out. Worth roughly 2x over the generic path.
fn ingest_stop_times(conn: &mut Connection, dir: &Path) -> Result<()> {
    let tx = conn.transaction()?;
    {
        let mut rdr = open_csv(dir, "stop_times.txt")?;
        let idx = header_index(&mut rdr)?;
        let col = |k: &str| -> Result<usize> {
            idx.get(k)
                .copied()
                .with_context(|| format!("stop_times.txt has no {k} column"))
        };
        let (i_trip, i_arr, i_dep, i_stop, i_seq) = (
            col("trip_id")?,
            col("arrival_time")?,
            col("departure_time")?,
            col("stop_id")?,
            col("stop_sequence")?,
        );

        let mut stmt = tx.prepare("INSERT INTO stop_times VALUES (?,?,?,?,?)")?;
        let mut rec = csv::ByteRecord::new();
        let mut n = 0u64;
        while rdr.read_byte_record(&mut rec)? {
            let f = |i: usize| std::str::from_utf8(rec.get(i).unwrap_or(b"")).unwrap_or("");
            let arr = parse_hms(f(i_arr));
            let dep = parse_hms(f(i_dep)).or(arr);
            stmt.execute(params![
                f(i_trip),
                f(i_stop),
                f(i_seq).parse::<i64>().unwrap_or(0),
                arr,
                dep
            ])?;
            n += 1;
            if n.is_multiple_of(1_000_000) {
                eprintln!("  stop_times {}M...", n / 1_000_000);
            }
        }
        eprintln!("  stop_times {n}");
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestFeed;

    fn one<T: rusqlite::types::FromSql>(conn: &Connection, sql: &str) -> T {
        conn.query_row(sql, [], |r| r.get(0)).expect(sql)
    }

    // ---------- parse_hms ----------

    #[test]
    fn parses_a_normal_clock_time() {
        assert_eq!(parse_hms("10:05:30"), Some(10 * 3600 + 5 * 60 + 30));
        assert_eq!(parse_hms("00:00:00"), Some(0));
    }

    #[test]
    fn parses_times_past_midnight_without_wrapping_them() {
        // The feed reaches 28:xx. Wrapping here would silently move a 1am trip
        // to the start of the day, and it is the trips you most want at 1am.
        assert_eq!(parse_hms("25:10:00"), Some(25 * 3600 + 10 * 60));
        assert_eq!(parse_hms("28:45:00"), Some(28 * 3600 + 45 * 60));
    }

    #[test]
    fn tolerates_whitespace_and_a_missing_seconds_field() {
        assert_eq!(parse_hms(" 10:05 "), Some(10 * 3600 + 5 * 60));
    }

    #[test]
    fn rejects_what_is_not_a_time() {
        assert_eq!(parse_hms(""), None);
        assert_eq!(parse_hms("10"), None);
        assert_eq!(parse_hms("abc"), None);
    }

    // ---------- ingest ----------

    #[test]
    fn ingests_a_minimal_feed_into_every_table() {
        let conn = TestFeed::new().ingest().unwrap();
        assert_eq!(one::<i64>(&conn, "SELECT COUNT(*) FROM routes"), 1);
        assert_eq!(one::<i64>(&conn, "SELECT COUNT(*) FROM stops"), 1);
        assert_eq!(one::<i64>(&conn, "SELECT COUNT(*) FROM trips"), 1);
        assert_eq!(one::<i64>(&conn, "SELECT COUNT(*) FROM calendar"), 1);
        assert_eq!(one::<i64>(&conn, "SELECT COUNT(*) FROM calendar_dates"), 1);
        assert_eq!(one::<i64>(&conn, "SELECT COUNT(*) FROM stop_times"), 1);
    }

    #[test]
    fn a_feed_carrying_a_byte_order_mark_ingests_correctly() {
        // OC Transpo ships every file with a UTF-8 BOM, and a BOM left on the
        // first header makes it read "\u{feff}route_id", so every route_id
        // comes out empty.
        //
        // This asserts the property, not our mechanism: the `csv` crate strips
        // the BOM on its own, so `header_index`'s own trim is belt-and-braces
        // and removing it does not fail this test. Keep the test — the property
        // matters whoever provides it.
        let feed = TestFeed::new();
        feed.write(
            "routes.txt",
            "\u{feff}route_id,route_short_name,route_long_name,route_type,route_color,route_text_color,route_sort_order\n99,99,Somewhere,3,FFFFFF,000000,0\n",
        );
        let conn = feed.ingest().unwrap();
        assert_eq!(one::<String>(&conn, "SELECT route_id FROM routes"), "99");
    }

    #[test]
    fn columns_are_read_by_name_so_their_order_does_not_matter() {
        let feed = TestFeed::new();
        feed.write(
            "stops.txt",
            "platform_code,stop_name,stop_id,stop_code,location_type,parent_station,stop_lon,stop_lat\nC,BAYVIEW C,S9,3060,0,3060_stn,-75.7,45.4\n",
        );
        let conn = feed.ingest().unwrap();
        assert_eq!(one::<String>(&conn, "SELECT stop_id FROM stops"), "S9");
        assert_eq!(one::<String>(&conn, "SELECT name FROM stops"), "BAYVIEW C");
        assert_eq!(one::<String>(&conn, "SELECT platform FROM stops"), "C");
    }

    #[test]
    fn times_past_midnight_survive_ingest_as_written() {
        let feed = TestFeed::new();
        feed.write(
            "stop_times.txt",
            "trip_id,arrival_time,departure_time,stop_id,stop_sequence\nt1,25:10:00,25:11:00,S1,1\n",
        );
        let conn = feed.ingest().unwrap();
        assert_eq!(
            one::<i64>(&conn, "SELECT arr FROM stop_times"),
            25 * 3600 + 600
        );
        assert_eq!(
            one::<i64>(&conn, "SELECT dep FROM stop_times"),
            25 * 3600 + 660
        );
    }

    #[test]
    fn a_missing_departure_time_falls_back_to_the_arrival() {
        let feed = TestFeed::new();
        feed.write(
            "stop_times.txt",
            "trip_id,arrival_time,departure_time,stop_id,stop_sequence\nt1,10:00:00,,S1,1\n",
        );
        let conn = feed.ingest().unwrap();
        assert_eq!(one::<i64>(&conn, "SELECT dep FROM stop_times"), 10 * 3600);
    }

    #[test]
    fn an_absent_optional_column_becomes_empty_not_an_error() {
        // platform_code is absent for most agencies; its absence must not fail
        // the whole ingest.
        let feed = TestFeed::new();
        feed.write(
            "stops.txt",
            "stop_id,stop_code,stop_name,stop_lat,stop_lon,location_type,parent_station\nS1,1902,BANK / SOMERSET W,45.0,-75.0,0,\n",
        );
        let conn = feed.ingest().unwrap();
        assert_eq!(one::<String>(&conn, "SELECT platform FROM stops"), "");
    }

    #[test]
    fn the_schema_version_is_stamped_so_a_layout_change_forces_a_rebuild() {
        let conn = TestFeed::new().ingest().unwrap();
        let v: String = one(&conn, "SELECT value FROM meta WHERE key='schema_version'");
        assert_eq!(v, SCHEMA_VERSION);
    }

    #[test]
    fn a_missing_required_column_names_the_file_and_the_column() {
        let feed = TestFeed::new();
        feed.write(
            "stop_times.txt",
            "trip_id,arrival_time,stop_id,stop_sequence\nt1,10:00:00,S1,1\n",
        );
        let err = feed.ingest().unwrap_err().to_string();
        assert!(
            err.contains("departure_time") && err.contains("stop_times"),
            "error should say what is missing and where, got: {err}"
        );
    }

    #[test]
    fn a_missing_file_names_the_file() {
        let feed = TestFeed::new();
        feed.remove("trips.txt");
        let err = format!("{:#}", feed.ingest().unwrap_err());
        assert!(err.contains("trips.txt"), "got: {err}");
    }

    #[test]
    fn ingesting_twice_replaces_rather_than_accumulates() {
        // update() rebuilds into a fresh file, but the schema drops its tables
        // first — so a re-ingest over an existing database must not double up.
        let feed = TestFeed::new();
        let db = feed.ingest().unwrap();
        drop(db);
        let conn = feed.ingest().unwrap();
        assert_eq!(one::<i64>(&conn, "SELECT COUNT(*) FROM routes"), 1);
    }
}
