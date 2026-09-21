// The board: a stop's scheduled calls, with what the feed says about each.
//
// The merge lives here rather than in the view because it is the one place the
// two clocks meet. A schedule is written in seconds on a service day and a
// prediction in seconds since 1970, and every bug in this area is one of those
// being read as the other.

import Foundation

/// Where a row's time came from, and how far off the timetable it is.
public enum Status: Sendable, Equatable {
    case cancelled
    case onTime
    case late(Int)
    case early(Int)
    /// No prediction: the feed never named this trip, or there is no feed.
    case scheduled

    /// What the row says.
    public var text: String {
        switch self {
        case .cancelled: "cancelled"
        case .onTime: "on time"
        // No number. The board shows the promised time struck out beside the
        // expected one, so the difference between them is on the row already
        // and saying it again in minutes is the same fact twice. The minutes
        // are still carried on the case — they decide the colour.
        case .late: "late"
        case .early: "early"
        case .scheduled: "scheduled"
        }
    }

    /// Whether this row's time is a prediction. A predicted time is the best
    /// answer the program has and reads plainly; a scheduled one is dim.
    public var isPredicted: Bool {
        switch self {
        case .onTime, .late, .early: true
        case .cancelled, .scheduled: false
        }
    }
}

/// One row of the board.
public struct Arrival: Sendable, Equatable, Identifiable {
    public let trip: String
    public let route: String
    public let headsign: String
    public let colour: String
    /// Seconds on the service day: the prediction where there is one, and the
    /// timetable otherwise. This is the time drawn, and the wait counts to it.
    public let at: Int
    /// What the timetable promised, kept beside what is now expected so the
    /// board can show both rather than the difference between them.
    public let scheduled: Int
    public let status: Status

    public var id: String { trip }
}

public enum Board {
    /// Within a minute either way is on time.
    ///
    /// The feed reports to the second and a bus is not late by fifteen of them.
    private static let slack = 1

    /// The rows for a stop, in the order they will arrive.
    ///
    /// Sorted after merging, not before: a prediction can move a trip past the
    /// one the timetable put in front of it, and a board that kept the
    /// scheduled order would show the later bus first.
    ///
    /// Ties keep the order they arrived in, which is the order
    /// `Cache.departures` went to the trouble of making total. Swift's sort
    /// promises nothing about equal elements, so two calls in the same second
    /// would otherwise be free to swap places between one poll and the next,
    /// and a board that reorders itself with no new data looks broken.
    public static func rows(
        from departures: [Departure], live: Realtime?, stop: String, clock: Clock
    ) -> [Arrival] {
        departures
            .map { row($0, live: live, stop: stop, clock: clock) }
            .enumerated()
            .sorted {
                $0.element.at != $1.element.at
                    ? $0.element.at < $1.element.at : $0.offset < $1.offset
            }
            .map(\.element)
    }

    private static func row(
        _ departure: Departure, live: Realtime?, stop: String, clock: Clock
    ) -> Arrival {
        func build(at: Int, _ status: Status) -> Arrival {
            Arrival(
                trip: departure.trip, route: departure.route, headsign: departure.headsign,
                colour: departure.colour, at: at, scheduled: departure.scheduled, status: status)
        }

        guard let live else { return build(at: departure.scheduled, .scheduled) }
        if live.isCancelled(departure.trip) {
            // A cancelled trip keeps its scheduled time. There is nothing else
            // to put there, and the word is what says it will not come.
            return build(at: departure.scheduled, .cancelled)
        }
        guard let epoch = live.arrival(trip: departure.trip, stop: stop) else {
            return build(at: departure.scheduled, .scheduled)
        }

        // The one line where the two clocks meet.
        let predicted = clock.second(of: epoch)
        let off = Format.minutes(predicted - departure.scheduled)
        let status: Status =
            if off > slack {
                .late(off)
            } else if off < -slack {
                .early(-off)
            } else {
                .onTime
            }
        return build(at: predicted, status)
    }
}
