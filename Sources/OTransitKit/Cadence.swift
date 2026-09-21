// When to ask again, and how much longer to wait each time the answer does not
// come.
//
// A model, not a timer. It is told the time and answers whether an attempt is
// owed; something else owns the clock and the network. That is what lets a
// cadence and a backoff be tested without waiting for either.
//
// Two things in this program ask a server the same shape of question on a
// schedule: the realtime poller, every twenty-five seconds, and the freshness
// check, every quarter of an hour. They had two implementations of this, in two
// time conventions — one in seconds since 1970 inside Poller, one in Dates and
// TimeIntervals spread across four fields of Schedule. This is the one both
// use, and it is the only place the arithmetic lives.

import Foundation

public struct Cadence: Sendable {
    /// Whether anything asks at all.
    private let asks: Bool
    /// Seconds between answers, once one arrives.
    private let every: Int
    /// What the first refusal costs. Each refusal after it doubles the wait.
    private let backoff: Int
    /// When the next attempt is owed, in seconds since 1970.
    private var due: Int
    /// Refusals since the last answer, which is what the backoff doubles on.
    private var failures = 0

    /// A cadence that never comes due. A program with no subscription key
    /// holds one of these, and asks nothing of anyone.
    public static func silent() -> Cadence {
        Cadence(asks: false, every: 0, backoff: 0, due: .max)
    }

    /// A cadence owed at once and every `seconds` after an answer.
    ///
    /// Owed at once because something that has just started has never asked,
    /// and the first answer is the one a person is waiting on.
    public static func every(_ seconds: Int, backingOffFrom backoff: Int, from now: Int) -> Cadence
    {
        Cadence(asks: true, every: seconds, backoff: backoff, due: now)
    }

    /// Whether an attempt has come due.
    public func owed(at now: Int) -> Bool { asks && now >= due }

    /// Seconds until the next attempt, or zero when one is owed.
    public func dueIn(at now: Int) -> Int { asks ? max(0, due - now) : 0 }

    /// Records an answer and works out when to ask again.
    ///
    /// Timed from the moment it arrived rather than the moment it fell due: an
    /// answer can only be had after it arrives, so the cadence starts from
    /// there rather than drifting into the past.
    public mutating func answered(at now: Int) {
        failures = 0
        due = now + every
    }

    /// Records what an attempt came back with, at the moment it settled.
    ///
    /// Both callers timed this from the answer rather than from the asking and
    /// both wrote the same four lines to do it. A check that failed must not
    /// buy the silence a good one does.
    public mutating func settled(answered: Bool, at now: Int) {
        if answered { self.answered(at: now) } else { refused(at: now) }
    }

    /// Records a refusal. The next attempt backs off, and each refusal after
    /// the first doubles the wait.
    public mutating func refused(at now: Int) {
        failures += 1
        // Doubling without limit puts the next attempt years away after a long
        // outage. Eight failures is about two hours from a minute, which is as
        // far as backing off is worth.
        due = now + backoff << min(failures - 1, 7)
    }
}
