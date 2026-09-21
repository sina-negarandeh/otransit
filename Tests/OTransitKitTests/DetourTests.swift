// What counts as a detour in a feed that is a content system, not a standard.
//
// Every case here is taken from the feed as it was actually published: the
// route lists written for a person to read, the night route tagged as its day
// route, the general messages that carry no route at all.

import Foundation
import Testing

@testable import OTransitKit

@Suite("Detours")
struct DetourTests {
    /// Four items, three of which are in the feed today.
    private let feed = """
        <?xml version="1.0" encoding="UTF-8"?>
        <rss version="2.0"><channel>
          <item>
            <title>DETOUR: Routes 19 Parliament, 40 Greenboro and 41 Billings Bridge</title>
            <link>https://www.octranspo.com/en/updates/19-40-41</link>
            <guid>updates/19-40-41</guid>
            <pubDate>Sat, 20 Sep 2026 09:15:00 EDT</pubDate>
            <description><![CDATA[<p>Long prose, about 2,800 characters of it.</p>]]></description>
            <category>Detours</category>
            <category>affectedRoutes-19, 40, 41</category>
          </item>
          <item>
            <title>DETOUR: Route N39 Rideau near Belfast</title>
            <link>https://www.octranspo.com/en/updates/n39</link>
            <guid>updates/n39</guid>
            <pubDate>Fri, 19 Sep 2026 14:02:00 EDT</pubDate>
            <category>Detours</category>
            <category>affectedRoutes-39</category>
          </item>
          <item>
            <title>Public washrooms: Bayview Station</title>
            <link>https://www.octranspo.com/en/updates/washrooms</link>
            <guid>updates/washrooms</guid>
            <pubDate>Sun, 06 Sep 2026 11:39:00 EDT</pubDate>
            <category>General Message</category>
            <category></category>
          </item>
          <item>
            <title>An item with nothing on it</title>
            <guid>updates/bare</guid>
          </item>
        </channel></rss>
        """

    private func read() throws -> Detours {
        try Detours.read(Data(feed.utf8))
    }

    @Test("a detour is an item tagged both ways, and nothing else is")
    func picks() throws {
        let found = try read()
        #expect(found.notices.count == 2)
        // Every item counted, so a feed that changed shape can be told from a
        // week with no detours.
        #expect(found.items == 4)
    }

    @Test("the route list is read the way a person wrote it")
    func routes() throws {
        // `affectedRoutes-19, 40, 41` carries the spaces a reader expects.
        let notice = try #require(try read().notices.first)
        #expect(notice.routes == ["19", "40", "41"])
    }

    @Test("a route is on a list or it is not")
    func matching() throws {
        let found = try read()
        #expect(found.naming("19").count == 1)
        #expect(found.naming("40").count == 1)
        // A list holding 4 says nothing about 44, and one holding 19 says
        // nothing about 1 or 9.
        #expect(found.naming("4").isEmpty)
        #expect(found.naming("1").isEmpty)
        #expect(found.naming("9").isEmpty)
        #expect(found.naming("441").isEmpty)
    }

    @Test("a night route tagged as its day route is shown as published")
    func nightRoute() throws {
        // The feed titles this N39 and tags it `affectedRoutes-39`. The tag is
        // the only field a program can read and it is wrong, so 39 matches.
        // Nothing here can fix that. What the screen can do is print the title
        // the feed wrote, which says N39, so a reader sees what a matcher
        // cannot.
        let found = try read()
        let matched = found.naming("39")
        #expect(matched.count == 1)
        #expect(matched.first?.title.contains("N39") == true)
        // And N39 itself is not tagged, so asking for it finds nothing.
        #expect(found.naming("N39").isEmpty)
    }

    @Test("a general message carries no route and is skipped")
    func generalMessage() throws {
        let found = try read()
        #expect(!found.notices.contains { $0.title.contains("washrooms") })
        // An empty category beside it must not read as a route.
        #expect(found.naming("").isEmpty)
    }

