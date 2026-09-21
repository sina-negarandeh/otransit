// Whether the cache is the export being published, and what is said when that
// cannot be established.
//
// The rule is held here rather than in the fetch, because the fetch is a
// wrapper around URLSession and this is the part with a decision in it. The
// failure it guards against is the quiet one: a stale cache draws a full board
// of scheduled times and looks exactly like a working program.

import Foundation
import Testing

@testable import OTransitKit

@Suite("Freshness")
struct FreshnessTests {
    private let built = Date(timeIntervalSince1970: 1_700_000_000)
    private let issued = Date(timeIntervalSince1970: 1_700_100_000)

    @Test("the same tag on both sides is a cache that is current")
    func matching() {
        let said = Publication(etag: "0x8DF1527158B027F", modified: issued)
        #expect(
            Freshness.compare(cached: "0x8DF1527158B027F", against: said, built: built)
                == .current(built: built))
    }

    @Test("a different tag is a newer export, named by when it was published")
    func differing() {
        // The real pair, from the day a board spent six days showing every bus
        // as scheduled: 53 of 327 live trips could be found in the cache.
        let said = Publication(etag: "0x8DF1527158B027F", modified: issued)
        #expect(
            Freshness.compare(cached: "0x8DF0ED386470907", against: said, built: built)
                == .stale(published: issued))
    }

    @Test("a server that did not answer leaves the cache unjudged")
    func silent() {
        // Not stale. A café with bad wifi is not evidence about OC Transpo, and
        // a row reading "out of date" on no evidence sends someone to download
        // 30 MB they already have.
        #expect(
            Freshness.compare(cached: "0x8DF1527158B027F", against: nil, built: built)
                == .unreachable(built: built))
    }

    @Test("a missing tag on either side is not a mismatch, and not the network")
    func absent() {
        // The server answered in every one of these. Reporting them as
        // unreachable blamed a working network for a local gap, and did it
        // permanently: no amount of signal ever supplies a tag the cache does
        // not carry. A rebuild does.
        let untagged = Publication(etag: "", modified: issued)
        #expect(
            Freshness.compare(cached: "0x8DF1", against: untagged, built: built)
                == .unstamped(built: built))
        // A cache built before the tag was recorded, which is every cache an
        // older version of this program wrote.
        let said = Publication(etag: "0x8DF1", modified: issued)
        #expect(
            Freshness.compare(cached: nil, against: said, built: built)
                == .unstamped(built: built))
        #expect(
            Freshness.compare(cached: "", against: said, built: built)
                == .unstamped(built: built))
    }

    @Test("a silent server is the only thing reported as unreachable")
    func onlySilence() {
        #expect(
            Freshness.compare(cached: "0x8DF1", against: nil, built: built)
                == .unreachable(built: built))
        #expect(
            Freshness.compare(cached: nil, against: nil, built: built)
                == .unreachable(built: built))
    }

    @Test("only a newer export asks to be acted on")
    func urging() {
        #expect(Freshness.stale(published: issued).urging)
        #expect(!Freshness.current(built: built).urging)
        #expect(!Freshness.unreachable(built: built).urging)
        #expect(!Freshness.unstamped(built: built).urging)
        #expect(!Freshness.checking.urging)
        #expect(!Freshness.unknown.urging)
    }

    @Test("the date a server writes is read against a fixed locale")
    func httpDate() {
        // The format is one fixed spelling in one fixed language. Read with the
        // machine's own locale it returns nil on a Mac set to, say, French,
        // and the row silently loses the date it was built to carry.
        let parsed = Feed.httpDate("Fri, 18 Sep 2026 01:49:31 GMT")
        #expect(parsed != nil)
        var utc = Calendar(identifier: .gregorian)
        utc.timeZone = TimeZone(identifier: "GMT")!
        let parts = utc.dateComponents([.year, .month, .day, .hour], from: parsed!)
        #expect(parts.year == 2026)
        #expect(parts.month == 9)
        #expect(parts.day == 18)
        #expect(parts.hour == 1)
        #expect(Feed.httpDate(nil) == nil)
        #expect(Feed.httpDate("last tuesday") == nil)
    }
}
