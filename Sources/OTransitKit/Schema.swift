// The database an ingest builds, and the database every query reads.
//
// It lives in one place so a test fixture and the real file cannot drift apart.
// A fixture carrying its own copy would keep passing while a query that depends
// on a column stopped working against the file on disk.
//
// Dates are written YYYYMMDD, which sorts and compares as text. Times are
// seconds on the service day, which reach past 86400 because a schedule does.

public enum Schema {
    /// What shape this program expects a cache to be.
    ///
    /// Written into `meta` when one is built and checked when one is opened. A
    /// cache built by an older version has the right file name and the wrong
    /// columns, and a query against it fails somewhere far from here — or
    /// worse, returns nothing and looks like a quiet day. Raise this whenever
    /// a table below changes.
    public static let version = "2"

    public static let tables = """
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
            -- Sparse but clean where present. A platform is never read out of a
            -- stop name, because 'CANTERBURY / AD. 860' carries a municipal
            -- address and no platform at all.
            platform TEXT
        );
        CREATE TABLE stop_times (
            trip_id TEXT, stop_id TEXT, seq INTEGER, arr INTEGER, dep INTEGER,
            -- Whether a bus takes anyone on here. The feed writes 1 at a
            -- trip's last stop, where everyone gets off and nobody gets on —
            -- 75,249 calls of three million. Without it a terminus counts a
            -- rider down to a bus they cannot board: route 75 calls at
            -- Tunney's Pasture 893 times a day toward Tunney's Pasture, and
            -- not one of them is boardable.
            --
            -- Per call and not per stop: stop 121 on route 86 toward Antares
            -- takes 340 trips that end there and 77 that carry on past it, and
            -- nothing about the stop says which a given bus is doing.
            boards INTEGER
        );
        CREATE TABLE calendar (
            service_id TEXT PRIMARY KEY,
            mon INTEGER, tue INTEGER, wed INTEGER, thu INTEGER,
            fri INTEGER, sat INTEGER, sun INTEGER,
            start_date TEXT, end_date TEXT
        );
        CREATE TABLE calendar_dates (service_id TEXT, date TEXT, exception INTEGER);
        CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT);
        """

    /// Applied after the rows, because maintaining five indexes across six
    /// million inserts costs more than building them once at the end. They are
    /// declared beside the tables for the reason the tables are here: a fixture
    /// that queries without them tests a plan the real file does not have.
    public static let indexes = """
        CREATE INDEX idx_st_stop ON stop_times(stop_id, arr);
        CREATE INDEX idx_st_trip ON stop_times(trip_id, seq);
        CREATE INDEX idx_trips_route ON trips(route_id, direction_id);
        CREATE INDEX idx_trips_service ON trips(service_id);
        CREATE INDEX idx_cd_date ON calendar_dates(date);
        """
}
