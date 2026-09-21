// What the cache answers, asked of a feed the test built.

import Testing

@testable import OTransitKit

@Suite("Cache")
struct CacheTests {
    @Test("a route that spans two booking periods is one route")
    func routesCollapse() async throws {
        let routes = try await Fixture.cache().routes(.bus, on: Fixture.monday)
        #expect(routes.filter { $0.shortName == "44" }.count == 1)
    }

    @Test("routes come back in the order a person reads route numbers")
    func routesOrder() async throws {
        let routes = try await Fixture.cache().routes(.bus, on: Fixture.monday)
        // 10 after 2 and not between 1 and 2, and a name starting with a letter
        // after every name starting with a digit.
        #expect(routes.map(\.shortName) == ["2", "10", "44", "N45"])
    }

    @Test("a service whose window has closed does not run today")
    func withdrawnRoute() async throws {
        let routes = try await Fixture.cache().routes(.bus, on: Fixture.monday)
        #expect(!routes.map(\.shortName).contains("99"))
    }

    @Test("rail and bus are different lists")
    func modes() async throws {
        let rail = try await Fixture.cache().routes(.rail, on: Fixture.monday)
        #expect(rail.map(\.shortName) == ["1"])
    }

    @Test("a route runs on a Monday and not on the Sunday before")
    func weekdayOnly() async throws {
        let sunday = try await Fixture.cache().routes(.bus, on: Fixture.sunday)
        #expect(sunday.map(\.shortName) == ["N45"])
    }

    @Test("directions are named by headsign, busiest first")
    func directions() async throws {
        let directions = try await Fixture.cache().directions(of: "44", on: Fixture.monday)
        #expect(directions.map(\.headsign) == ["Kanata", "Hurdman"])
        // Three of them, counted across both booking periods.
        #expect(directions.first?.trips == 3)
    }

    @Test("a direction's stops are one trip's path, in the order it calls")
    func routeStops() async throws {
        let stops = try await Fixture.cache().stops(of: "2", toward: "Bayshore", on: Fixture.monday)
        // The longest trip, which is the one that calls everywhere. The shorter
        // pattern skips Alta Vista, and merging the two would draw Carleton,
        // Alta Vista, Billings Bridge as a route no bus takes in that order.
        #expect(stops.map(\.name) == ["Carleton", "Alta Vista / Rolland", "Billings Bridge"])
    }

    @Test("a platform is listed under its station's name, not the feed's own")
    func stationName() async throws {
        let stops = try await Fixture.cache().stops(of: "2", toward: "Bayshore", on: Fixture.monday)
        let carleton = try #require(stops.first)
        // The feed calls it CARLETON O-TRAIN SOUTH / SUD, in capitals. The
        // suffix is the same on every station of a line read in one direction,
        // so it is the only part of the row a person cannot use; the capitals
        // are the stops table shouting, which the headsigns in the same feed
        // do not do.
        #expect(carleton.name == "Carleton")
        // What the suffix was saying survives, on the plate at the end of the
        // row rather than in the line under it.
        #expect(carleton.platform == "A")
        #expect(carleton.codeLine == "#1111")
    }

    @Test("a stop with no station keeps its own name and says only its code")
    func unparented() async throws {
        let stop = try #require(try await Fixture.cache().stop("1003"))
        #expect(stop.name == "Rideau / Augusta")
        #expect(stop.codeLine == "#2331")
    }

    @Test("a board carries yesterday's late trip, shifted onto today's axis")
    func afterMidnight() async throws {
        let board = try await Fixture.cache()
            .departures(at: "1003", on: Fixture.monday, after: Fixture.sunday)

        // Sunday's 25:10 is Monday's 01:10, and it sorts before Monday's 07:10.
        let first = try #require(board.first)
        #expect(first.scheduled == 4200)
        #expect(first.afterMidnight)
        // Route 44 toward Kanata ends at this stop, so its three calls are
        // buses emptying out and none of them is on the board. What is left is
        // the one trip that starts here, the two night trips, and the train.
        #expect(board.map(\.scheduled) == [4200, 26400, 36000, 90000])
    }

    @Test("today's own late trip keeps its own time")
    func pastMidnightToday() async throws {
        let board = try await Fixture.cache()
            .departures(at: "1003", on: Fixture.monday, after: Fixture.sunday)
        let night = try #require(board.last)
        #expect(night.scheduled == 90000)
        #expect(!night.afterMidnight)
    }

