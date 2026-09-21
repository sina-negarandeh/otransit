// Where you are, and everything it took to get there.
//
// One value, and every case of it carries what that place needs. There is no
// second copy of this to disagree with: a board cannot exist without the stop
// it is a board of, and a stop cannot have been chosen before a route was,
// because neither is a state this type can hold.
//
// The alternative — one optional per answer — is the same information with the
// impossible states left in, and then a guard chain in every reader to walk
// past them.

/// A screen, and the answers that led to it.
public enum Place: Sendable, Equatable {
    case transit
    case routes(Mode)
    case direction(Mode, Route)
    case stops(Mode, Route, String)
    case board(Mode, Route, String, Stop)

    /// What this place is called and what it asks.
    public var screen: Screen {
        switch self {
        case .transit: .transit
        case .routes(let mode): .routes(mode)
        case .direction(let mode, _): .direction(mode)
        case .stops(let mode, _, _): .stops(mode)
        case .board(let mode, _, _, _): .board(mode)
        }
    }

    /// How many questions have been answered. Also the number of crumbs the
    /// trail draws, because they are the same count read two ways.
    public var depth: Int {
        switch self {
        case .transit: 0
        case .routes: 1
        case .direction: 2
        case .stops: 3
        case .board: 4
        }
    }

    /// The place one question shallower, or nil at the first screen.
    ///
    /// Each case knows what it was before it, so stepping back is neither a
    /// count nor a set of fields to clear — both of which had to be kept in
    /// agreement with this enum by hand.
    public var back: Place? {
        switch self {
        case .transit: nil
        case .routes: .transit
        case .direction(let mode, _): .routes(mode)
        case .stops(let mode, let route, _): .direction(mode, route)
        case .board(let mode, let route, let headsign, _): .stops(mode, route, headsign)
        }
    }

    /// The place with only the first `count` answers kept. Tapping "Bus" in the
    /// trail of a stop list keeps one and drops the rest.
    public func keeping(_ count: Int) -> Place {
        var out = self
        while out.depth > count, let shallower = out.back { out = shallower }
        return out
    }
}
