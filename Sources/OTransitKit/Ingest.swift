// Turning the published export into the database every query reads.
//
// Six files, six million rows, one transaction. The indexes are built at the
// end rather than maintained across the inserts, which is the difference
// between ten seconds and several minutes.
//
// Every column is read by name. GTFS does not fix column order and two exports
// of one feed have disagreed about it, so reading by position is the bug that
// puts a latitude in a stop's name six months after anyone last looked.

import Foundation

public enum Ingest {
    /// A time written HH:MM:SS on the service day, in seconds.
    ///
    /// The hour reaches past 24 and is meant to: 25:10 is ten past one the next
    /// morning, on the service day that began the morning before. Anything that
    /// is not three numbers is nothing, and the row that held it is skipped.
    static func seconds(_ written: String) -> Int? {
        let parts = written.split(separator: ":")
        guard parts.count == 3, let h = Int(parts[0]), let m = Int(parts[1]), let s = Int(parts[2])
        else { return nil }
        return h * 3600 + m * 60 + s
    }

    /// Builds the whole cache from an archive.
    public static func build(
        from zip: Zip, into db: Database, progress: (Building) -> Void
    ) throws {
        // Durability is worth nothing here: this is built beside the live cache
        // and moved over it at the end, so a machine that loses power loses a
        // file nothing had started reading.
        try db.execute(
            """
            PRAGMA journal_mode = OFF;
            PRAGMA synchronous = OFF;
            PRAGMA temp_store = MEMORY;
            PRAGMA cache_size = -64000;
            """)
        try db.execute(Schema.tables)

        try db.transaction {
            let routes = try db.prepare("INSERT OR REPLACE INTO routes VALUES (?,?,?,?,?,?,?)")
            try read(zip, "routes.txt", progress) { header, row in
                try routes.run([
                    .text(header.string(row, "route_id")),
                    .text(header.string(row, "route_short_name")),
                    .text(header.string(row, "route_long_name")),
                    .int(Int64(header.int(row, "route_type") ?? 3)),
                    .text(header.string(row, "route_color")),
                    .text(header.string(row, "route_text_color")),
                    .int(Int64(header.int(row, "route_sort_order") ?? 0)),
                ])
            }

            let trips = try db.prepare("INSERT OR REPLACE INTO trips VALUES (?,?,?,?,?)")
            try read(zip, "trips.txt", progress) { header, row in
                try trips.run([
                    .text(header.string(row, "trip_id")),
                    .text(header.string(row, "route_id")),
                    .text(header.string(row, "service_id")),
                    .text(header.string(row, "trip_headsign")),
                    .int(Int64(header.int(row, "direction_id") ?? 0)),
                ])
            }

            let stops = try db.prepare("INSERT OR REPLACE INTO stops VALUES (?,?,?,?,?,?,?,?)")
            try read(zip, "stops.txt", progress) { header, row in
                let platform = header.string(row, "platform_code")
                try stops.run([
                    .text(header.string(row, "stop_id")),
                    .text(header.string(row, "stop_code")),
                    .text(header.string(row, "stop_name")),
                    .real(Double(header.string(row, "stop_lat")) ?? 0),
                    .real(Double(header.string(row, "stop_lon")) ?? 0),
                    .int(Int64(header.int(row, "location_type") ?? 0)),
                    .text(header.string(row, "parent_station")),
                    // Null and not "", so a query can tell a stop with no
                    // platform from one the export left blank.
                    platform.isEmpty ? .null : .text(platform),
                ])
            }

            let calls = try db.prepare("INSERT INTO stop_times VALUES (?,?,?,?,?,?)")
            try read(zip, "stop_times.txt", progress) { header, row in
                // A call with no arrival time is one nothing can be caught at.
                // The feed writes these for timepoint-less rows on some routes,
                // and a board built from them shows a departure at 00:00.
                guard let arrival = seconds(header.string(row, "arrival_time")) else { return }
                let departure = seconds(header.string(row, "departure_time")) ?? arrival
                // GTFS: 0 is a regular pickup, 1 is none, 2 and 3 are "phone
                // ahead" and "ask the driver". Anything that is not a flat no
                // is a bus a person can get on, so only 1 is excluded.
                let pickup = header.int(row, "pickup_type") ?? 0
                try calls.run([
                    .text(header.string(row, "trip_id")),
                    .text(header.string(row, "stop_id")),
                    .int(Int64(header.int(row, "stop_sequence") ?? 0)),
                    .int(Int64(arrival)),
                    .int(Int64(departure)),
                    .int(pickup == 1 ? 0 : 1),
                ])
            }

            let calendar = try db.prepare(
                "INSERT OR REPLACE INTO calendar VALUES (?,?,?,?,?,?,?,?,?,?)")
            try read(zip, "calendar.txt", progress) { header, row in
                try calendar.run(
                    [.text(header.string(row, "service_id"))]
                        + [
                            "monday", "tuesday", "wednesday", "thursday", "friday", "saturday",
                            "sunday",
                        ]
                        .map { .int(Int64(header.int(row, $0) ?? 0)) }
                        + [
                            .text(header.string(row, "start_date")),
                            .text(header.string(row, "end_date")),
                        ])
            }

            let dates = try db.prepare("INSERT INTO calendar_dates VALUES (?,?,?)")
            try read(zip, "calendar_dates.txt", progress) { header, row in
                try dates.run([
                    .text(header.string(row, "service_id")),
                    .text(header.string(row, "date")),
                    .int(Int64(header.int(row, "exception_type") ?? 1)),
                ])
            }

            // What this was built from, so a later run can tell whether the
            // published feed has moved on without downloading it again.
            let meta = try db.prepare("INSERT OR REPLACE INTO meta VALUES (?,?)")
            try meta.run([.text("built"), .text(ISO8601DateFormatter().string(from: .now))])
            try meta.run([.text("schema"), .text(Schema.version)])
        }

        progress(.indexing)
        try db.execute(Schema.indexes)
        // The planner chooses between five indexes on six million rows. Without
        // this it chooses by rule of thumb, and the departures query has picked
        // the wrong one of them before.
        try db.execute("ANALYZE")
    }

    /// Reads one file of the archive, handing each row to `row`, and reports
    /// how far along it is as it goes.
    ///
    /// The count is reported every 250,000 rows rather than every row: at six
    /// million rows a hop to the main actor per row costs more than the insert
    /// it is reporting on.
    static func read(
        _ zip: Zip, _ name: String, _ progress: (Building) -> Void,
        _ row: (Header, [String]) throws -> Void
    ) throws {
        progress(.reading(name, rows: 0))

        // The parser holds its callback across feeds, so it needs an escaping
        // one, and these two do not outlive this call. This is what that pair
        // of facts is spelled as.
        var count = 0
        try withoutActuallyEscaping(row) { row in
            try withoutActuallyEscaping(progress) { progress in
                var header: Header?
                var failure: Error?

                let parser = CSVParser { fields in
                    if failure != nil { return }
                    guard let known = header else {
                        header = Header(fields)
                        return
                    }
                    do {
                        try row(known, fields)
                        count += 1
                        if count % 250_000 == 0 { progress(.reading(name, rows: count)) }
                    } catch {
                        failure = error
                    }
                }

                try zip.inflate(try zip.entry(named: name)) { piece in
                    // A row that failed stops the file, but the decompressor is
                    // already running and its remaining pieces are dropped.
                    if failure == nil { parser.feed(piece) }
                }
                parser.finish()
                if let failure { throw failure }
            }
        }

        progress(.reading(name, rows: count))
    }
}
