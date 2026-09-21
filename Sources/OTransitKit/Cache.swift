// Every question a screen asks of the schedule, and the SQL that answers it.
//
// An actor, because the handle underneath is a C pointer and a popover can ask
// two questions at once. Serialising them here is cheaper than a connection
// pool and honest about what SQLite is doing anyway.

import Foundation

public actor Cache {
    private let db: Database

    /// Opens a cache, or refuses one this version cannot read.
    ///
    /// The check is here and not at the first query, because a cache with the
    /// wrong columns does not fail loudly: a query naming a column that is not
    /// there fails far from the cause, and one that merely reads fewer of them
    /// returns rows that look like a quiet Sunday.
    public init(path: URL) throws {
        try self.init(database: Database(path: path.path(percentEncoded: false)))
    }

    /// A cache over a database a test built. The only way in that does not name
    /// a file, and the only way a test is allowed to get one.
    ///
    /// Both doors come through here, so the stamp is checked on both: a fixture
    /// that could skip the check would keep passing against a shape the real
    /// file no longer has, which is the one thing a fixture must not do.
    public init(database: Database) throws {
        self.db = database
        let stamped = try db.prepare("SELECT value FROM meta WHERE key = 'schema'")
            .rows([]) { $0.text(0) }.first
        guard stamped == Schema.version else { throw CacheError.outdated(found: stamped) }
    }

    /// The service ids that run on `date`, written YYYYMMDD. Honours the weekly
    /// calendar, the window it is valid over, and both kinds of exception.
    public func activeServices(on date: String) throws -> [String] {
        let column = try Self.weekdayColumn(date)
        // One of seven fixed names, chosen here and never by a caller, so it
        // cannot carry anything but a column name.
        let sql = """
            SELECT service_id FROM calendar
             WHERE \(column) = 1 AND start_date <= ? AND end_date >= ?
            UNION
            SELECT service_id FROM calendar_dates WHERE date = ? AND exception = 1
            EXCEPT
            SELECT service_id FROM calendar_dates WHERE date = ? AND exception = 2
            """
        return try rows(
            sql, Array(repeating: .text(date), count: 4)
        ) { $0.text(0) }
    }

    /// The routes of one mode that run on `date`, one row per route.
    ///
    /// A `route_id` repeats across booking periods, so 44 arrives as both 44 and
    /// 44-1. They are one route to a rider and they collapse by short name.
    public func routes(_ mode: Mode, on date: String) throws -> [Route] {
        let services = try activeServices(on: date)
        if services.isEmpty { return [] }

        let sql = """
            SELECT r.short_name, MIN(r.long_name), MIN(r.color)
              FROM routes r
             WHERE r.route_type IN (\(Self.holes(mode.types.count)))
               AND EXISTS (
                   SELECT 1 FROM trips t
                    WHERE t.route_id = r.route_id
                      AND t.service_id IN (\(Self.holes(services.count))))
             GROUP BY r.short_name
            """
        let found = try rows(
            sql, mode.types.map { .int(Int64($0)) } + services.map { .text($0) }
        ) { Route(shortName: $0.text(0), longName: $0.text(1), colour: $0.text(2)) }

        // Not the feed's sort_order, which puts rail line 1 after line 2, and
        // not the text, which puts 10 between 1 and 2. It is the number a person
        // reads, worked out here: no expression SQLite has says it, and
        // short_name is the group, so the sort is total.
        return found.sorted { Self.byNumber($0.shortName, $1.shortName) }
    }

    /// The directions a route runs on a date, with how many trips each has.
    /// The busiest comes first, and a tie is broken by headsign.
    public func directions(of route: String, on date: String) throws -> [Direction] {
        let services = try activeServices(on: date)
        if services.isEmpty { return [] }

        let sql = """
            SELECT t.headsign, COUNT(*)
              FROM trips t
              JOIN routes r ON r.route_id = t.route_id
             WHERE r.short_name = ? AND t.service_id IN (\(Self.holes(services.count)))
             -- By headsign and not by direction_id. The feed gives rail line 1
             -- one trip towards Lyon under each id, and two rows reading
             -- "toward Lyon" ask a person to choose between them.
             GROUP BY t.headsign
             ORDER BY COUNT(*) DESC, t.headsign
            """
        return try rows(
            sql,
            [.text(route)] + services.map { .text($0) }
        ) { Direction(headsign: $0.text(0), trips: $0.int(1)) }
    }

    /// The stops one direction of a route calls at, in the order it calls at
    /// them. That is not the order their names sort in, and a rider reading the
    /// list down is reading the way the bus goes.
    public func stops(of route: String, toward headsign: String, on date: String) throws -> [Stop] {
        let services = try activeServices(on: date)
        if services.isEmpty { return [] }

        // One trip's path, and not every trip's stops merged.
        //
        // Trips going the same way do not all call at the same stops: on route
        // 48 toward Hurdman some start at Carleton and some at Billings Bridge.
        // Ordering the union by each stop's earliest sequence number interleaves
        // those patterns and draws a route no bus takes. The longest trip is the
        // one that calls everywhere. The count ties often, so the id breaks it:
        // without that the order is the database's to choose.
        let sql = """
            SELECT s.stop_id, s.stop_code, COALESCE(p.name, s.name), COALESCE(s.platform, ''),
                   -- Whether a bus ever takes anyone on here, asked of every
                   -- trip on this route and direction and not only of the one
                   -- whose path is drawn: a stop one trip terminates at is a
                   -- stop another carries on past, and it is that second trip
                   -- that makes it boardable.
                   EXISTS (
                     SELECT 1
                       FROM stop_times b
                       JOIN trips bt ON bt.trip_id = b.trip_id
                       JOIN routes br ON br.route_id = bt.route_id
                      WHERE b.stop_id = s.stop_id AND b.boards = 1
                        AND br.short_name = ? AND bt.headsign = ?
                        AND bt.service_id IN (\(Self.holes(services.count))))
              FROM stop_times st
              JOIN stops s ON s.stop_id = st.stop_id
              -- The station a platform belongs to, when the feed names one.
              -- Its name is the one on the sign: TUNNEY'S PASTURE rather than
              -- TUNNEY'S PASTURE O-TRAIN EAST / EST, and BILLINGS BRIDGE rather
              -- than BILLINGS BRIDGE 4C. Every rail platform has one.
              LEFT JOIN stops p ON p.stop_id = s.parent
             WHERE st.trip_id = (
                   SELECT longest.trip_id
                     FROM stop_times longest
                     JOIN trips t ON t.trip_id = longest.trip_id
                     JOIN routes r ON r.route_id = t.route_id
                    WHERE r.short_name = ? AND t.headsign = ?
                      AND t.service_id IN (\(Self.holes(services.count)))
                    GROUP BY longest.trip_id
                    ORDER BY COUNT(*) DESC, longest.trip_id
                    LIMIT 1)
             GROUP BY s.stop_id
             -- Total: stop_id is the group, and one trip can call at a stop
             -- twice, which keeps the first of them.
             ORDER BY MIN(st.seq), s.stop_id
            """
        // Route and headsign twice over, once for each ? in the statement, in
        // the order they appear in it: the EXISTS is in the select list and so
        // binds before the subquery in the WHERE.
        let arguments = [Value.text(route), .text(headsign)] + services.map { Value.text($0) }
        return try rows(sql, arguments + arguments) {
            Stop(
                id: $0.text(0), code: $0.text(1), name: $0.text(2).english.titled,
                platform: $0.text(3), boards: $0.int(4) == 1)
        }
    }

    /// Every call at a stop on one service day, in order.
    ///
    /// Two days are read. A trip written 25:10 on Friday is the one a person
    /// catches at 01:10 on Saturday, so yesterday's late rows are shifted back
    /// by a day and both days sit on one axis. Everything downstream assumes it.
    public func departures(at stop: String, on day: String, after previous: String) throws
        -> [Departure]
    {
        let today = try calls(at: stop, on: day, shift: 0)
        // Only yesterday's rows that reach past midnight can still be caught.
        let borrowed = try calls(at: stop, on: previous, shift: 86400)

        // Stable, because `scheduled` is not unique: two routes can call in the
        // same second. Each day is already ordered totally, and today's rows
        // come before yesterday's at the same second because they are first.
        return (today + borrowed).enumerated()
            .sorted {
                $0.element.scheduled != $1.element.scheduled
                    ? $0.element.scheduled < $1.element.scheduled : $0.offset < $1.offset
            }
            .map(\.element)
    }

    /// The calls at a stop on one date, with `shift` subtracted from each. A
    /// shift of 86400 also drops everything that does not reach past midnight,
    /// because nothing earlier is still catchable the next day.
    private func calls(at stop: String, on date: String, shift: Int) throws -> [Departure] {
        let services = try activeServices(on: date)
        if services.isEmpty { return [] }

        let sql = """
            SELECT st.trip_id, r.short_name, t.headsign, st.arr, r.color
              FROM stop_times st
              JOIN trips t ON t.trip_id = st.trip_id
              JOIN routes r ON r.route_id = t.route_id
             WHERE st.stop_id = ? AND t.service_id IN (\(Self.holes(services.count)))
               AND st.arr >= ?
               -- A bus that only empties out here is not a departure. Three
               -- million calls in the feed and 75,249 of them are this, nearly
               -- all at a terminus: without the test, route 75's board at
               -- Tunney's Pasture counts a rider down to 893 buses a day that
               -- will not open a door for them.
               AND st.boards = 1
             -- Total on purpose, and the second key is the order the export
             -- wrote the rows in. Two routes can call in the same second and
             -- nothing in the data says which is drawn first, so the file's own
             -- order does. The sort that merges the two days is stable.
             ORDER BY st.arr, st.rowid
            """
        return try rows(
            sql,
            [.text(stop)] + services.map { .text($0) } + [.int(Int64(shift))]
        ) {
            Departure(
                trip: $0.text(0), route: $0.text(1), headsign: $0.text(2),
                scheduled: $0.int(3) - shift, afterMidnight: shift != 0, colour: $0.text(4))
        }
    }

    /// One stop by id, or nothing when the cache has never heard of it. A pin
    /// made before an update can name a stop the new feed dropped.
    public func stop(_ id: String) throws -> Stop? {
        let sql = """
            SELECT s.stop_id, s.stop_code, COALESCE(p.name, s.name), COALESCE(s.platform, '')
              FROM stops s
              LEFT JOIN stops p ON p.stop_id = s.parent
             WHERE s.stop_id = ? LIMIT 1
            """
        return try rows(sql, [.text(id)]) {
            Stop(
                id: $0.text(0), code: $0.text(1), name: $0.text(2).english.titled,
                platform: $0.text(3))
        }.first
    }

    /// What the ingest recorded about the feed it built this from.
    public func meta(_ key: String) throws -> String? {
        try rows("SELECT value FROM meta WHERE key = ?", [.text(key)]) {
            $0.text(0)
        }.first
    }

    /// Prepares `sql`, binds `values`, and builds a row from each result. Every
    /// query here has this shape, and a statement is finalised when it goes out
    /// of scope whether it ran or threw.
    private func rows<T>(_ sql: String, _ values: [Value] = [], _ read: (Statement) -> T) throws
        -> [T]
    {
        try db.prepare(sql).rows(values, read)
    }

    /// The placeholder list for an `IN` clause of `count` values.
    private static func holes(_ count: Int) -> String {
        Array(repeating: "?", count: count).joined(separator: ",")
    }

    /// Orders two route names the way a person reads them: by the number they
    /// start with, and then as text. A name starting with a letter comes after
    /// every name starting with a digit, because E1, N45 and R1 are a different
    /// kind of thing from 6 and 12.
    static func byNumber(_ a: String, _ b: String) -> Bool {
        let m = leadingNumber(a)
        let n = leadingNumber(b)
        return m == n ? a < b : m < n
    }

    private static func leadingNumber(_ s: String) -> Int {
        let digits = s.prefix { $0.isASCII && $0.isNumber }
        return Int(digits) ?? Int.max
    }

    /// The calendar column for a date written YYYYMMDD.
    static func weekdayColumn(_ date: String) throws -> String {
        guard date.count == 8, let y = Int(date.prefix(4)),
            let m = Int(date.dropFirst(4).prefix(2)), let d = Int(date.suffix(2))
        else {
            throw CacheError.badDate(date)
        }
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        // Noon, so no zone or transition can move the day this lands on. The
        // weekday of a calendar date is the same in every zone; the instant is
        // only a way to ask for it.
        guard
            let at = calendar.date(from: DateComponents(year: y, month: m, day: d, hour: 12))
        else {
            throw CacheError.badDate(date)
        }
        return ["sun", "mon", "tue", "wed", "thu", "fri", "sat"][
            calendar.component(.weekday, from: at) - 1]
    }
}
