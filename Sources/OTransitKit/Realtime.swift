// One fetch of the GTFS-Realtime trip updates, and what it said about a trip.
//
// The endpoint has `beta` in its URL, so its shape will change. A broken parse
// looks exactly like a quiet Sunday: no predictions, and every row reading
// sched. So parsing reports an error rather than returning an empty feed, and a
// caller that swallows that error has built the quiet Sunday itself.
//
// Nothing here reads the clock. Age arrives as an argument.

import Foundation

/// One trip at one stop, which is what a prediction is about. A trip calls at a
/// stop once, so the pair is the key.
private struct Call: Hashable {
    let trip: String
    let stop: String
}

public struct Realtime: Sendable {
    /// The relationship that takes a trip off the board. The other three mean
    /// it is running, including the two the schedule never had.
    private static let cancelledRelationship = 3
    /// How old a feed can be and still be counted in seconds.
    private static let secondsShown = 60 * 2

    /// When the feed says it was made, and whether it said at all. A feed that
    /// did not say is taken as current, because the alternative is reporting it
    /// as 1970.
    private let built: Int?
    private let off: Set<String>
    private let arrivals: [Call: Int]

    public enum Failure: Error, CustomStringConvertible {
        case unreadable(String)

        public var description: String {
            switch self {
            case .unreadable(let why): "reading the trip updates: \(why)"
            }
        }
    }

    /// Reads a feed.
    public init(json data: Data) throws {
        guard let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            throw Failure.unreadable("the body is not an object")
        }

        // A number the endpoint may omit. Read loosely: it has been a string.
        built = (root["Header"] as? [String: Any]).flatMap { Self.seconds($0["Timestamp"]) }

        var off: Set<String> = []
        var arrivals: [Call: Int] = [:]

        for entity in root["Entity"] as? [[String: Any]] ?? [] {
            guard let update = entity["TripUpdate"] as? [String: Any] else { continue }
            let trip = Self.text((update["Trip"] as? [String: Any])?["TripId"])
            // An update naming no trip is about nothing. Dropped rather than
            // stored under the empty id, which a departure with no trip would
            // then match.
            guard !trip.isEmpty else { continue }

            // Read the way every other field here is read. A strict `as? Int`
            // returns nil the day the endpoint writes "3" rather than 3, and a
            // nil relationship cancels nothing: the trip would keep its place
            // on the board with a countdown to a bus that is not coming.
            let relationship = Self.seconds(
                (update["Trip"] as? [String: Any])?["ScheduleRelationship"])
            if relationship == Self.cancelledRelationship { off.insert(trip) }

            for stop in update["StopTimeUpdate"] as? [[String: Any]] ?? [] {
                let stopID = Self.text(stop["StopId"])
                // Arrival first, then departure. A vehicle does not arrive at
                // the stop it starts from, so the first stop of every trip
                // carries a departure and no arrival — and a terminus is
                // exactly where someone stands waiting to be told.
                guard !stopID.isEmpty,
                    let at = Self.time(stop["Arrival"]) ?? Self.time(stop["Departure"])
                else { continue }
                arrivals[Call(trip: trip, stop: stopID)] = at
            }
        }

        self.off = off
        self.arrivals = arrivals
    }

    /// When the feed expects a trip at a stop, in seconds since 1970.
    public func arrival(trip: String, stop: String) -> Int? {
        arrivals[Call(trip: trip, stop: stop)]
    }

    /// Whether the feed says a trip is not running.
    public func isCancelled(_ trip: String) -> Bool { off.contains(trip) }

    /// How many predictions arrived. A feed that parsed but said nothing is a
    /// quiet Sunday; one that says nothing every time is a changed endpoint.
    public var count: Int { arrivals.count }

    /// What the status line says about a feed read at `now`.
    ///
    /// Seconds for the first two minutes, then whole minutes. A feed stamped
    /// ahead of the clock is not old: the endpoint's clock and ours are two
    /// clocks.
    public func note(now: Int) -> String {
        let age = built.map { max(0, now - $0) } ?? 0
        return age <= Self.secondsShown ? "live \(age)s" : "live \(age / 60)m old"
    }

    /// When one half of a stop time update says it has a time.
    ///
    /// No time is not a time: taking the zero would put the trip at the epoch,
    /// which draws as a departure fifty years gone.
    private static func time(_ half: Any?) -> Int? {
        guard let half = half as? [String: Any], truthy(half["HasTime"]) else { return nil }
        return seconds(half["Time"])
    }

    /// An id the feed writes as a number where the cache holds text. StopId
    /// arrives as 10248 and the cache has "10248"; compared as they arrive,
    /// nothing ever matches and every row reads sched.
    private static func text(_ value: Any?) -> String {
        switch value {
        case let s as String: s
        case let n as NSNumber: n.stringValue
        default: ""
        }
    }

    /// A flag the feed writes as 1 rather than true.
    private static func truthy(_ value: Any?) -> Bool {
        (value as? NSNumber)?.intValue ?? 0 != 0
    }

    /// A number the feed may write as a number or as a string.
    private static func seconds(_ value: Any?) -> Int? {
        switch value {
        case let n as Int: n
        case let n as Double: Int(n)
        case let s as String: Int(s)
        default: nil
        }
    }
}
