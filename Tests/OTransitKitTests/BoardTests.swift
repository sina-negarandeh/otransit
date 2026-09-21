// Merging a stop's timetable with what the feed says about it.

import Foundation
import Testing

@testable import OTransitKit

@Suite("Board")
struct BoardTests {
    /// 07:00 on a Monday in Ottawa, which fixes where the two clocks meet.
    private let clock = Clock(date: "2026-09-14", hhmm: "07:00")!

    private func departure(_ trip: String, at seconds: Int) -> Departure {
        Departure(
            trip: trip, route: "7", headsign: "Carleton", scheduled: seconds,
            afterMidnight: false, colour: "0057b8")
    }

    /// A feed putting `trip` at `stop` `offset` seconds off its timetable.
    private func live(_ trip: String, _ stop: String, scheduled: Int, off: Int) throws -> Realtime {
        let epoch = clock.epoch - clock.now + scheduled + off
        return try Realtime(
            json: Data(
                """
                {"Entity":[{"TripUpdate":{"Trip":{"TripId":"\(trip)"},
                  "StopTimeUpdate":[{"StopId":"\(stop)","Arrival":{"HasTime":1,"Time":\(epoch)}}]}}]}
                """.utf8))
    }

    @Test("with no feed at all, every row is the timetable")
    func noFeed() {
        let rows = Board.rows(
            from: [departure("t", at: 25200)], live: nil, stop: "s", clock: clock)
        #expect(rows.first?.status == .scheduled)
        #expect(rows.first?.at == 25200)
    }

    @Test("a trip the feed never named keeps its scheduled time")
    func notNamed() throws {
        let feed = try live("other", "s", scheduled: 25200, off: 0)
        let rows = Board.rows(
            from: [departure("t", at: 25200)], live: feed, stop: "s", clock: clock)
        #expect(rows.first?.status == .scheduled)
    }

    @Test("within a minute either way is on time")
    func slack() throws {
        // The feed reports to the second and a bus is not late by fifteen.
        for off in [-59, -15, 0, 15, 59] {
            let feed = try live("t", "s", scheduled: 25200, off: off)
            let rows = Board.rows(
                from: [departure("t", at: 25200)], live: feed, stop: "s", clock: clock)
            #expect(rows.first?.status == .onTime, "\(off)s should read on time")
        }
    }

    @Test("past the slack it is late or early, in whole minutes")
    func lateAndEarly() throws {
        let late = try live("t", "s", scheduled: 25200, off: 7 * 60)
        #expect(
            Board.rows(from: [departure("t", at: 25200)], live: late, stop: "s", clock: clock)
                .first?.status == .late(7))

        let early = try live("t", "s", scheduled: 25200, off: -3 * 60)
        #expect(
            Board.rows(from: [departure("t", at: 25200)], live: early, stop: "s", clock: clock)
                .first?.status == .early(3))
    }

    @Test("a predicted row is drawn at the predicted time, not the timetable's")
    func predictedTime() throws {
        let feed = try live("t", "s", scheduled: 25200, off: 7 * 60)
        let rows = Board.rows(
            from: [departure("t", at: 25200)], live: feed, stop: "s", clock: clock)
        #expect(rows.first?.at == 25200 + 7 * 60)
        #expect(rows.first?.status.isPredicted == true)
    }

    @Test("a cancelled trip keeps its scheduled time and is not predicted")
    func cancelled() throws {
        let feed = try Realtime(
            json: Data(
                """
                {"Entity":[{"TripUpdate":{"Trip":{"TripId":"t","ScheduleRelationship":3},
                  "StopTimeUpdate":[]}}]}
                """.utf8))
        let rows = Board.rows(
            from: [departure("t", at: 25200)], live: feed, stop: "s", clock: clock)
        #expect(rows.first?.status == .cancelled)
        #expect(rows.first?.at == 25200)
        #expect(rows.first?.status.isPredicted == false)
    }

    @Test("a late bus sorts after one the timetable put behind it")
    func reordering() throws {
        // The whole reason the sort happens after the merge: a prediction can
        // move a trip past the one in front of it, and a board left in
        // scheduled order would show the later bus first.
        let feed = try Realtime(
            json: Data(
                """
                {"Entity":[{"TripUpdate":{"Trip":{"TripId":"first"},
                  "StopTimeUpdate":[{"StopId":"s","Arrival":{"HasTime":1,
                  "Time":\(clock.epoch - clock.now + 25200 + 600)}}]}}]}
                """.utf8))
        let rows = Board.rows(
            from: [departure("first", at: 25200), departure("second", at: 25500)],
            live: feed, stop: "s", clock: clock)
        #expect(rows.map(\.trip) == ["second", "first"])
    }

    @Test("what each row says")
    func words() {
        #expect(Status.cancelled.text == "cancelled")
        #expect(Status.onTime.text == "on time")
        // No number: the board draws the promised time struck out beside the
        // expected one, so the minutes between them are already on the row.
        #expect(Status.late(7).text == "late")
        #expect(Status.early(3).text == "early")
        #expect(Status.scheduled.text == "scheduled")
    }
}

