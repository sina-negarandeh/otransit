// Every screen this program can be asked for by name, in one place.
//
// Two tools want them. Preview puts one in an ordinary window to look at while
// a layout is being worked on; Shot draws one off-screen and photographs it.
// They had a switch each, and the two had already drifted apart: the tool for
// looking could not show settings, staleness or any browsing screen, and the
// tool for photographing could not reach `long` — the four-crumb worst case
// that exists precisely because reaching it by clicking every time a layout
// changes is how a layout stops being checked.
//
// So the names live here and the drivers hold none. A screen added here is
// immediately available to both.
//
// The seams the states need — a Schedule stopped in one state, a key that is
// not on this machine, a sample to fill a field with — are supplied here too. A
// view has no business knowing what a demonstration key looks like.

import OTransitKit
import SwiftUI

@MainActor
enum Screens {
    /// Every name worth drawing, in the order a person meets them.
    static let all = [
        "first", "outdated", "checking", "downloading", "unpacking", "reading",
        "indexing", "failed", "app", "stale", "offline", "settings", "settings-off",
        "settings-shown", "long", "browse",
    ]

    /// Not a key. It is the right shape and belongs to nobody, which is what a
    /// screenshot needs: a real one must never end up in a picture.
    private static let sampleKey = "0123456789abcdef0123456789abcdef"

    /// The screens one name asks for.
    ///
    /// Every name but `browse` is a single screen; `browse` is the whole way
    /// down, which is four. A caller that wants one view takes the first.
    static func named(_ name: String) async throws -> [(String, AnyView)] {
        switch name {
        case "browse":
            return try await browsing()

        case "app":
            return [(name, AnyView(RootView(schedule: Schedule())))]

        case "settings", "settings-off", "settings-shown":
            // The key is the model's, so the state is asked for by handing the
            // model one rather than by telling the view what to pretend.
            let schedule = Schedule(
                showing: .missing(.never), key: name == "settings-off" ? nil : sampleKey)
            return [
                (
                    name,
                    AnyView(
                        chrome {
                            SettingsView(
                                back: {}, sample: name == "settings-shown" ? sampleKey : nil)
                        }
                        .environment(schedule))
                )
            ]

        case "stale", "offline":
            // Two states waiting will not produce: one needs the city to
            // republish and the other needs the network to be down.
            let cache = try Cache(path: Paths.cache)
            let day = ISO8601DateFormatter().date(from: "2026-09-18T01:49:31Z")
            let freshness: Freshness =
                name == "stale" ? .stale(published: day) : .unreachable(built: day)
            return [
                (
                    name,
                    AnyView(
                        RootView(schedule: Schedule(showing: .ready(cache), freshness: freshness)))
                )
            ]

        case "long":
            // The worst case the trail has: four crumbs ending in the longest
            // station name in the feed. It is here because that layout broke —
            // the crumbs demanded their full width, every parent obliged, and
            // the departure rows spilled out of both edges of the popover.
            let cache = try Cache(path: Paths.cache)
            let start = Browser(
                .board(
                    .rail,
                    Route(
                        shortName: "4", longName: "South Keys <> Airport ~ Aéroport",
                        colour: "a6228f"),
                    "Airport ~ Aéroport",
                    Stop(
                        id: "RE992", code: "3039",
                        name: "AIRPORT O-TRAIN NORTH / AÉROPORT O-TRAIN NORD", platform: "")))
            return [
                (
                    name,
                    AnyView(
                        chrome {
                            BrowseView(cache: cache, clock: Clock(at: .now), start: start)
                        })
                )
            ]

        default:
            guard let state = stopped(at: name) else { throw Missing(name) }
            return [(name, AnyView(RootView(schedule: Schedule(showing: state))))]
        }
    }

    /// No screen goes by that name.
    struct Missing: Error, CustomStringConvertible {
        let name: String
        init(_ name: String) { self.name = name }
        var description: String { "no such screen" }
    }

    /// The states worth looking at that are reached by waiting: the download is
    /// over in ten seconds and the failure needs the network to be down.
    private static func stopped(at name: String) -> Schedule.State? {
        switch name {
        case "first": .missing(.never)
        case "outdated": .missing(.outdated)
        case "checking": .building(.checking)
        case "downloading": .building(.downloading(received: 14_000_000, total: 31_000_000))
        case "unpacking": .building(.unpacking)
        case "reading": .building(.reading("stop_times.txt", rows: 2_250_000))
        case "indexing": .building(.indexing)
        case "failed": .failed("the server answered 503\n\nTry again in a few minutes.")
        default: nil
        }
    }

    /// Every browsing screen, drawn against the cache on disk.
    ///
    /// The path down is found and not written here. A hard-coded route is a
    /// picture that breaks the week the city reorganises its network, which it
    /// does twice a year — so this takes a route that is running today, the
    /// direction most of its trips take, and the first stop on that direction
    /// something is still due at. An empty board is a true picture of a stop
    /// after the last bus and a useless picture of the screen.
    private static func browsing() async throws -> [(String, AnyView)] {
        let cache = try Cache(path: Paths.cache)
        let clock = Clock(at: .now)
        let today = clock.date

        let routes = try await cache.routes(.bus, on: today)
        guard let route = routes.first(where: { $0.shortName == "7" }) ?? routes.first else {
            throw Missing("a bus route running today")
        }
        let directions = try await cache.directions(of: route.shortName, on: today)
        guard let toward = directions.first?.headsign else {
            throw Missing("a direction for route \(route.shortName)")
        }
        let stops = try await cache.stops(of: route.shortName, toward: toward, on: today)
        guard let stop = try await awaited(in: stops, cache, clock) ?? stops.first else {
            throw Missing("a stop on route \(route.shortName)")
        }

        // The board is the screen this whole program is for, and one drawn with
        // no predictions is the screen failing to show what it is for.
        var live: Realtime?
        if let key = Key.read() { live = try? await Feed.trips(key: key) }
        let hearing: Hearing = live == nil ? .none : .live

        return [
            ("browse-routes", Place.routes(.bus)),
            ("browse-direction", .direction(.bus, route)),
            ("browse-stops", .stops(.bus, route, toward)),
            ("browse-board", .board(.bus, route, toward, stop)),
        ].map { name, place in
            (
                name,
                AnyView(
                    chrome {
                        BrowseView(
                            cache: cache, clock: clock, live: live, hearing: hearing,
                            start: Browser(place))
                    })
            )
        }
    }

    /// The first of these stops something is still due at. Nil once the day is
    /// over everywhere on the route, which is a real answer and not a failure.
    ///
    /// Bounded, because each of these is a two-service-day scan of three and a
    /// half million rows. The stop wanted is almost always the first or second;
    /// walking all fifty to prove that none of them qualifies costs far more
    /// than falling back to the first stop and drawing an empty board.
    private static let looksAt = 8

    private static func awaited(in stops: [Stop], _ cache: Cache, _ clock: Clock) async throws
        -> Stop?
    {
        for stop in stops.prefix(looksAt) {
            let due = try await cache.departures(
                at: stop.id, on: clock.date, after: clock.yesterday)
            if due.contains(where: { $0.scheduled >= clock.now }) { return stop }
        }
        return nil
    }

    /// The bars the real popover puts around a screen that is not the whole of
    /// it. RootView draws its own; these are for the screens shown beneath it.
    private static func chrome(@ViewBuilder _ content: () -> some View) -> some View {
        VStack(spacing: 0) {
            content()
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            Rule()
            QuitRow()
        }
    }
}
