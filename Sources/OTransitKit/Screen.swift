// What each screen is called, and what it asks.
//
// Two names per screen, because they do different jobs. The name is where you
// are — a noun, in the top bar, answering a glance. The prompt is what to do
// here — a question, in the bottom bar, answering what the noun leaves out.
// "Stops" and "Which stop?" are not a repetition: the first says which screen
// this is, and the second is the sentence a person is in the middle of.
//
// Both live here rather than in a view. They are decisions about transit and
// about the words riders use, so a test can hold them; a view picks the font.

/// The words a mode uses for its own parts. A bus has routes and stops; rail
/// has lines and stations. Nothing else about the two screens differs, so this
/// is the whole of the difference.
extension Mode {
    /// The name of the mode itself, as the first screen offers it.
    public var title: String {
        switch self {
        case .bus: "Bus"
        case .rail: "O-Train"
        }
    }

    var routesPrompt: String {
        switch self {
        case .bus: "Which route?"
        case .rail: "Which line?"
        }
    }

    var stopsPrompt: String {
        switch self {
        case .bus: "Which stop?"
        case .rail: "Which station?"
        }
    }

    /// The mark the operator's own vehicle wears, which is what the first
    /// screen draws beside each name. Carried here rather than chosen at that
    /// screen, because the trail draws it too and the two must be the same mark.
    public var symbol: String {
        switch self {
        case .bus: "bus.fill"
        case .rail: "tram.fill"
        }
    }
}

/// One screen of the program, and the two things it is called.
public enum Screen: Sendable, Equatable {
    case transit
    case routes(Mode)
    case direction(Mode)
    case stops(Mode)
    case board(Mode)
    /// Not a place in the transit hierarchy — `Place` has no case for it, and
    /// should not: it is not reached by answering a question about a journey.
    /// It is a screen, so it is named here like the rest.
    case settings

    /// The name in the top bar: where you are.
    /// What this screen is asking, drawn as its title.
    ///
    /// The screen used to carry a noun at the top and this at the bottom, and
    /// on five of the seven they were the same word: "Stations" over "Which
    /// station?". The noun went, because the question already contains it and
    /// the bar it sat in was a quarter of the room the trail needed.
    ///
    /// The board's is not a question. You have finished asking by the time you
    /// reach it, and heading a list of times with "Which departure?" would
    /// invite a choice that screen does not offer.
    public var prompt: String {
        switch self {
        case .transit: "What are you taking?"
        case .routes(let mode): mode.routesPrompt
        case .direction: "Which way?"
        case .stops(let mode): mode.stopsPrompt
        case .board: "Departures"
        case .settings: "Settings"
        }
    }

    /// Whether there is anywhere behind this screen to go back to.
    public var hasBack: Bool {
        if case .transit = self { return false }
        return true
    }
}

extension Route {
    /// The two ends of a route, when the feed names both.
    ///
    /// OC Transpo writes a route that runs between two places as "Blair <>
    /// Tunney's Pasture" — 169 of the 175 routes running today. The six that do
    /// not are loops and shuttles named for the one place they serve, and they
    /// have no ends to split.
    ///
    /// The angle brackets are the feed's way of drawing an arrow in a CSV
    /// field, and a screen that can draw a real one should. Splitting is here
    /// rather than in the view because it is a fact about the feed's format,
    /// and the day it writes something else this is the one place that is wrong.
    public var ends: (from: String, to: String)? {
        // The arrow is found first and the halves shortened after, never the
        // other way round: route 105 is "Airport ~ Aéroport <> Hurdman /
        // St-Laurent & N Rideau", and shortening the whole string first would
        // take the arrow away with the French and leave a route with one end.
        let parts = longName.components(separatedBy: "<>")
        guard parts.count == 2 else { return nil }
        let from = parts[0].english
        let to = parts[1].english
        guard !from.isEmpty, !to.isEmpty else { return nil }
        return (from, to)
    }

    /// The route's name as a screen shows it, for the six routes that have no
    /// ends to split — loops and shuttles named for the one place they serve.
    /// One of the six is route 615, `Parliament ~ Parlement`, so the name as a
    /// whole needs shortening and not only the halves of a name that has them.
    /// The route's name in words, for anything with room for all of it — a
    /// tooltip over a row too narrow to show it, or a screen reader. 29 of the
    /// 158 routes with two ends do not fit the 226 points a row gives them.
    public var spoken: String {
        guard let ends else { return title }
        return "\(ends.from) to \(ends.to)"
    }

    public var title: String {
        longName.components(separatedBy: "<>").map(\.english).joined(separator: " <> ")
    }
}

extension Direction {
    /// Where this direction is going, as a screen shows it. `headsign` itself
    /// stays as the feed wrote it, because it is what the stop query and the
    /// board filter match on: shortening the key would match nothing.
    public var name: String { headsign.english }
}

extension Stop {
    /// The code printed on the pole, which is what a person standing at one
    /// reads back to check they are at the right one.
    ///
    /// The platform is not here. It used to be — `#3062 · Platform A` — which
    /// gave the thing you act on the same weight as the thing you verify with.
    /// It is a plate on the row now, and the code is left to be a code.
    ///
    /// Nil rather than empty when the feed has no code, so a row with nothing
    /// to add is one line instead of two.
    public var codeLine: String? {
        code.isEmpty ? nil : "#\(code)"
    }

    /// The whole of what this stop is, for a pointer resting on a row too
    /// narrow to show all of it.
    ///
    /// The row itself gives its margin to one thing: the code where you can
    /// board, and where you cannot, the reason. The reason wins there because
    /// the code answers "am I at the right pole" and nobody is going to stand
    /// at that one — but `boards` is a fact about this route and this
    /// direction, not about the place. Blair takes plenty of buses; it just
    /// does not take this one. So the code is not wrong to want, and this is
    /// where it went.
    public var spoken: String {
        [
            name,
            codeLine,
            platform.isEmpty ? nil : "Platform \(platform)",
            boards ? nil : "drop-off only",
        ]
        .compactMap { $0 }
        .joined(separator: " · ")
    }
}

extension Arrival {
    /// What the timetable promised, when the bus is not keeping to it.
    ///
    /// Read off the status rather than off the two times. Comparing the drawn
    /// minutes looked equivalent and is not: on time means within a minute
    /// either way, and a bus scheduled 11:19:50 predicted for 11:20:50 is one
    /// minute off and so on time, while the minutes it draws are 11:19 and
    /// 11:20. That row struck out a time the status said was being kept.
    ///
    /// Late and early are at least ninety-one seconds from the timetable, which
    /// always lands in another minute, so there is no case where this returns a
    /// time that draws the same as the one beside it.
    public var promised: Int? {
        switch status {
        case .late, .early: scheduled
        case .onTime, .scheduled, .cancelled: nil
        }
    }
}
