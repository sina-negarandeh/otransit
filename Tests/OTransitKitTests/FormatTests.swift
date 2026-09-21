// The colour rule, and the two ways a time is written.
//
// The expected strings are the Rust and Go ports' own, taken from their tests.
// Three programs reading one feed should not disagree about what 66 minutes is
// called, so this suite is where that is held.

import Testing

@testable import OTransitKit

@Suite("Format")
struct FormatTests {
    @Test("a service day past midnight is written as a clock reads it")
    func wrapping() {
        #expect(Format.hhmm(25200) == "07:00")
        // 25:10, which is what a person catches at ten past one.
        #expect(Format.hhmm(90600) == "01:10")
        #expect(Format.hhmm(0) == "00:00")
        #expect(Format.hhmm(23 * 3600 + 59 * 60) == "23:59")
    }

    @Test("a wait never looks like a clock time")
    func waitForms() {
        #expect(Format.wait(minutes: 0) == "due")
        #expect(Format.wait(minutes: -3) == "due")
        #expect(Format.wait(minutes: 27) == "27 min")
        #expect(Format.wait(minutes: 59) == "59 min")
        #expect(Format.wait(minutes: 86) == "1 hr 26 min")
        #expect(Format.wait(minutes: 195) == "3 hr 15 min")
    }

    @Test("past an hour the minutes are zero-padded")
    func padding() {
        #expect(Format.wait(minutes: 66) == "1 hr 06 min")
        #expect(Format.wait(minutes: 60) == "1 hr 00 min")
    }

    @Test("a right-aligned column of waits does not stagger")
    func column() {
        // The bug this is here for: "1h 6 min" is a place narrower than
        // "1h 36 min", so right-aligning drags its "h" one place over and the
        // column reads as a ragged edge. Zero padding is what makes every hour
        // form the same shape.
        let width = 12
        let rows = [0, 2, 27, 59, 60, 66, 86, 195, 999].map { minutes in
            String(repeating: " ", count: max(0, width - Format.wait(minutes: minutes).count))
                + Format.wait(minutes: minutes)
        }
        for row in rows {
            #expect(row.count == width, "\(row) is not one column wide")
        }

        func places(of needle: String) -> [Int] {
            rows.compactMap { row in
                row.range(of: needle).map { row.distance(from: row.startIndex, to: $0.lowerBound) }
            }
        }
        // Every "h" in one place, every " min" in another.
        #expect(Set(places(of: " hr ")).count == 1)
        #expect(Set(places(of: " min")).count == 1)
    }

    @Test("seconds round to the nearest minute, both ways")
    func rounding() {
        #expect(Format.minutes(0) == 0)
        #expect(Format.minutes(29) == 0)
        #expect(Format.minutes(30) == 1)
        #expect(Format.minutes(89) == 1)
        #expect(Format.minutes(90) == 2)
        // Away from zero on both sides, so a wait and the same wait counted
        // backwards agree about how long it is.
        #expect(Format.minutes(-30) == -1)
        #expect(Format.minutes(-29) == 0)
    }

    @Test("the colour rule is red at two minutes and dim past fifteen")
    func urgency() {
        #expect(Urgency(minutes: -1) == .gone)
        #expect(Urgency(minutes: 0) == .imminent)
        #expect(Urgency(minutes: 2) == .imminent)
        #expect(Urgency(minutes: 3) == .soon)
        #expect(Urgency(minutes: 6) == .soon)
        #expect(Urgency(minutes: 7) == .comfortable)
        #expect(Urgency(minutes: 15) == .comfortable)
        #expect(Urgency(minutes: 16) == .later)
    }

    @Test("the words and the colour are built from the same minutes")
    func agreement() {
        // 119 seconds and 121 seconds both read "2 min". They must also be the
        // same colour, which they are only because both are taken from the
        // rounded minute rather than one from each.
        for seconds in [119, 121] {
            let minutes = Format.minutes(seconds)
            #expect(Format.wait(minutes: minutes) == "2 min")
            #expect(Urgency(minutes: minutes) == .imminent)
        }
    }
}

@Suite("Counting")
struct CountTests {
    @Test("one of a thing is not plural")
    func agreement() {
        // Route 88 toward Franco-Ouest runs once a day, and the direction
        // screen said "1 trips today".
        #expect(Format.count(1, "trip") == "1 trip")
        #expect(Format.count(0, "trip") == "0 trips")
        #expect(Format.count(2, "trip") == "2 trips")
        #expect(Format.count(166, "route") == "166 routes")
        #expect(Format.count(1, "line") == "1 line")
    }
}
