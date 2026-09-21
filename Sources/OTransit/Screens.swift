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
        "settings-shown", "long", "detour", "kept", "browse",
    ]

    /// Not a key. It is the right shape and belongs to nobody, which is what a
    /// screenshot needs: a real one must never end up in a picture.
    private static let sampleKey = "0123456789abcdef0123456789abcdef"

    /// The updates feed, asked for once.
    ///
    /// `make shots` draws twenty screens in a few seconds and two of them want
    /// this. Without the hold it fetched the same 78 KB twice.
    private static var asked = false
    private static var published = Detours.quiet

    private static func notices() async -> Detours {
        if asked { return published }
        asked = true
        published = (try? await Feed.detours()) ?? .quiet
        return published
    }

    /// The screens one name asks for.
    ///
    /// Every name but `browse` is a single screen; `browse` is the whole way
    /// down, which is four. A caller that wants one view takes the first.
    static func named(_ name: String) async throws -> [(String, AnyView)] {
        switch name {
        case "browse":
            return try await browsing(at: Clock(at: .now))

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

        case "kept":
            return try await keeping(named: name)

        case "detour":
            // The board, at eight in the morning rather than at whatever hour
            // the shot was taken: past the last bus every board is empty, and
            // an empty board is a poor picture of a board.
            //
            // The same builder as `browse`, not a second copy of it. It was a
            // copy, and the copy kept a route-picking rule that could still
            // choose a route with nothing due — the bug the other one had
            // already been fixed for.
            guard let morning = Clock(at: .now).at("08:00") else {
                throw Missing("a clock for this morning")
            }
            return try await browsing(at: morning)
                .filter { $0.0 == "browse-board" }
                .map { (name, $0.1) }

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
    /// The first screen with boards kept on it, which is what the app looks
    /// like once somebody has used it for a week.
    ///
    /// The pins are made here rather than read, so the picture does not depend
    /// on what this machine happens to keep. Drawn at eight in the morning for
    /// the reason every screen here is: past the last bus a kept row says
    /// "none today", which is a poor picture of a kept row.
    private static func keeping(named name: String) async throws -> [(String, AnyView)] {
        let cache = try Cache(path: Paths.cache)
        let clock = Clock(at: .now).at("08:00") ?? Clock(at: .now)
        let published = await notices()
        let boards = try await Example.boards(
            in: cache, updates: published, at: clock, count: Pins.room)

        // Dated, because this screen shows the Schedule row and an undated one
        // reads as a cache nobody has checked.
        let schedule = Schedule(
            showing: .ready(cache), freshness: .current(built: .now),
            key: Key.read(), pins: boards.compactMap(Pin.init))
        await schedule.resolve()

        return [
            (name, popover(.transit, in: cache, at: clock, model: schedule, published: published))
        ]
    }

    /// One browse screen inside the popover's own bars.
    ///
    /// One spelling of the shell. It was two, and they had drifted: one handed
    /// the view a `Navigation` and the other did not, with nothing saying
    /// which was meant. The footer that reads it draws on the first screen
    /// only, so handing it over always is free on every other screen and
    /// right on that one.
    ///
    /// The browse shell and not `RootView`. RootView starts the clock the
    /// moment it appears, and a screen asked for by name must keep the hour it
    /// was asked for.
    private static func popover(
        _ place: Place, in cache: Cache, at clock: Clock,
        live: Realtime? = nil, hearing: Hearing = .none,
        model: Schedule, published: Detours
    ) -> AnyView {
        AnyView(
            chrome {
                BrowseView(
                    cache: cache, clock: clock, live: live, hearing: hearing,
                    start: Browser(place))
            }
            .environment(model)
            .environment(Notices(showing: published))
            .environment(Navigation()))
    }

    private static func browsing(at clock: Clock) async throws -> [(String, AnyView)] {
        let cache = try Cache(path: Paths.cache)

        // What the city has published, so the board and the route list can
        // show it. Nil where the feed did not answer, which draws as no detour.
        let updates = await notices()

        guard let board = try await Example.board(in: cache, updates: updates, at: clock) else {
            throw Missing("a bus route running today")
        }

        // The board is the screen this whole program is for, and one drawn with
        // no predictions is the screen failing to show what it is for.
        var live: Realtime?
        if let key = Key.read() { live = try? await Feed.trips(key: key) }
        let hearing: Hearing = live == nil ? .none : .live

        // The model the screens read for the key, the notices and what is
        // kept. Its cadences are silent, so nothing here asks the network a
        // second time.
        //
        // The board being drawn is kept, so the pin in the bar is photographed
        // filled. Waiting cannot reach that state: a shot would have to click
        // the pin first, and nothing here clicks anything.
        let schedule = Schedule(
            showing: .ready(cache), key: Key.read(), pins: [Pin(board)].compactMap { $0 })

        // The way down to that board is the same place with its last answers
        // taken off, and `keeping` already does exactly that. Nothing here
        // reassembles a route, a headsign and a stop into the enum they came
        // out of.
        return [
            ("browse-routes", board.keeping(1)),
            ("browse-direction", board.keeping(2)),
            ("browse-stops", board.keeping(3)),
            ("browse-board", board),
        ].map { name, place in
            (
                name,
                popover(
                    place, in: cache, at: clock, live: live, hearing: hearing,
                    model: schedule, published: updates)
            )
        }
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
