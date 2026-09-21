// The realtime poller: when it asks, what it keeps, and what it says for itself.
//
// A model, not a timer. It is told the time and answers whether an attempt is
// owed; something else owns the clock and the network. That is what lets the
// cadence and the backoff be tested without waiting twenty-five seconds, and it
// is how both other ports are built.

import Foundation

public struct Poller: Sendable {
    /// How often an answer is asked for, in seconds.
    ///
    /// Public because it is also how often the screen wants waking: the clock
    /// on the board keeps this pace whether or not the endpoint is answering.
    public static let cadence = 25
    /// What a refusal costs. Each refusal after the first doubles it.
    static let backoff = 60

    /// The last answer that arrived.
    ///
    /// It stays while the feed is down: a stale prediction carrying an honest
    /// age beats no prediction, and the age is on screen for a person to judge.
    public private(set) var live: Realtime?
    /// What the last attempt said when it was refused, if it was.
    public private(set) var refusal: String?
    /// When the last answer arrived, in seconds since 1970.
    public private(set) var heardAt: Int?

    /// When to ask again. The arithmetic is Cadence's; what is kept between
    /// attempts is this type's.
    private var when: Cadence
    /// Whether anything asks at all. A program with no subscription key never
    /// does, and every time on screen is a scheduled one.
    private let asks: Bool

    /// A poller with no key: it never asks.
    public static func silent() -> Poller { Poller(when: .silent(), asks: false) }

    /// A poller with a key. The first attempt is owed at once, because a
    /// program that has just started has never asked.
    public static func polling(from now: Int) -> Poller {
        Poller(when: .every(cadence, backingOffFrom: backoff, from: now), asks: true)
    }

    private init(when: Cadence, asks: Bool) {
        self.when = when
        self.asks = asks
    }

    /// Whether an attempt has come due.
    public func owed(at now: Int) -> Bool { when.owed(at: now) }

    /// Records an answer and works out when to ask again.
    ///
    /// Recorded at the moment it arrived rather than the moment it fell due: an
    /// answer can only be had after it arrives, so the cadence starts from
    /// there rather than drifting into the past.
    public mutating func heard(_ feed: Realtime, at now: Int) {
        live = feed
        refusal = nil
        heardAt = now
        when.answered(at: now)
    }

    /// Records a refusal. The next attempt backs off, and each refusal after
    /// the first doubles the wait — but the last good feed is kept.
    public mutating func refused(_ why: String, at now: Int) {
        refusal = why
        when.refused(at: now)
    }

    /// Seconds until the next attempt, or zero when one is owed.
    public func dueIn(at now: Int) -> Int { when.dueIn(at: now) }

    /// How well the feed is being heard, for the mark that says so.
    ///
    /// The threshold is on the age of the answer and not on whether the last
    /// attempt failed, because the age is what a reader is deciding about: one
    /// refusal three seconds after a good answer leaves nothing on screen that
    /// is out of date, and saying otherwise would be alarming about nothing.
    public func hearing(at now: Int) -> Hearing {
        guard asks, let heardAt else { return .none }
        let age = now - heardAt
        // Two cadences. One missed attempt is a hiccup; two is the endpoint.
        return age <= Self.cadence * 2 ? .live : .stale(seconds: age)
    }
}

/// What the board can say for itself about the feed behind it.
public enum Hearing: Sendable, Equatable {
    /// Nothing asks, or nothing has answered yet. No key is the usual reason.
    case none
    /// An answer arrived recently enough that the board is current.
    case live
    /// The last good answer is this old and the board is still showing it.
    /// Kept deliberately — a stale prediction with an honest age beats none.
    case stale(seconds: Int)
}
