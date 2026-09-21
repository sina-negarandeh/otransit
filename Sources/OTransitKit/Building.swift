// What building a cache is doing, in the words a person reading it would use.
//
// Model and not view: the phases are facts about the work, the sentence each
// one reads as is a decision about transit and not about SwiftUI, and a test
// can reach both. A view chooses the font.

import Foundation

public enum Building: Sendable, Equatable {
    case checking
    /// `total` is absent when the server declines to say how long the body is,
    /// which is a progress bar that cannot be drawn rather than a failure.
    case downloading(received: Int64, total: Int64?)
    case unpacking
    case reading(String, rows: Int)
    case indexing

    /// The line shown while this phase runs.
    public var line: String {
        switch self {
        case .checking: "checking for a new schedule"
        case .downloading(let received, let total):
            if let total, total > 0 {
                "downloading \(Self.mb(received)) of \(Self.mb(total))"
            } else {
                "downloading \(Self.mb(received))"
            }
        case .unpacking: "unpacking"
        case .reading(let file, let rows):
            rows == 0 ? "reading \(file)" : "reading \(file) — \(Self.count(rows))"
        case .indexing: "building indexes"
        }
    }

    /// How far along, when that is known. Indexing takes about as long as the
    /// rows did and reports nothing while it runs, so it is deliberately not a
    /// fraction: a bar that sits at 99% for ten seconds is a bar that lies.
    public var fraction: Double? {
        if case .downloading(let received, let total) = self, let total, total > 0 {
            return min(1, Double(received) / Double(total))
        }
        return nil
    }

    /// Megabytes as the download itself counts them. A server measuring a body
    /// in millions of bytes and a progress line dividing by 1,048,576 disagree
    /// by five per cent, and the person watching can see the total is wrong.
    private static func mb(_ bytes: Int64) -> String {
        "\(bytes / 1_000_000) MB"
    }

    private static func count(_ rows: Int) -> String {
        rows >= 1_000_000
            ? String(format: "%.1fM rows", Double(rows) / 1_000_000) : "\(rows / 1000)k rows"
    }
}
