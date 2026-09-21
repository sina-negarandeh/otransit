// What counts as news about a route in a feed that is a content system, not a
// standard.
//
// Every case here is taken from the feed as it was actually published: the
// route lists written for a person to read, the night route tagged as its day
// route, the station notices that carry no route at all, and the live rail
// alert the operator files as a general message.

import Foundation
import Testing

@testable import OTransitKit

@Suite("Updates")
struct UpdatesTests {
    /// Five items, all of which are in the feed today.
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
            <title>O-Train Line 1 Alert</title>
            <link>https://www.octranspo.com/e/269950</link>
            <guid>https://www.octranspo.com/e/269950</guid>
            <pubDate>Mon, 21 Sep 2026 08:21:00 EDT</pubDate>
            <category>General Message</category>
            <category>affectedRoutes-1 O-Train</category>
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

    private func read() throws -> Updates {
        try Updates.read(Data(feed.utf8))
    }

    @Test("an item is kept when it names a route, and for no other reason")
    func picks() throws {
        let found = try read()
        // The two detours and the rail alert. Not the washrooms, whose
        // category is there and empty, and not the item with no categories.
        #expect(found.notices.count == 3)
        // Every item counted, so a feed that changed shape can be told from a
        // quiet week.
        #expect(found.items == 5)
    }

    @Test("a general message that names a route is news about that route")
    func alert() throws {
        // The item this rule exists for. The operator files live rail
        // limitations as general messages, and requiring the `Detours` heading
        // dropped every one of them: "service is operating on the eastbound
        // platforms only at Tremblay station" is not a reroute, and was being
        // discarded for not being one.
        let found = try read()
        let line1 = try #require(found.naming("1").first)
        #expect(line1.title == "O-Train Line 1 Alert")
        #expect(line1.kind == .alert)
        // And the heading still says which of the two an item is.
        #expect(found.naming("19").first?.kind == .detour)
    }

    @Test("rail is named with its number and its name, and matched on the number")
    func railToken() throws {
        // Buses are tagged `affectedRoutes-19, 40, 41` and rail is tagged
        // `affectedRoutes-1 O-Train`. No route in the timetable has a space in
        // its name, so the first word is the name.
        let found = try read()
        #expect(found.naming("1").count == 1)
        // And not under the whole string, which matches no timetable.
        #expect(found.naming("1 O-Train").isEmpty)
        #expect(found.naming("O-Train").isEmpty)
    }

    @Test("an alert outranks a detour where one mark stands for a route")
    func precedence() throws {
        let both = """
            <?xml version="1.0"?><rss><channel>
              <item><title>Detour: Somewhere</title>
                <category>Detours</category><category>affectedRoutes-5</category></item>
              <item><title>Route 5 Alert</title>
                <category>General Message</category><category>affectedRoutes-5</category></item>
            </channel></rss>
            """
        let found = try Updates.read(Data(both.utf8))
        // A limitation is happening now and the roadwork has been running
        // since spring, so the alert is the one mark a route list can show.
        #expect(found.kind(of: "5") == .alert)
        // Order does not decide it.
        let swapped = try Updates.read(
            Data(both.replacingOccurrences(of: "Detour: Somewhere", with: "Z").utf8))
        #expect(swapped.kind(of: "5") == .alert)
        // And the board shows both, the alert first.
        #expect(found.naming("5").map(\.kind) == [.alert, .detour])
        #expect(found.kind(of: "19") == nil)
    }

    @Test("two notices of one kind keep the order the feed published them in")
    func stableOrder() throws {
        // `naming` puts the alerts first, and a sort would be the obvious way
        // to write that. Swift's sort promises nothing about equal elements, so
        // two detours on one route would be free to swap places between one
        // poll and the next, and a board that reorders itself with no new data
        // looks broken.
        let two = """
            <?xml version="1.0"?><rss><channel>
              <item><title>Detour: First</title>
                <category>Detours</category><category>affectedRoutes-5</category></item>
              <item><title>Detour: Second</title>
                <category>Detours</category><category>affectedRoutes-5</category></item>
              <item><title>Route 5 Alert</title>
                <category>General Message</category><category>affectedRoutes-5</category></item>
            </channel></rss>
            """
        let found = try Updates.read(Data(two.utf8))
        #expect(
            found.naming("5").map(\.title) == ["Route 5 Alert", "Detour: First", "Detour: Second"])
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
        // nothing about 9.
        #expect(found.naming("4").isEmpty)
        #expect(found.naming("9").isEmpty)
        #expect(found.naming("441").isEmpty)
        // Route 1 is in this feed, and it is there because the rail alert
        // names it rather than because 19 begins with it.
        #expect(found.naming("1").map(\.title) == ["O-Train Line 1 Alert"])
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

