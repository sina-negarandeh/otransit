// How a time and a wait are written, and how urgent a wait is.
//
// The colour is the interface. How long you have is red at two minutes or
// less, amber at six, green at fifteen, and dim past that, so the board answers
// "do I need to leave now" before a number is read.
//
// Every rule here is the one the Rust and Go ports already settled, down to the
// zero padding and the word "due". Three programs reading one feed should not
// disagree about what 66 minutes is called, and the two that came first have
// the better-tested answer.

import Foundation

/// How much time there is, in the only amounts that change what a person does:
/// run, leave, finish the paragraph, or stop looking.
public enum Urgency: Sendable, Equatable {
    case gone
    case imminent
    case soon
    case comfortable
    case later

    /// The bucket a wait falls in, in whole minutes.
    ///
    /// Minutes and not seconds, and the same minutes the text is built from:
    /// computed apart, a row reading "2 min" could be painted amber while the
    /// row above it reading "2 min" was painted red, because one was 119
    /// seconds and the other 121.
    public init(minutes: Int) {
        self =
            switch minutes {
            case ..<0: .gone
            case ...2: .imminent
            case ...6: .soon
            case ...15: .comfortable
            default: .later
            }
    }
}

public enum Format {
    /// A count and the thing counted, in agreement.
    ///
    /// Route 88 toward Franco-Ouest runs once, and the board said "1 trips
    /// today". Every number on these screens comes out of a query and any of
    /// them can be one: a route with a single direction, a line with a single
    /// trip left.
    ///
    /// English only, and only the regular plural, because that is the whole of
    /// what this app writes. A word this cannot make plural should not be
    /// handed to it.
    public static func count(_ n: Int, _ thing: String) -> String {
        "\(n) \(thing)\(n == 1 ? "" : "s")"
    }

    /// A time on the service day, written as a clock reads it.
    ///
    /// The day wraps: a schedule reaches 28:xx, and 25:10 is written 01:10
    /// because that is what a clock on the wall says when the bus arrives.
    public static func hhmm(_ secondsOnServiceDay: Int) -> String {
        let wrapped = ((secondsOnServiceDay % 86400) + 86400) % 86400
        return String(format: "%02d:%02d", wrapped / 3600, (wrapped % 3600) / 60)
    }

    /// Seconds as whole minutes, rounded to nearest.
    ///
    /// Rounded rather than truncated, and away from zero on both sides, so a
    /// wait and the same wait counted backwards agree about how long it is.
    public static func minutes(_ seconds: Int) -> Int {
        seconds < 0 ? -((-seconds + 30) / 60) : (seconds + 30) / 60
    }

    /// "due", "7 min", "1 hr 26 min", "16 hr 40 min".
    ///
    /// Deliberately not "1:26": this sits beside actual clock times, and a
    /// duration that looks like a time is a misread waiting to happen.
    ///
    /// Past an hour the minutes are always spelled out and zero-padded, so
    /// every hour form is the same shape. The column is right-aligned, so any
    /// string whose length follows its own contents drags the "h" along with
    /// it: "1 hr 6 min" lands a place right of "1 hr 36 min", and a bare "4 hr"
    /// is flung to the far edge with a hole under the minutes.
    ///
    /// The hour itself is not padded: right-aligning already puts the "hr" of a
    /// one-digit hour under the "hr" of a two-digit one, and "04 hr" would read
    /// as a clock.
    ///
    /// A vehicle leaving this minute is not a quantity of time, and neither is
    /// one that went while you were reading. Both are due.
    public static func wait(minutes: Int) -> String {
        switch minutes {
        case ..<1: "due"
        case ..<60: "\(minutes) min"
        default: String(format: "%d hr %02d min", minutes / 60, minutes % 60)
        }
    }
}