    @Test("what a notice carries through")
    func fields() throws {
        let notice = try #require(try read().notices.first)
        #expect(notice.id == "updates/19-40-41")
        #expect(notice.link?.absoluteString == "https://www.octranspo.com/en/updates/19-40-41")
        // The prefix stays. It is sometimes the only thing that says a detour
        // was extended rather than started.
        #expect(notice.title.hasPrefix("DETOUR:"))

        var utc = Calendar(identifier: .gregorian)
        utc.timeZone = TimeZone(identifier: "GMT")!
        let day = utc.dateComponents([.year, .month, .day], from: try #require(notice.published))
        #expect(day.year == 2026)
        #expect(day.month == 9)
        #expect(day.day == 20)
    }

    @Test("the label the icon already carries comes off the front")
    func headline() throws {
        let found = try read()
        // `DETOUR:` says nothing a detour icon does not.
        #expect(
            found.notices[0].headline == "Routes 19 Parliament, 40 Greenboro and 41 Billings Bridge"
        )
        // The full title stays available, for the tooltip and for anything that
        // needs what was actually published.
        #expect(found.notices[0].title.hasPrefix("DETOUR:"))

        // "extended" is the only word that says a detour has been running
        // rather than starting, so that spelling keeps its first word.
        let extended = Notice(
            id: "x", title: "Detour extended: Cheo Roadway closure", routes: ["5"],
            link: nil, published: nil)
        #expect(extended.headline == "Detour extended: Cheo Roadway closure")

        // A wording this does not know is left alone.
        let unknown = Notice(
            id: "y", title: "[Updated] Something else", routes: ["5"], link: nil, published: nil)
        #expect(unknown.headline == "[Updated] Something else")
    }

    @Test("a feed that parses and matches nothing is not a quiet week")
    func renamedTags() throws {
        // The failure this whole design guards against: the content system
        // renames its tag, every item still parses, and nothing matches. On
        // screen that is identical to a week with no detours.
        let renamed =
            feed
            .replacingOccurrences(
                of: "<category>Detours</category>", with: "<category>Detour</category>")
        let found = try Detours.read(Data(renamed.utf8))

        #expect(found.notices.isEmpty)
        #expect(found.items == 4)
        // Which is what `unreadable` says, and what `otransit detours` refuses
        // on. Without it the two states are indistinguishable.
        #expect(found.unreadable)

        // And a feed that really is quiet is not reported as broken.
        let quiet = try Detours.read(
            Data(
                "<?xml version=\"1.0\"?><rss><channel></channel></rss>".utf8))
        #expect(quiet.items == 0)
        #expect(!quiet.unreadable)
    }

    @Test("membership is answered without building an array")
    func names() throws {
        let found = try read()
        #expect(found.affected == ["19", "40", "41", "39"])
        #expect(found.names("19"))
        #expect(!found.names("4"))
        // The two answers cannot disagree.
        for route in ["19", "40", "41", "39", "4", "N39", ""] {
            #expect(found.names(route) == !found.naming(route).isEmpty)
        }
    }

    @Test("a title in CDATA reads the same as a title in text")
    func cdata() throws {
        // RSS generators commonly wrap a title. Read through the wrong
        // delegate method it comes back empty, and the row draws an icon
        // beside nothing.
        let wrapped = feed.replacingOccurrences(
            of: "<title>DETOUR: Route N39 Rideau near Belfast</title>",
            with: "<title><![CDATA[DETOUR: Route N39 Rideau near Belfast]]></title>")
        let found = try Detours.read(Data(wrapped.utf8))
        #expect(found.naming("39").first?.title == "DETOUR: Route N39 Rideau near Belfast")
    }

    @Test("two notices without a guid do not share an identity")
    func identity() throws {
        // A ForEach over two rows with one id is undefined behaviour, and the
        // feed publishes near-identical wording for a detour and its extension.
        let twins = """
            <?xml version="1.0"?><rss><channel>
              <item><title>Detour: Somewhere</title>
                <category>Detours</category><category>affectedRoutes-5</category></item>
              <item><title>Detour: Somewhere</title>
                <category>Detours</category><category>affectedRoutes-5</category></item>
            </channel></rss>
            """
        let found = try Detours.read(Data(twins.utf8))
        #expect(found.notices.count == 2)
        #expect(found.notices[0].id != found.notices[1].id)
    }

    @Test("a feed that is not a feed is refused rather than read as empty")
    func refuses() {
        #expect(throws: Detours.Failure.self) {
            try Detours.read(Data("not xml at all <<<".utf8))
        }
    }
}