    @Test("an item about no route is skipped")
    func stationNotice() throws {
        let found = try read()
        // A washroom closure carries the category with nothing after it, and a
        // cancelled trip carries no categories at all. Neither is about a
        // route, and the heading has nothing to do with it.
        #expect(!found.notices.contains { $0.title.contains("washrooms") })
        #expect(!found.notices.contains { $0.title.contains("nothing on it") })
        // An empty category must not read as a route.
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
            id: "x", kind: .detour, title: "Detour extended: Cheo Roadway closure",
            routes: ["5"], link: nil, published: nil)
        #expect(extended.headline == "Detour extended: Cheo Roadway closure")

        // A wording this does not know is left alone.
        let unknown = Notice(
            id: "y", kind: .detour, title: "[Updated] Something else", routes: ["5"],
            link: nil, published: nil)
        #expect(unknown.headline == "[Updated] Something else")
    }

    @Test("a feed that parses and matches nothing is not a quiet week")
    func renamedTags() throws {
        // The failure this whole design guards against: the content system
        // renames its tag, every item still parses, and nothing matches. On
        // screen that is identical to a week with no news.
        let renamed =
            feed
            .replacingOccurrences(of: "affectedRoutes-", with: "routesAffected-")
        let found = try Updates.read(Data(renamed.utf8))

        #expect(found.notices.isEmpty)
        #expect(found.items == 5)
        // Which is what `unreadable` says, and what `otransit updates` refuses
        // on. Without it the two states are indistinguishable.
        #expect(found.unreadable)

        // And a feed that really is quiet is not reported as broken.
        let quiet = try Updates.read(
            Data(
                "<?xml version=\"1.0\"?><rss><channel></channel></rss>".utf8))
        #expect(quiet.items == 0)
        #expect(!quiet.unreadable)
    }

    @Test("renaming the heading costs a mark and not an item")
    func renamedHeading() throws {
        // The heading used to be half the test, and renaming it hid every
        // detour in the feed. It now picks which mark is drawn, so the same
        // rename costs a glance: the items are all still here and all still
        // matched to their routes.
        let renamed =
            feed
            .replacingOccurrences(
                of: "<category>Detours</category>", with: "<category>Detour</category>")
        let found = try Updates.read(Data(renamed.utf8))

        #expect(found.notices.count == 3)
        #expect(!found.unreadable)
        #expect(found.naming("19").count == 1)
        // Drawn as alerts, which is the mark that claims least: it says there
        // is something to read, where a detour says the route goes elsewhere.
        #expect(found.notices.allSatisfy { $0.kind == .alert })
    }

    @Test("membership is answered without building an array")
    func names() throws {
        let found = try read()
        #expect(Set(found.affected.keys) == ["19", "40", "41", "39", "1"])
        #expect(found.names("19"))
        #expect(!found.names("4"))
        // The two answers cannot disagree.
        for route in ["19", "40", "41", "39", "1", "4", "N39", ""] {
            #expect(found.names(route) == !found.naming(route).isEmpty)
            #expect((found.kind(of: route) != nil) == found.names(route))
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
        let found = try Updates.read(Data(wrapped.utf8))
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
        let found = try Updates.read(Data(twins.utf8))
        #expect(found.notices.count == 2)
        #expect(found.notices[0].id != found.notices[1].id)
    }

    @Test("a feed that is not a feed is refused rather than read as empty")
    func refuses() {
        #expect(throws: Updates.Failure.self) {
            try Updates.read(Data("not xml at all <<<".utf8))
        }
    }
}
