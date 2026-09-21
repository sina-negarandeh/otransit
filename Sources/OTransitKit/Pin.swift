// A board somebody kept, and the file the kept ones live in.
//
// A pin is a board and not a stop. One pole holds two of them and needs to:
// nine routes serve the single pole at TRANSITWAY / TERMINAL and they run to
// eight destinations, and routes 44 and 48 both end at Billings Bridge by roads
// that do not meet. A pin that remembered only the stop would answer a question
// nobody asked.
//
// The file is this app's own, beside the cache and the key. It is tab
// separated because a stop name holds commas and slashes and never a tab, so
// nothing needs escaping and a person can edit it by hand.

import Foundation

public struct Pin: Sendable, Equatable, Identifiable {
    /// The stop's id in the feed, which is what every query takes.
    public let stop: String
    /// The pole number and the name, kept only so they can be written back.
    /// Nothing resolves through them: every screen asks the cache instead,
    /// because an update replaces the whole timetable.
    public let code: String
    public let name: String
    public let route: String
    public let headsign: String

    /// What makes two pins the same pin. The stop alone is not enough.
    public var id: String { "\(stop)\t\(route)\t\(headsign)" }

    public init(stop: String, code: String, name: String, route: String, headsign: String) {
        self.stop = stop
        self.code = code
        self.name = name
        self.route = route
        self.headsign = headsign
    }
}

public enum Pins {
    /// How many kept boards a screen has room for.
    ///
    /// Measured, and measured again each time the row changed shape. A kept row
    /// is 41 points: the stop on one line and the direction under it, which is
    /// the shape every other row on this screen has. Three of them with the
    /// group's own padding is 130, and that leaves 9 points of air under the
    /// mode list above.
    ///
    /// Nine and not more because the first screen has no slack to give. What
    /// looked like room above the pins was the mode list being stretched, and
    /// it gives that up as the pins take it: at 48 points a row the gap under
    /// "166 routes running today" went to 3, and a row taller still would push
    /// Bus up until its second line was cut off. That is the failure to watch
    /// for here, and it appears in the mode list rather than in the pins.
    ///
    /// A fourth pin does not fit at any row height worth reading.
    public static let room = 3

    /// Whether another board can be kept, given how many can be drawn now.
    ///
    /// Counted against what can be drawn and not against the lines in the file,
    /// so pins nothing can resolve today do not block one that would draw. The
    /// caller decides what "drawable" means; this owns the number.
    public static func fits(drawable: Int) -> Bool { drawable < room }

    /// The first `room` of these, which is all a screen can show.
    ///
    /// The rest stay in the file. A file is something a person can edit, and a
    /// fourth line somebody typed is not a reason to throw the line away.
    public static func drawable<Kept>(_ boards: [Kept]) -> [Kept] {
        Array(boards.prefix(room))
    }

    /// The pins a file holds, in the order they were kept.
    ///
    /// A line that will not parse is skipped and the rest are kept. This is a
    /// file a person can edit, and one mangled line must not lose the pins
    /// above it. Blank lines and comments are not lines that failed.
    public static func parse(_ text: String) -> [Pin] {
        text.split(whereSeparator: \.isNewline).compactMap { line in
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            guard !trimmed.isEmpty, !trimmed.hasPrefix("#") else { return nil }

            let fields = trimmed.components(separatedBy: "\t")
            guard fields.count == 5, !fields[0].isEmpty, !fields[3].isEmpty else { return nil }
            return Pin(
                stop: fields[0], code: fields[1], name: fields[2],
                route: fields[3], headsign: fields[4])
        }
    }

    /// The pins as the file holds them, in the form `parse` reads.
    public static func render(_ pins: [Pin]) -> String {
        pins.map { [$0.stop, $0.code, $0.name, $0.route, $0.headsign].joined(separator: "\t") }
            .joined(separator: "\n") + (pins.isEmpty ? "" : "\n")
    }

    /// The pins in the file, or none where there is no file yet.
    ///
    /// A missing file is not an error: it is what "nothing kept yet" looks
    /// like, and a first run must not fail on it.
    public static func read(from url: URL = Paths.pins) -> [Pin] {
        guard let text = try? String(contentsOf: url, encoding: .utf8) else { return [] }
        return parse(text)
    }

    /// Writes the pins down, replacing what was there.
    ///
    /// Atomically, because a menu bar app is quit without warning and a write
    /// that stopped half way would leave a truncated line and no pins.
    public static func write(_ pins: [Pin], to url: URL = Paths.pins) throws {
        guard !pins.isEmpty else {
            try? FileManager.default.removeItem(at: url)
            return
        }
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try render(pins).write(to: url, atomically: true, encoding: .utf8)
    }
}

extension Pin {
    /// The pin that names this board, or nil where the place is not a board.
    ///
    /// Here and not beside `Kept`, which holds the other direction of it: both
    /// of these types are the kit's, so a conversion written in the app is one
    /// the kit cannot reach and the next caller writes out again.
    public init?(_ place: Place) {
        guard case .board(_, let route, let headsign, let stop) = place else { return nil }
        self.init(
            stop: stop.id, code: stop.code, name: stop.name,
            route: route.shortName, headsign: headsign)
    }
}
