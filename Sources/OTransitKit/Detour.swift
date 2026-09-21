// What OC Transpo has published about a route, and which routes it names.
//
// This is the weakest of the three sources this program reads. The other two
// are specified formats. This one is a content system that emits RSS, where an
// item tagged `Detours` carrying a category that begins `affectedRoutes-` is
// that system's convention and not a contract. If either spelling changes,
// every screen shows no detour, which looks exactly like a week without one.
// `items` is how a caller tells those apart: it counts what the feed held.
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
    public let id: String
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
        id: String, title: String, routes: [String], link: URL?, published: Date?
    ) {
        self.id = id
        self.title = title
        self.routes = routes
        self.link = link
        self.published = published
    }
}

/// One fetch of the updates feed, holding only the detours.
public struct Detours: Sendable, Equatable {
    public let notices: [Notice]
    /// How many items the feed held, detour or not.
    ///
    /// A feed that parsed and held nothing this program understands is a
    /// different thing from a week with no detours, and only this tells them
    /// apart.
    public let items: Int

    /// Every route any notice names.
    ///
    /// Built once here rather than filtered per row. The route list asks this
    /// of 176 routes on every pass, and `naming` allocates an array to answer
    /// each one.
    public let affected: Set<String>

    public init(notices: [Notice], items: Int) {
        self.notices = notices
        self.items = items
        self.affected = Set(notices.flatMap(\.routes))
    }

    /// Whether anything names this route.
    public func names(_ route: String) -> Bool { affected.contains(route) }

    /// Whether the feed held items and this program understood none of them.
    ///
    /// The shape this guards against: the content system renames `Detours` or
    /// `affectedRoutes-`, every item still parses, and every screen shows no
    /// detour. That looks exactly like a quiet week. `otransit detours` reports
    /// this, so a change of shape appears as a number rather than as silence.
    public var unreadable: Bool { items > 0 && notices.isEmpty }

    /// Every notice naming this route.
    ///
    /// All of them, not the first. Eleven routes carry two today and one
    /// carries three, and a screen that showed only the first would say a route
    /// has one detour on a day it has three.
    ///
    /// Matched exactly. A list holding `4` says nothing about `44`.
    public func naming(_ route: String) -> [Notice] {
        notices.filter { $0.routes.contains(route) }
    }
}

extension Detours {
    /// The two tags an item needs, spelled as the feed spells them. Both are
    /// matched exactly: lowering either one finds nothing, every week, quietly.
    static let kind = "Detours"
    static let prefix = "affectedRoutes-"

    /// Reads the feed. An item that is not a detour is skipped rather than
    /// refused: the feed carries general messages and stop notices too, and one
    /// item this program does not understand must not take the others with it.
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

    /// The routes an item is about, and whether it is a detour at all.
    static func affected(_ categories: [String]) -> [String]? {
        var routes: [String] = []
        var tagged = false
        for category in categories {
            if category == kind {
                tagged = true
                continue
            }
            guard category.hasPrefix(prefix) else { continue }
            // Trimmed, because the feed writes the list for a person to read:
            // `affectedRoutes-19, 42, 44, 48`.
            routes += category.dropFirst(prefix.count)
                .split(separator: ",")
                .map { $0.trimmingCharacters(in: .whitespaces) }
                .filter { !$0.isEmpty }
        }
        return tagged && !routes.isEmpty ? routes : nil
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
            guard let routes = Detours.affected(categories) else { return }
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
