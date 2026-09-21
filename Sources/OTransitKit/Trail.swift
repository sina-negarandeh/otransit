// The path you took to get here, as the bottom bar draws it.
//
// Built from what has been chosen rather than from a navigation stack, so the
// trail cannot disagree with the screen: there is one place that knows a route
// was picked, and this reads it. A crumb for every answer so far, in order,
// and nothing for the questions still open.

/// One step of the path.
public struct Crumb: Sendable, Equatable, Identifiable {
    /// Its place in the trail, which is stable while the trail is on screen and
    /// is what a list needs to tell two crumbs apart. Two stops on one route can
    /// share a name; their positions cannot.
    public let id: Int
    public let label: String
    /// The route's own colour, set only on the crumb that is a route. A crumb
    /// with one is drawn as a badge; the rest are text.
    public let colour: String?
    /// The kind of service, so the badge here is the same shape as the one in
    /// the list it was chosen from.
    public let service: Service?
    /// The platform, set only on the crumb that is a stop. It travels with the
    /// name because it is the thing you are standing there looking for: the
    /// board says when, and this is the only place left that says where.
    public let platform: String?
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
        id: Int, label: String, colour: String? = nil, service: Service? = nil,
        platform: String? = nil, symbol: String? = nil, spoken: String? = nil
    ) {
        self.id = id
        self.label = label
        self.colour = colour
        self.service = service
        self.platform = platform
        self.symbol = symbol
        self.spoken = spoken ?? label
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
                        id: out.count, label: "", symbol: mode.symbol, spoken: mode.title))
            case .direction(let mode, let route):
                out.append(
                    Crumb(
                        id: out.count, label: route.shortName, colour: route.colour,
                        service: route.service(in: mode)))
            case .stops(_, let route, let headsign):
                // The arrow says "toward", and says it in 10 points where the
                // word takes 38. It is the arrow the direction screen already
                // puts at the head of every row it offers.
                let toward = headsign.english
                out.append(
                    Crumb(
                        id: out.count, label: toward, colour: route.colour,
                        symbol: "arrowshape.right.fill", spoken: "Toward \(toward)"))
            case .board(_, _, _, let stop):
                out.append(
                    Crumb(
                        id: out.count, label: stop.name,
                        platform: stop.platform.isEmpty ? nil : stop.platform))
            }
        }
        return out
    }
}
