// The path you took to get here, as the bottom bar draws it.
//
// Built from what has been chosen rather than from a navigation stack, so the
// trail cannot disagree with the screen: there is one place that knows a route
// was picked, and this reads it. A crumb for every answer so far, in order,
// and nothing for the questions still open.

/// One step of the path.
public struct Crumb: Sendable, Equatable, Identifiable {
    /// What this crumb is a crumb of, and whatever drawing that kind needs.
    ///
    /// Carried rather than inferred. `crumbs` switches over exactly these four
    /// cases to build them, and it used to throw the answer away: every place
    /// that drew a crumb worked the case back out of a combination of
    /// optionals. A colour and a service meant a route, a colour alone meant a
    /// direction, and "past the second crumb, with no symbol" meant a stop.
    /// Three rules in three files, none of them naming what it tested, each of
    /// them wrong the day the order here changes.
    public enum Kind: Sendable, Equatable {
        /// Bus or train, drawn as the mark the first screen gave it.
        case mode
        /// Drawn as a badge, in the shape its service wears everywhere else.
        case route(colour: String, service: Service)
        /// The route's colour travels with it, so the arrow here is the colour
        /// of the badge before it.
        case direction(colour: String)
        case stop(platform: String?)
    }

    /// Its place in the trail, which is stable while the trail is on screen and
    /// is what a list needs to tell two crumbs apart. Two stops on one route can
    /// share a name; their positions cannot.
    public let id: Int
    public let kind: Kind
    public let label: String
    /// A mark this crumb is drawn as, where one says it shorter than the word.
    ///
    /// The bar cannot be made to fit — one headsign alone is 180 of its 225
    /// points — so what it costs matters. A bus and a tram are already the
    /// marks the first screen puts beside those two words, and an arrow is
    /// already what the direction screen puts beside a headsign, so this takes
    /// nothing away that the screen it came from did not already teach.
    public let symbol: String?
    /// What a screen reader says. The words, even where the screen shows a mark.
    public let spoken: String

    init(
        id: Int, kind: Kind, label: String, symbol: String? = nil, spoken: String? = nil
    ) {
        self.id = id
        self.kind = kind
        self.label = label
        self.symbol = symbol
        self.spoken = spoken ?? label
    }

    /// The colour this crumb's mark is drawn in, where its mark has one.
    ///
    /// Only a direction has both. A route carries a colour too, but it wears it
    /// as a badge rather than as a mark, and a mode's mark is the same grey
    /// whatever route is picked next.
    public var tint: String? {
        if case .direction(let colour) = kind { return colour }
        return nil
    }

    /// Whether this is the crumb for a stop, which is the last one and the only
    /// one that can carry a plate.
    public var isStop: Bool {
        if case .stop = kind { return true }
        return false
    }

    /// Whether this is the crumb for a direction, which is the one that says
    /// where the route is going rather than where you are standing.
    public var isDirection: Bool {
        if case .direction = kind { return true }
        return false
    }

    /// The plate on the pole, where this crumb is a stop that has one.
    ///
    /// It travels with the name because it is the thing you are standing there
    /// looking for: the board says when, and this is the only place left that
    /// says where.
    public var platform: String? {
        if case .stop(let platform) = kind { return platform }
        return nil
    }

    /// The badge this crumb is drawn as, where it is drawn as one.
    public var plate: (colour: String, service: Service)? {
        if case .route(let colour, let service) = kind { return (colour, service) }
        return nil
    }
}

public enum Trail {
    /// The crumbs for a place, one per answer that got there.
    ///
    /// Read off the place itself rather than off a set of optionals, so there
    /// is no arrangement of arguments this has to defend against and no
    /// hand-written id that has to stay in step with the order below.
    public static func crumbs(of place: Place) -> [Crumb] {
        var out: [Crumb] = []
        var here: Place? = place
        // Shallowest first, which is the order they are read in.
        var chain: [Place] = []
        while let step = here, step.depth > 0 {
            chain.insert(step, at: 0)
            here = step.back
        }

        for step in chain {
            switch step {
            case .transit:
                break
            case .routes(let mode):
                // The mark, not the word. It is the same mark the row on the
                // first screen carried, and it is 19 points where "O-Train"
                // is 44.
                out.append(
                    Crumb(
                        id: out.count, kind: .mode, label: "",
                        symbol: mode.symbol, spoken: mode.title))
            case .direction(let mode, let route):
                out.append(
                    Crumb(
                        id: out.count,
                        kind: .route(colour: route.colour, service: route.service(in: mode)),
                        label: route.shortName))
            case .stops(_, let route, let headsign):
                // The arrow says "toward", and says it in 10 points where the
                // word takes 38. It is the arrow the direction screen already
                // puts at the head of every row it offers.
                let toward = headsign.english
                out.append(
                    Crumb(
                        id: out.count, kind: .direction(colour: route.colour), label: toward,
                        symbol: "arrowshape.right.fill", spoken: "Toward \(toward)"))
            case .board(_, _, _, let stop):
                out.append(
                    Crumb(
                        id: out.count,
                        kind: .stop(platform: stop.platform.isEmpty ? nil : stop.platform),
                        label: stop.name))
            }
        }
        return out
    }
}
