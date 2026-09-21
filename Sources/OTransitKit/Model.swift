// What the app is about: a network, a route, a stop, a direction, a call.
//
// Plain values with no behaviour and no opinion about where they came from. A
// query builds them, a screen draws them, and neither needs the other's file
// open to know what one is. They lived in Cache.swift, which meant a reader
// asking what a Stop is had to know to look in the file named for the database.

import Foundation

/// The kind of vehicle a screen is about.
public enum Mode: Sendable, CaseIterable {
    // Rail first, which is the order OC Transpo lists its own networks in:
    // three lines, then two hundred routes.
    case rail
    case bus

    /// The GTFS `route_type` values the mode covers. Bus is 3. Rail covers
    /// light rail, underground and heavy rail, because the O-Train is 0 today
    /// and the feed is free to describe a line as any of them.
    var types: [Int] {
        switch self {
        case .bus: [3]
        case .rail: [0, 1, 2]
        }
    }

    // What a mode is called, and the words it uses for its parts, live in
    // Screen.swift: they are one vocabulary and they change together.
}

/// One route, already collapsed across its booking periods.
public struct Route: Sendable, Identifiable, Equatable {
    public var id: String { shortName }
    public let shortName: String
    public let longName: String

    public init(shortName: String, longName: String, colour: String) {
        self.shortName = shortName
        self.longName = longName
        self.colour = colour
    }
    /// The route's own colour from the feed, without the leading hash. A badge
    /// is drawn on it in black or white: the feed's `route_text_color` is not
    /// read, because it is not legible on its own background often enough to be
    /// trusted.
    public let colour: String
}

/// One stop.
public struct Stop: Sendable, Identifiable, Equatable {
    public let id: String
    public let code: String

    public init(
        id: String, code: String, name: String, platform: String, boards: Bool = true
    ) {
        self.id = id
        self.code = code
        self.name = name
        self.platform = platform
        self.boards = boards
    }
    /// What the stop is called on the sign: the station's name where the feed
    /// gives the platform a parent, and the stop's own name where it does not.
    ///
    /// The feed's own name for a platform repeats the station and then names the
    /// platform — TUNNEY'S PASTURE O-TRAIN EAST / EST, BILLINGS BRIDGE 4C — so a
    /// list of them is one word thirteen times followed by the only part that
    /// differs. The part that differs is `platform`, and it is shown there.
    public let name: String
    public let platform: String
    /// Whether anything on this route and direction takes a rider on here.
    ///
    /// False at a terminus, where every call is a bus emptying out. The stop is
    /// still on the route and still worth showing — it is where the route goes
    /// — but there is no board to draw for it, because nothing there can be
    /// caught.
    public let boards: Bool
}

/// One direction of a route, named by where it is going and not by the feed's
/// `direction_id`, because the two disagree: one headsign can appear under both.
public struct Direction: Sendable, Identifiable, Equatable {
    public var id: String { headsign }
    public let headsign: String
    public let trips: Int
}

/// One scheduled call at a stop.
public struct Departure: Sendable, Identifiable, Equatable {
    /// The id a realtime feed names a prediction by. It is the only thing that
    /// joins a row of the schedule to a row of a feed, and it is the identity
    /// here because two routes can be scheduled to the same second.
    public let trip: String
    public var id: String { trip }
    public let route: String
    public let headsign: String
    /// Seconds on the service day the board is drawn for. It can exceed 86400,
    /// because a service day reaches 28:xx, and a row borrowed from yesterday
    /// is shifted onto this axis rather than carrying its own.
    public let scheduled: Int
    /// Marks a row that came from yesterday's service day.
    public let afterMidnight: Bool
    public let colour: String
}

/// What a query can complain about that SQLite has no opinion on.
public enum CacheError: Error, CustomStringConvertible, Equatable {
    case badDate(String)
    /// The file was built by a version of this program that wrote different
    /// columns. Nil where it predates the stamp itself.
    ///
    /// There is no migration. The file is a cache, its source is a download the
    /// city publishes, and rebuilding it takes about ten seconds — carrying
    /// migrations for something that can be thrown away and made again is work
    /// with nobody to do it for.
    case outdated(found: String?)

    public var description: String {
        switch self {
        case .badDate(let d): "\(d) is not a date written YYYYMMDD"
        case .outdated(let found):
            "the cache is schema \(found ?? "unstamped"), this version reads \(Schema.version)"
        }
    }
}
