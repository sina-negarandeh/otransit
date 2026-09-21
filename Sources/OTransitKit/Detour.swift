// What OC Transpo has published about a route, and which routes it names.
//
// This is the weakest of the three sources this program reads. The other two
// are specified formats. This one is a content system that emits RSS, where a
// category beginning `affectedRoutes-` is that system's convention and not a
// contract. If that spelling changes, every screen shows nothing, which looks
// exactly like a week without news. `items` is how a caller tells those apart:
// it counts what the feed held.
//
// An item is kept when it names a route, and for no other reason. The feed
// also files each item under `Detours` or `General Message`, and that heading
// used to be half the test. It was doing nothing: every item filed under
// `Detours` names routes, and of those filed under `General Message` the only
// ones that name a route are the ones worth reading. What the heading cost was
// the live rail alerts, which the operator files as general messages — "Line 1:
// service is operating on the eastbound platforms only at Tremblay station" is
// not a reroute, and was being dropped for not being one.
//
// The heading is kept for what it is good for, which is saying which of the
// two an item is: a bus sent around a closure, or a route running under a
// limitation. It picks the mark drawn beside the words. It is not always right
// — a stop relocation is filed under `Detours` often as not — and being wrong
// about a mark costs a glance, where being wrong about a gate cost the alert.
//
// Two things the feed gets wrong, which shape what is drawn:
//
// A night route is tagged as its day route. `DETOUR: Route N39 Rideau near
// Belfast` carries `affectedRoutes-39`, so a rider on 39 is shown a notice
// about N39. The category is the only field a program can read, and it is
// wrong. Nothing here can fix that, so the title is shown exactly as published:
// it says N39, and a person reading it sees what a matcher cannot.
//
// A route is named that no timetable has. `454` is tagged and no route by that
// name runs. Harmless, because nothing ever asks for it.

import Foundation

/// One published notice, and the routes it names.
public struct Notice: Sendable, Equatable, Identifiable {
    /// Which of the two sorts of news this is.
    ///
    /// Read off the heading the feed files an item under, which is the only
    /// structured field that says anything about it. A detour sends a route
    /// somewhere else; an alert says a route is running under a limitation
    /// where it is. They are drawn as different marks and not as different
    /// colours: orange already means "the operator has published something
    /// about this route", and one mark meaning one thing should not be two
    /// colours.
    public enum Kind: Sendable, Equatable {
        case detour
        case alert
    }

    public let id: String
    public let kind: Kind
    /// As published, including the prefix the feed happens to use that day:
    /// `DETOUR:`, `Detour:`, `Detour extended:`, `[Updated`. Not tidied. The
    /// prefix is eight characters and the wording is sometimes the only thing
    /// that says a detour has changed rather than started.
    public let title: String
    /// The routes the feed says this is about, as it spells them.
    public let routes: [String]
    /// The full notice, which is where the detail lives. A description runs to
    /// about 2,800 characters of HTML, so it is never drawn here.
    public let link: URL?
    public let published: Date?

    /// The title without the word the icon beside it already says.
    ///
    /// Only the two spellings that are exactly the label: `DETOUR:` and
    /// `Detour:`. `Detour extended:` keeps its first word, because "extended"
    /// is the only thing in the notice that says this one has been running for
    /// a while rather than starting today, and that is worth eight characters.
    ///
    /// A prefix this does not know is left alone, so a feed that changes its
    /// wording loses the saving and nothing else.
    public var headline: String {
        for label in ["DETOUR:", "Detour:"] where title.hasPrefix(label) {
            return String(title.dropFirst(label.count)).trimmingCharacters(in: .whitespaces)
        }
        return title
    }

    public init(
        id: String, kind: Kind, title: String, routes: [String], link: URL?, published: Date?
    ) {
        self.id = id
        self.kind = kind
        self.title = title
        self.routes = routes
        self.link = link
        self.published = published
    }
}

/// One fetch of the updates feed, holding everything that names a route.
public struct Detours: Sendable, Equatable {
    public let notices: [Notice]
    /// How many items the feed held, about a route or not.
    ///
    /// A feed that parsed and held nothing this program understands is a
    /// different thing from a quiet week, and only this tells them apart.
    public let items: Int