@Suite("Hearing")
struct HearingTests {
    private func feed() throws -> Realtime { try Realtime(json: Data("{\"Entity\":[]}".utf8)) }

    @Test("a poller with no key has nothing to say")
    func silent() {
        #expect(Poller.silent().hearing(at: 1_000) == Hearing.none)
    }

    @Test("a poller that has not answered yet says nothing rather than guessing")
    func unanswered() {
        // The first attempt is owed the instant the popover opens, so this
        // lasts a fraction of a second. Calling it live would be a claim about
        // an answer that has not arrived.
        #expect(Poller.polling(from: 1_000).hearing(at: 1_000) == Hearing.none)
    }

    @Test("an answer is live until it is two cadences old")
    func fresh() throws {
        var poller = Poller.polling(from: 1_000)
        poller.heard(try feed(), at: 1_000)
        #expect(poller.hearing(at: 1_000) == .live)
        #expect(poller.hearing(at: 1_050) == .live)
        #expect(poller.hearing(at: 1_051) == .stale(seconds: 51))
    }

    @Test("a refusal on its own does not make the board out of date")
    func refusedButCurrent() throws {
        // The age of what is on screen is the thing a reader is deciding about.
        // A refusal three seconds after a good answer leaves nothing stale.
        var poller = Poller.polling(from: 1_000)
        poller.heard(try feed(), at: 1_000)
        poller.refused("503", at: 1_003)
        #expect(poller.hearing(at: 1_003) == .live)
        // It goes stale by getting old, which it now will.
        #expect(poller.hearing(at: 1_200) == .stale(seconds: 200))
    }

    @Test("a fresh answer clears a stale one")
    func recovered() throws {
        var poller = Poller.polling(from: 1_000)
        poller.heard(try feed(), at: 1_000)
        #expect(poller.hearing(at: 1_400) == .stale(seconds: 400))
        poller.heard(try feed(), at: 1_400)
        #expect(poller.hearing(at: 1_400) == .live)
    }
}

@Suite("Struck times")
struct PromisedTests {
    private func arrival(scheduled: Int, at: Int, _ status: Status) -> Arrival {
        Arrival(
            trip: "t", route: "7", headsign: "Carleton", colour: "0057b8",
            at: at, scheduled: scheduled, status: status)
    }

    @Test("a row that is keeping the timetable strikes nothing out")
    func onTimeStrikesNothing() {
        // 11:19:50 scheduled, 11:20:50 predicted: one minute off, so on time —
        // and two different clock minutes. Deciding by the drawn minutes put a
        // struck-out time on a row whose status said it was being kept.
        #expect(arrival(scheduled: 41_990, at: 42_050, .onTime).promised == nil)
        #expect(arrival(scheduled: 42_000, at: 42_000, .onTime).promised == nil)
    }

    @Test("a row that is not keeping it shows what was promised")
    func lateAndEarlyStrike() {
        #expect(arrival(scheduled: 42_000, at: 42_180, .late(3)).promised == 42_000)
        #expect(arrival(scheduled: 42_000, at: 41_820, .early(3)).promised == 42_000)
    }

    @Test("a row with no prediction has one time, and a cancelled one is not a change")
    func nothingToCompare() {
        #expect(arrival(scheduled: 42_000, at: 42_000, .scheduled).promised == nil)
        #expect(arrival(scheduled: 42_000, at: 42_000, .cancelled).promised == nil)
    }
}

@Suite("Row order")
struct RowOrderTests {
    private func departure(_ trip: String, at: Int) -> Departure {
        Departure(
            trip: trip, route: "7", headsign: "Carleton", scheduled: at,
            afterMidnight: false, colour: "0057b8")
    }

    @Test("calls in the same second keep the order they arrived in")
    func tiesAreNotReordered() throws {
        // Cache.departures builds a total order and says so; a sort with no
        // stability guarantee would let two calls at one second swap places
        // between polls, and a board that reorders with no new data is broken.
        let same = (1...6).map { departure("t\($0)", at: 42_000) }
        let clock = Clock(at: .now)
        let once = Board.rows(from: same, live: nil, stop: "3011", clock: clock)
        #expect(once.map(\.trip) == ["t1", "t2", "t3", "t4", "t5", "t6"])

        // And the same answer every time it is asked.
        for _ in 0..<20 {
            #expect(
                Board.rows(from: same, live: nil, stop: "3011", clock: clock).map(\.trip)
                    == once.map(\.trip))
        }
    }

    @Test("a later call still sorts after an earlier one")
    func orderStillHolds() {
        let mixed = [departure("late", at: 43_000), departure("early", at: 42_000)]
        let rows = Board.rows(from: mixed, live: nil, stop: "3011", clock: Clock(at: .now))
        #expect(rows.map(\.trip) == ["early", "late"])
    }
}
