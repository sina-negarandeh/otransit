// Whether the schedule on disk is the one the city is publishing.
//
// This exists because of a failure that looks like nothing at all. Predictions
// join to the timetable by trip id and by nothing else, and a trip id is only
// good against the export it was issued against. When the city republishes, the
// ids in the live feed stop naming trips the cache has heard of, every row
// quietly falls back to its scheduled time, and the board goes on looking
// exactly like a board — correct, complete, and hours out of date.
//
// It happened here. Six days after an ingest, 53 of 327 live trips could be
// found in the cache: 16 per cent. After the rebuild it was 321 of 327, and the
// six that were left were the feed's own added trips, which are not in any
// timetable by definition. Nothing on screen said so either time.
//
// So the state is modelled rather than inferred, and the screen says it.

import Foundation

/// What is known about the schedule on disk against the one being published.
public enum Freshness: Sendable, Equatable {
    /// Nothing has been asked yet.
    case unknown
    /// The question is in flight.
    case checking
    /// The cache holds what the server is publishing.
    case current(built: Date?)
    /// The server is publishing something newer.
    case stale(published: Date?)
    /// The server could not be reached. The cache may be perfectly good; we do
    /// not know, and saying so is better than implying either.
    case unreachable(built: Date?)
    /// The server answered, and the cache carries no tag to compare with it —
    /// so the two cannot be told apart. Distinct from `unreachable`, which
    /// blames the network: this is a local gap, and no amount of signal fixes
    /// it. A rebuild does.
    case unstamped(built: Date?)

    /// Whether a download is worth offering prominently. It is always allowed —
    /// this only decides how loudly to say so.
    public var urging: Bool {
        if case .stale = self { return true }
        return false
    }
}

/// What the server says it holds, learned without downloading it.
///
/// One request, no body. The entity tag is the whole answer: it changes when
/// the export changes and is compared as an opaque string, never parsed.
public struct Publication: Sendable, Equatable {
    public let etag: String
    public let modified: Date?

    public init(etag: String, modified: Date?) {
        self.etag = etag
        self.modified = modified
    }
}

extension Freshness {
    /// The verdict, given what the cache recorded and what the server answered.
    ///
    /// Pure, so the rule can be held by a test: the network is not the part
    /// that is easy to get wrong. An unknown tag on either side is not a
    /// mismatch — a cache built before this was recorded, or a server that
    /// stopped sending the header, must not be reported as out of date on no
    /// evidence.
    public static func compare(cached: String?, against published: Publication?, built: Date?)
        -> Freshness
    {
        guard let published else { return .unreachable(built: built) }
        // The server answered. Anything missing from here on is ours.
        guard let cached, !cached.isEmpty, !published.etag.isEmpty else {
            return .unstamped(built: built)
        }
        return cached == published.etag
            ? .current(built: built)
            : .stale(published: published.modified)
    }
}