    /// What each route named has, and the worse of the two where it has both.
    ///
    /// Built once here rather than filtered per row. The route list asks this
    /// of 176 routes on every pass, and `naming` allocates an array to answer
    /// each one.
    public let affected: [String: Notice.Kind]

    /// A fetch with nothing to say.
    ///
    /// Which is also what "nothing has been asked yet" looks like, and that is
    /// the point of it. The two states answer every question this type is
    /// asked identically — no kind for any route, no notices naming one, and
    /// `unreadable` false because nothing was held — so a caller holding this
    /// optional was holding a distinction that does not exist.
    public static let quiet = Detours(notices: [], items: 0)

    public init(notices: [Notice], items: Int) {
        self.notices = notices
        self.items = items
        var worst: [String: Notice.Kind] = [:]
        for notice in notices {
            for route in notice.routes where worst[route] != .alert {
                worst[route] = notice.kind
            }
        }
        self.affected = worst
    }

    /// What this route has, or nil where nothing names it.
    ///
    /// An alert where it has one, whatever else it has. A route can carry both,
    /// and where one mark has to stand for the pair it is the alert: a
    /// limitation is happening now and roadwork has been running since spring.
    public func kind(of route: String) -> Notice.Kind? { affected[route] }

    /// Whether anything names this route.
    public func names(_ route: String) -> Bool { affected[route] != nil }

    /// Whether the feed held items and this program understood none of them.
    ///
    /// The shape this guards against: the content system renames
    /// `affectedRoutes-`, every item still parses, and every screen shows
    /// nothing. That looks exactly like a quiet week. `otransit updates`
    /// reports this, so a change of shape appears as a number rather than as
    /// silence.
    public var unreadable: Bool { items > 0 && notices.isEmpty }

    /// Every notice naming this route, the alerts first.
    ///
    /// All of them, not the first. Eleven routes carry two today and one
    /// carries three, and a screen that showed only the first would say a route
    /// has one notice on a day it has three. Sorted so the live limitation is
    /// read before the roadwork, and stably, so two of a kind keep the order
    /// the feed published them in.
    ///
    /// Matched exactly. A list holding `4` says nothing about `44`.
    public func naming(_ route: String) -> [Notice] {
        let mine = notices.filter { $0.routes.contains(route) }
        return mine.filter { $0.kind == .alert } + mine.filter { $0.kind != .alert }
    }
}

extension Detours {
    /// The tag an item needs, spelled as the feed spells it. Matched exactly:
    /// lowering it finds nothing, every week, quietly.
    static let prefix = "affectedRoutes-"

    /// The heading that means a route was sent somewhere else. Anything else,
    /// including no heading at all, is a route running under a limitation.
    static let detour = "Detours"

    /// Reads the feed. An item that names no route is skipped rather than
    /// refused: the feed carries station notices and cancelled trips too, and
    /// one item this program does not understand must not take the others with
    /// it.
    public static func read(_ data: Data) throws -> Detours {
        let reader = Reader()
        let parser = XMLParser(data: data)
        parser.delegate = reader
        guard parser.parse() else {
            throw Failure.unreadable(parser.parserError?.localizedDescription ?? "not XML")
        }
        return Detours(notices: reader.notices, items: reader.items)
    }

    public enum Failure: Error, CustomStringConvertible {
        case unreadable(String)

        public var description: String {
            switch self {
            case .unreadable(let why): "reading the updates: \(why)"
            }
        }
    }

    /// The routes an item is about, as the feed spells them.
    ///
    /// Empty where it is about no route, which is what keeps station notices
    /// and cancelled trips out: a washroom closure carries the category with
    /// nothing after it, and a cancelled trip carries no categories at all.
    ///
    /// Not `affected`, which is the property above holding every route this
    /// whole fetch touches. One name for the routes of one item and the routes
    /// of all of them is one name too few.
    static func routes(_ categories: [String]) -> [String] {
        var routes: [String] = []
        for category in categories where category.hasPrefix(prefix) {
            // Trimmed, because the feed writes the list for a person to read:
            // `affectedRoutes-19, 42, 44, 48`.
            //
            // And cut at the first space. Rail is written `1 O-Train` where a
            // bus is written `1`, and no route in the timetable has a space in
            // its name, so the first word is the name and the rest is the feed
            // being helpful.
            routes += category.dropFirst(prefix.count)
                .split(separator: ",")
                .compactMap { $0.split(whereSeparator: \.isWhitespace).first.map(String.init) }
        }
        return routes
    }

