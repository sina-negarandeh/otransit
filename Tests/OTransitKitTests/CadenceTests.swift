// When something is owed again, and how far each refusal pushes it out.
//
// Held here rather than only through the poller that uses it, because two
// things now ask on a schedule — the realtime feed every twenty-five seconds
// and the freshness check every quarter of an hour — and the arithmetic they
// share is the part worth pinning down.

import Testing

@testable import OTransitKit

@Suite("Cadence")
struct CadenceTests {
    private let start = 1_700_000_000

    @Test("a silent cadence is never owed")
    func silent() {
        let quiet = Cadence.silent()
        #expect(!quiet.owed(at: start))
        // Not merely far off: no clock reaches it.
        #expect(!quiet.owed(at: .max - 1))
        #expect(quiet.dueIn(at: start) == 0)
    }

    @Test("the first attempt is owed at once")
    func immediate() {
        // Something that has just started has never asked, and the first
        // answer is the one a person is waiting on.
        let asking = Cadence.every(25, backingOffFrom: 60, from: start)
        #expect(asking.owed(at: start))
        #expect(asking.dueIn(at: start) == 0)
    }

    @Test("an answer sets the next attempt a cadence after it arrived")
    func answered() {
        var asking = Cadence.every(25, backingOffFrom: 60, from: start)
        // Arrived four seconds after it fell due: the next one is timed from
        // the arrival, not from the moment it was owed, so the cadence cannot
        // drift into the past.
        asking.answered(at: start + 4)
        #expect(!asking.owed(at: start + 28))
        #expect(asking.owed(at: start + 29))
        #expect(asking.dueIn(at: start + 4) == 25)
    }

    @Test("each refusal doubles the wait, and the doubling stops")
    func backoff() {
        var asking = Cadence.every(25, backingOffFrom: 60, from: start)
        asking.refused(at: start)
        #expect(asking.dueIn(at: start) == 60)
        asking.refused(at: start)
        #expect(asking.dueIn(at: start) == 120)
        asking.refused(at: start)
        #expect(asking.dueIn(at: start) == 240)

        // Doubling without limit puts the next attempt years away. Eight
        // failures is about two hours from a minute, and it stops there.
        for _ in 0..<20 { asking.refused(at: start) }
        #expect(asking.dueIn(at: start) == 60 << 7)
    }

    @Test("one answer puts the cadence back where it was")
    func recovery() {
        var asking = Cadence.every(25, backingOffFrom: 60, from: start)
        for _ in 0..<5 { asking.refused(at: start) }
        #expect(asking.dueIn(at: start) > 25)
        // Not half the backoff: a feed that answered is not a feed recovering.
        asking.answered(at: start)
        #expect(asking.dueIn(at: start) == 25)
        asking.refused(at: start)
        #expect(asking.dueIn(at: start) == 60)
    }
}
