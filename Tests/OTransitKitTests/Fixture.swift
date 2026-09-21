// A feed a test builds, in memory, row by row.
//
// Every test that asks the cache a question builds this. Nothing here reads the
// file the app downloaded: a test that did would pass or fail on what the city
// published this morning, and would take ten seconds to find out.
//
// The rows are few and chosen. Each is here because some query has a reason to
// get it wrong: a route that repeats across booking periods, a service whose
// window has closed, two patterns of stop under one headsign, and a trip that
// runs past midnight and belongs on the next day's board.

import Foundation

@testable import OTransitKit

enum Fixture {
    static let monday = "20260914"
    static let sunday = "20260913"

    static func cache() throws -> Cache {
        let db = try Database.inMemory()
        try db.execute(Schema.tables)

        // Route 44 arrives as two route_ids, the way an export writes a route
        // that spans two booking periods. 2 and 10 are here so the order a
        // person reads can be told from the order text sorts in, and N45 so a
        // letter can be told from a digit. 99 runs only on a service whose
        // window has closed.
        try insert(
            db, "routes", 7,
            [
                ["44", "44", "Kanata", "3", "d62839", "", "0"],
                ["44-1", "44", "Kanata", "3", "d62839", "", "0"],
                ["2", "2", "Bayshore", "3", "0075c9", "", "0"],
                ["10", "10", "Hurdman", "3", "0075c9", "", "0"],
                ["N45", "N45", "Night", "3", "333333", "", "0"],
                ["99", "99", "Withdrawn", "3", "888888", "", "0"],
                ["L1", "1", "Confederation Line", "0", "d62839", "", "0"],
            ])

        try insert(
            db, "calendar", 10,
            [
                ["WEEK", "1", "1", "1", "1", "1", "0", "0", "20260101", "20261231"],
                ["SUN", "0", "0", "0", "0", "0", "0", "1", "20260101", "20261231"],
                // Runs on weekdays and stopped last year. The window is the only
                // thing excluding it, and a query that forgets the window puts
                // route 99 on every Monday.
                ["OLD", "1", "1", "1", "1", "1", "0", "0", "20250101", "20251231"],
            ])

        try insert(
            db, "calendar_dates", 3,
            [
                // A Monday the weekday service does not run, and a Monday the
                // Sunday service does.
                ["WEEK", "20260921", "2"],
                ["SUN", "20260928", "1"],
            ])

        try insert(
            db, "stops", 8,
            [
                ["1001", "3034", "BILLINGS BRIDGE", "45.38", "-75.67", "0", "", ""],
                ["1002", "8626", "ALTA VISTA / ROLLAND", "45.39", "-75.66", "0", "", ""],
                ["1003", "2331", "RIDEAU / AUGUSTA", "45.42", "-75.68", "0", "", ""],
                // A platform and the station it belongs to, the way the feed
                // writes a station: the platform repeats the station's name and
                // then says which side of it this is, and the station itself is
                // a location_type 1 row carrying the name on the sign.
                [
                    "1004", "1111", "CARLETON O-TRAIN SOUTH / SUD", "45.38", "-75.69", "0",
                    "1004_stn", "A",
                ],
                ["1004_stn", "1111", "CARLETON", "45.38", "-75.69", "1", "", ""],
            ])

        // Route 44 toward Kanata: three trips, the last on the second booking
        // period's route_id. Toward Hurdman: one. So the directions screen has a
        // busiest and a tie to break.
        try trip(
            db, "t1", "44", "WEEK", "Kanata", [("1001", 25200), ("1002", 25500), ("1003", 25800)])
        try trip(
            db, "t2", "44", "WEEK", "Kanata", [("1001", 28800), ("1002", 29100), ("1003", 29400)])
        try trip(
            db, "t3", "44-1", "WEEK", "Kanata", [("1001", 32400), ("1002", 32700), ("1003", 33000)])
        try trip(db, "t4", "44", "WEEK", "Hurdman", [("1003", 36000), ("1001", 36300)])

        // Route 2 toward Bayshore under two patterns: one skips Alta Vista. The
        // stops screen must draw the longer, because the union of the two draws
        // a route no bus takes.
        try trip(db, "t5", "2", "WEEK", "Bayshore", [("1004", 30000), ("1001", 30300)])
        try trip(
            db, "t6", "2", "WEEK", "Bayshore", [("1004", 31000), ("1002", 31200), ("1001", 31500)])

        try trip(db, "t7", "10", "WEEK", "Hurdman", [("1001", 27000)])
        // 25:00 on Monday, which is Tuesday morning and stays on Monday's board.
        try trip(db, "t8", "N45", "WEEK", "Night", [("1003", 90000)])
        // 25:10 on Sunday, which is the row a person catches at 01:10 Monday.
        try trip(db, "t9", "N45", "SUN", "Night", [("1003", 90600)])
        try trip(db, "t10", "99", "OLD", "Withdrawn", [("1001", 26000)])
        try trip(db, "t11", "L1", "WEEK", "Blair", [("1003", 26400)])

        try db.prepare("INSERT INTO meta VALUES (?,?)")
            .run([.text("schema"), .text(Schema.version)])
        return try Cache(database: db)
    }

    /// A trip and its calls. The last call of a trip takes nobody on, which is
    /// what the feed says of a terminus and what a board has to leave out.
    private static func trip(
        _ db: Database, _ id: String, _ route: String, _ service: String, _ headsign: String,
        _ calls: [(String, Int)]
    ) throws {
        try db.prepare("INSERT INTO trips VALUES (?,?,?,?,?)")
            .run([.text(id), .text(route), .text(service), .text(headsign), .int(0)])
        let call = try db.prepare("INSERT INTO stop_times VALUES (?,?,?,?,?,?)")
        for (seq, at) in calls.enumerated() {
            try call.run([
                .text(id), .text(at.0), .int(Int64(seq)), .int(Int64(at.1)), .int(Int64(at.1)),
                // A trip written with one call is shorthand for "a bus calls
                // here", not for one that ends here.
                .int(seq == calls.count - 1 && calls.count > 1 ? 0 : 1),
            ])
        }
    }

    /// Inserts rows of text. The columns have TEXT affinity where it matters, so
    /// binding every field as text is what the export itself holds: CSV has no
    /// types, and the ingest is what decides which fields are numbers.
    private static func insert(_ db: Database, _ table: String, _ columns: Int, _ rows: [[String]])
        throws
    {
        let holes = Array(repeating: "?", count: columns).joined(separator: ",")
        let statement = try db.prepare("INSERT INTO \(table) VALUES (\(holes))")
        for row in rows { try statement.run(row.map { Value.text($0) }) }
    }
}
