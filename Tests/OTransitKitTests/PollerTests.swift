// When the poller asks, and what a refusal costs.

import Foundation
import Testing

@testable import OTransitKit

@Suite("Poller")
struct PollerTests {
    private func feed() throws -> Realtime { try Realtime(json: Data("{\"Entity\":[]}".utf8)) }

    @Test("a poller with no key never asks")
    func silent() {
        let poller = Poller.silent()
        #expect(!poller.owed(at: 0))
        #expect(!poller.owed(at: 1_000_000_000))
    }

    @Test("a program that has just started has never asked")
    func firstIsOwedAtOnce() {
        #expect(Poller.polling(from: 1000).owed(at: 1000))
    }

    @Test("an answer sets the cadence from when it arrived")
    func cadence() throws {
        var poller = Poller.polling(from: 1000)
        // Asked at 1000, answered at 1004: the next is owed 25s after the
        // answer, not after the attempt, or the cadence drifts into the past.
        poller.heard(try feed(), at: 1004)
        #expect(!poller.owed(at: 1028))
        #expect(poller.owed(at: 1029))
    }

    @Test("each refusal doubles the wait")
    func backoff() {
        var poller = Poller.polling(from: 0)
        poller.refused("timed out", at: 0)
        #expect(poller.dueIn(at: 0) == 60)
        poller.refused("timed out", at: 60)
        #expect(poller.dueIn(at: 60) == 120)
        poller.refused("timed out", at: 180)
        #expect(poller.dueIn(at: 180) == 240)
    }

    @Test("backing off stops doubling before the next attempt is years away")
    func backoffCeiling() {
        var poller = Poller.polling(from: 0)
        for _ in 0..<40 { poller.refused("timed out", at: 0) }
        // Eight doublings of a minute is about two hours, which is as far as
        // backing off is worth.
        #expect(poller.dueIn(at: 0) == 60 << 7)
    }

    @Test("one answer puts the cadence back, not half the backoff")
    func recovery() throws {
        // A feed that answered is not a feed recovering.
        var poller = Poller.polling(from: 0)
        poller.refused("timed out", at: 0)
        poller.refused("timed out", at: 60)
        poller.heard(try feed(), at: 180)
        #expect(poller.dueIn(at: 180) == Poller.cadence)
        #expect(poller.refusal == nil)
    }

    @Test("the last good feed is kept while the endpoint is down")
    func keepsStaleFeed() throws {
        // A stale prediction carrying an honest age beats no prediction.
        var poller = Poller.polling(from: 0)
        poller.heard(try feed(), at: 0)
        poller.refused("timed out", at: 25)
        #expect(poller.live != nil)
        #expect(poller.refusal == "timed out")
    }
}