    @Test("a stop resolves by id, and an unknown one is absent rather than empty")
    func stopByID() async throws {
        let cache = try Fixture.cache()
        #expect(try await cache.stop("1003")?.code == "2331")
        // Resolved by id as well as in a list, so a pin on a platform names the
        // station the same way the list that made it did.
        #expect(try await cache.stop("1004")?.name == "Carleton")
        #expect(try await cache.stop("nope") == nil)
    }

    @Test("an added service runs on a day its calendar excludes")
    func addedException() async throws {
        // 28 September is a Monday. SUN's weekly calendar excludes it and
        // calendar_dates adds it back.
        let services = try await Fixture.cache().activeServices(on: "20260928")
        #expect(services.contains("SUN"))
    }

    @Test("a removed service does not run on a day its calendar includes")
    func removedException() async throws {
        let services = try await Fixture.cache().activeServices(on: "20260921")
        #expect(!services.contains("WEEK"))
    }

    @Test("a date is read as a weekday, and a date that is not one is refused")
    func weekdays() throws {
        #expect(try Cache.weekdayColumn("20260914") == "mon")
        #expect(try Cache.weekdayColumn("20260913") == "sun")
        #expect(throws: CacheError.self) { try Cache.weekdayColumn("2026-09-14") }
    }
}

@Suite("Boarding")
struct BoardingTests {
    @Test("a bus that only empties out here is not a departure")
    func terminalCallsAreNotDepartures() async throws {
        let board = try await Fixture.cache()
            .departures(at: "1003", on: Fixture.monday, after: Fixture.sunday)
        // Route 44 toward Kanata calls here three times and ends here all
        // three. Counting a rider down to a bus that will not open its door is
        // the one thing on this screen that would be untrue rather than merely
        // unhelpful.
        #expect(!board.contains { $0.route == "44" && $0.headsign == "Kanata" })
        // The same route the other way starts here, and that is a departure.
        #expect(board.contains { $0.route == "44" && $0.headsign == "Hurdman" })
    }

    @Test("a stop is offered only where something can be caught")
    func stopsSayWhetherTheyBoard() async throws {
        let stops = try await Fixture.cache().stops(of: "2", toward: "Bayshore", on: Fixture.monday)
        // Every trip on this route and direction ends at Billings Bridge, so
        // the route does go there and nothing there can be boarded.
        let last = try #require(stops.last)
        #expect(last.name == "Billings Bridge")
        #expect(!last.boards)
        let earlier = stops.dropLast().allSatisfy(\.boards)
        #expect(earlier)
    }

    @Test("boarding is asked of the direction, not of the stop")
    func boardingIsPerDirection() async throws {
        let cache = try Fixture.cache()
        // Stop 1003 is where route 44 toward Kanata ends and where the same
        // route toward Hurdman begins. One of those is a door and one is not,
        // and nothing about the stop itself says which.
        let kanata = try await cache.stops(of: "44", toward: "Kanata", on: Fixture.monday)
        #expect(kanata.last?.id == "1003")
        #expect(kanata.last?.boards == false)

        let hurdman = try await cache.stops(of: "44", toward: "Hurdman", on: Fixture.monday)
        #expect(hurdman.first?.id == "1003")
        #expect(hurdman.first?.boards == true)
    }

    @Test("a cache this version cannot read is refused rather than misread")
    func schemaIsChecked() {
        // Built fresh each time rather than shared: an actor may not be handed
        // a database another closure still holds.
        func open(stamped: String?) -> CacheError? {
            do {
                let db = try Database.inMemory()
                try db.execute(Schema.tables)
                if let stamped {
                    try db.prepare("INSERT INTO meta VALUES (?,?)")
                        .run([.text("schema"), .text(stamped)])
                }
                _ = try Cache(database: db)
                return nil
            } catch let error as CacheError {
                return error
            } catch {
                return nil
            }
        }

        // No stamp is what every cache built before the stamp existed looks
        // like. It must not open: its columns are a different shape, and a
        // query reading fewer of them comes back looking like a quiet Sunday
        // rather than like a fault.
        #expect(open(stamped: nil) == .outdated(found: nil))
        #expect(open(stamped: "1") == .outdated(found: "1"))
        #expect(open(stamped: Schema.version) == nil)
    }

}
