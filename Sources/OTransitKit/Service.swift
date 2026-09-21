// What kind of service a route is, in OC Transpo's own terms.
//
// The operator publishes both halves of this: a shape and colour for each kind,
// and a numbering rule for most of them. Riders read the shapes off every map
// and pole in the city, so a badge that borrows them says what kind of route
// this is before its number is read — and it keeps saying it in greyscale.
//
// The feed never names the kind, but it does not have to. Between route_color,
// the number, and the letter a name starts with, every route today lands
// somewhere the operator itself would put it.
//
//   Frequent      blue hexagon        route_color 0057B8
//   Connexion     purple oval         route_color B66B94, numbered in the 200s
//   Local         dark grey rectangle route_color 6D6E70
//   Limited       white background    route_color FFFFFF — "only runs during
//                                     certain times of the day, or on certain
//                                     days of the week"
//
// Eight of those nine are a kind the operator names. The ninth is not: what is
// left after the eight have taken theirs is called Other here, and it is not
// OC Transpo's Limited. Seven of its 27 routes are the express ones — 526, 539,
// 561 to 566 — which the operator does not call Limited at all. A residual is
// worth having and worth admitting to; it is not worth borrowing a name for.
//   School        white rectangle     numbered in the 600s
//   Event         white rectangle     numbered in the 400s and 450s
//   Shopper       —                   301 to 305, one round trip a week
//   Night         white hexagon       route name begins with N
//   Replacement   —                   R1, R2, R4, and the E1 Shuttle Express

extension Route {
    /// The kind of service this route is, as the network calls it.
    public func service(in mode: Mode) -> Service {
        guard mode == .bus else { return .line }

        // The letter first. A night route is an extension of a Frequent one and
        // carries its colour, so asked by colour it would come back Frequent —
        // and the operator gives it a symbol of its own.
        if shortName.hasPrefix("N") { return .night }
        // R1, R2, R4 stand in for a closed line. E1 is the Shuttle Express,
        // which the operator documents on the same page.
        if shortName.hasPrefix("R") || shortName.hasPrefix("E") { return .replacement }

        switch colour.trimmingCharacters(in: .whitespaces).uppercased() {
        case "0057B8": return .frequent
        case "B66B94": return .connexion
        case "6D6E70": return .local
        default: break
        }

        // White is not the absence of a colour. It is the operator's mark for a
        // route that does not run all day, and the number says which sort.
        switch Int(shortName) {
        case .some(600...699): return .school
        case .some(400...459): return .event
        case .some(301...305): return .shopper
        default: return .other
        }
    }
}

public enum Service: Sendable, Equatable, CaseIterable {
    case line
    case frequent
    case connexion
    case local
    /// What is left when every kind the operator names has taken its own.
    /// Mostly routes that run only at certain times, which is what OC Transpo
    /// means by Limited — but not only those, so not that word.
    case other
    case school
    case event
    case shopper
    case night
    case replacement

    /// What the network calls it.
    /// Everything fixed about one kind of service, in one place.
    ///
    /// These were four switches over the same ten cases, sixty lines apart, so
    /// everything known about Connexion was written in four places and adding a
    /// kind meant finding all of them. One switch puts a kind on one line.
    ///
    /// A dictionary would say the same thing more briefly and would cost the
    /// exhaustiveness: `[Service: Facts]` needs a bang or a default, and a
    /// default is how a kind gets added and silently drawn as something else.
    /// The switch makes the compiler ask.
    private struct Facts: Sendable {
        /// What anything that stores this calls it. Never `name`: that is a
        /// label on a screen and free to change — Limited became Other — and a
        /// preference keyed on the old word keeps a dead entry nothing can
        /// match while the section it stood for comes back unfolded.
        let key: String
        /// What the network calls it.
        let name: String
        /// The colour the operator gives it, as the feed writes it. The three
        /// constants here are the ones the classifier matches on, so a heading
        /// cannot be drawn in a colour that would not have sorted a route into
        /// it. White is the operator's mark for a route that does not run all
        /// day, and is drawn as no colour at all.
        let colour: String
        /// The numbers it is drawn from, where there is a pattern to state.
        ///
        /// Read off the same ranges the classifier sorts by, which sit in the
        /// switch two properties up. Blank for the three that have no pattern —
        /// Frequent runs 5 to 111, Local 8 to 197, Other 13 to 566 — and for
        /// Replacement, whose routes are lettered rather than numbered and are
        /// not all the same letter: R1 and R2 stand in for a closed line, E1 is
        /// the Shuttle Express.
        let numbering: String
    }

    private var facts: Facts {
        switch self {
        case .line: Facts(key: "line", name: "Lines", colour: "", numbering: "")
        case .frequent: Facts(key: "frequent", name: "Frequent", colour: "0057B8", numbering: "")
        case .connexion:
            Facts(key: "connexion", name: "Connexion", colour: "B66B94", numbering: "200s")
        case .local: Facts(key: "local", name: "Local", colour: "6D6E70", numbering: "")
        case .other: Facts(key: "other", name: "Other", colour: "FFFFFF", numbering: "")
        case .school: Facts(key: "school", name: "School", colour: "FFFFFF", numbering: "600s")
        case .event: Facts(key: "event", name: "Event", colour: "FFFFFF", numbering: "400-459")
        case .shopper:
            Facts(key: "shopper", name: "Shopper", colour: "FFFFFF", numbering: "301-305")
        case .night: Facts(key: "night", name: "Night", colour: "FFFFFF", numbering: "N")
        case .replacement:
            Facts(key: "replacement", name: "Replacement", colour: "FFFFFF", numbering: "")
        }
    }

    public var key: String { facts.key }
    public var name: String { facts.name }
    public var colour: String { facts.colour }
    public var numbering: String { facts.numbering }

    /// The order the sections are offered in.
    ///
    /// The three kinds that run all day and carry a colour of their own first,
    /// then the four the operator names by a number or a letter, then the
    /// residual, then Replacement. Other goes near the end because a bucket of
    /// leftovers listed fourth reads as a kind of service rather than as what
    /// is left over.
    ///
    /// Several are empty on any given day — Event runs for events, Night and
    /// Replacement have no route at all in this feed — and an empty section is
    /// not drawn.
    static let order: [Service] = [
        .line, .frequent, .connexion, .local,
        .school, .event, .shopper, .night,
        .other, .replacement,
    ]

    /// Whether a section starts closed.
    ///
    /// School alone. On a weekday it is 52 of the 166 routes running and serves
    /// one trip for one audience, so it is a third of the list standing between
    /// a person and the route they want. The heading still counts them.
    public var startsClosed: Bool { self == .school }
}

/// One section of the route list.
public struct RouteGroup: Sendable, Identifiable, Equatable {
    public var id: String { service.name }
    public let service: Service
    public let routes: [Route]
}

extension Service {
    /// Splits routes into sections, in the order above, dropping any that no
    /// route lands in.
    public static func group(_ routes: [Route], in mode: Mode) -> [RouteGroup] {
        let byService = Dictionary(grouping: routes) { $0.service(in: mode) }
        return order.compactMap { service in
            guard let found = byService[service], !found.isEmpty else { return nil }
            return RouteGroup(service: service, routes: found)
        }
    }
}