    /// Which of the two sorts of news an item is.
    ///
    /// A heading this does not know reads as an alert rather than as a detour.
    /// The mark for an alert says "there is something to read here", which is
    /// true of anything the feed publishes; the mark for a detour says the
    /// route goes somewhere else, which is a claim.
    static func kind(_ categories: [String]) -> Notice.Kind {
        categories.contains(detour) ? .detour : .alert
    }

    /// The date an item carries, which RSS writes one fixed way.
    ///
    /// One formatter, built once. Constructing a DateFormatter is among the
    /// more expensive things in Foundation, and this is asked once per item.
    /// Nothing configures it again after this, which is what makes reading a
    /// date from it safe from anywhere.
    private static let rfc822: DateFormatter = {
        let form = DateFormatter()
        form.locale = Locale(identifier: "en_US_POSIX")
        form.dateFormat = "EEE, dd MMM yyyy HH:mm:ss zzz"
        return form
    }()

    static func published(_ text: String) -> Date? {
        rfc822.date(from: text.trimmingCharacters(in: .whitespacesAndNewlines))
    }
}

/// The delegate XMLParser wants, kept private to this file.
///
/// It reads four of an item's six fields and ignores `description`, which is
/// the largest by far and is served as CDATA. XMLParser hands CDATA to a
/// different method than text, so ignoring it costs nothing and keeps 2,800
/// characters an item out of memory.
private final class Reader: NSObject, XMLParserDelegate {
    private(set) var notices: [Notice] = []
    private(set) var items = 0

    private var inItem = false
    private var text = ""
    private var categories: [String] = []
    private var fields: [String: String] = [:]

    func parser(
        _ parser: XMLParser, didStartElement name: String, namespaceURI: String?,
        qualifiedName: String?, attributes: [String: String]
    ) {
        text = ""
        if name == "item" {
            inItem = true
            categories = []
            fields = [:]
        }
    }

    func parser(_ parser: XMLParser, foundCharacters found: String) {
        guard inItem else { return }
        text += found
    }

    /// CDATA arrives here and not above.
    ///
    /// Only `description` is wrapped today, and nothing reads it. The moment
    /// the content system wraps a title or a link, which RSS generators
    /// commonly do, the field would otherwise read back empty and the row would
    /// draw an icon beside nothing at all.
    func parser(_ parser: XMLParser, foundCDATA block: Data) {
        guard inItem, let found = String(data: block, encoding: .utf8) else { return }
        text += found
    }

    func parser(
        _ parser: XMLParser, didEndElement name: String, namespaceURI: String?,
        qualifiedName: String?
    ) {
        guard inItem else { return }

        if name == "item" {
            inItem = false
            items += 1
            let routes = Detours.routes(categories)
            guard !routes.isEmpty else { return }
            let title = (fields["title"] ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
            let guid = (fields["guid"] ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
            let link = (fields["link"] ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
            notices.append(
                Notice(
                    // The feed's own guid where it has one, so a notice keeps
                    // its identity across fetches and a list does not animate
                    // every row on every poll. Where it has none, the title
                    // alone can repeat: the feed publishes near-identical
                    // wording for a detour and its extension, and two rows
                    // sharing an id is undefined behaviour for a ForEach. The
                    // position settles it.
                    id: guid.isEmpty ? "\(items):\(title)" : guid,
                    kind: Detours.kind(categories),
                    title: title,
                    routes: routes,
                    link: URL(string: link),
                    published: Detours.published(fields["pubDate"] ?? "")))
            return
        }

        if name == "category" {
            categories.append(text.trimmingCharacters(in: .whitespacesAndNewlines))
        } else {
            fields[name] = text
        }
    }
}
