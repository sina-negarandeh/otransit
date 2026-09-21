// Drilling down: what you are taking, which route, which way, which stop, and
// then the board.
//
// One shell with the list swapped inside it, rather than a navigation stack.
// The chrome is the same on every screen and only the middle changes, and the
// trail is worked out from what has been chosen — so the bar and the screen
// cannot disagree, which two independent pieces of state would eventually let
// them do.

import OTransitKit
import SwiftUI

/// What has been answered so far.
///
/// One `Place`, which is the whole of it. This used to hold four optionals and
/// derive the place from them by a guard chain — a second representation of the
/// same state, able to express a stop chosen before a route, and needing a set
/// of bare integers to step back through that had to stay in agreement with
/// both the guard chain and the number of crumbs the trail happened to draw.
/// The enum already carried every answer; the optionals were saying it twice.
@MainActor
@Observable
final class Browser {
    /// Starting somewhere other than the first screen is a preview's business:
    /// the worst case for the trail is four crumbs ending in a long station
    /// name, and reaching it by clicking four times every time a layout changes
    /// is how a layout stops being checked.
    private(set) var place: Place

    init(_ place: Place = .transit) {
        self.place = place
    }

    var crumbs: [Crumb] { Trail.crumbs(of: place) }

    // Each of these is only reachable from the screen before it, so the guard
    // is a statement of where the call comes from rather than a fallback: a
    // route can only be chosen from a route list, which only exists once a
    // network has been.
    func choose(_ mode: Mode) { place = .routes(mode) }

    func choose(_ route: Route) {
        guard case .routes(let mode) = place else { return }
        place = .direction(mode, route)
    }

    func choose(headsign: String) {
        guard case .direction(let mode, let route) = place else { return }
        place = .stops(mode, route, headsign)
    }

    func choose(_ stop: Stop) {
        guard case .stops(let mode, let route, let headsign) = place else { return }
        place = .board(mode, route, headsign, stop)
    }

    /// Steps back one screen.
    func back() { place = place.back ?? place }

    /// Drops every answer past the first `count` of them.
    func keep(_ count: Int) { place = place.keeping(count) }
}

struct BrowseView: View {
    let cache: Cache
    let clock: Clock
    /// The last thing the realtime feed said, or nil when nothing has asked.
    let live: Realtime?
    /// How well that feed is being heard.
    let hearing: Hearing

    @State private var browser: Browser

    init(
        cache: Cache, clock: Clock, live: Realtime? = nil, hearing: Hearing = .none,
        start: Browser = Browser()
    ) {
        self.cache = cache
        self.clock = clock
        self.live = live
        self.hearing = hearing
        _browser = State(initialValue: start)
    }

    var body: some View {
        VStack(spacing: 0) {
            TopBar(screen: browser.place.screen, back: back, hearing: listening)
            Rule()
            Lists(cache: cache, clock: clock, live: live, browser: browser)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            Rule()
            PathBar(crumbs: browser.crumbs, jump: browser.keep)
        }
        .animation(.smooth(duration: 0.22), value: browser.place)
    }

    /// What to say about the feed on the screen being shown.
    ///
    /// Only a board has one. Every screen before it is a list out of the
    /// timetable, which does not change while you look at it — and rail has no
    /// realtime at all, so a mark there would be reporting on a feed that never
    /// mentions the train you are waiting for.
    private var listening: Hearing {
        guard case .board(let mode, _, _, _) = browser.place, mode == .bus else { return .none }
        return hearing
    }

    /// Nil on the first screen, which has nothing behind it. Written out rather
    /// than inlined as a ternary: an optional closure built inside a view
    /// builder is what the type checker gives up on.
    private var back: (() -> Void)? {
        guard browser.place.screen.hasBack else { return nil }
        return { browser.back() }
    }

}

/// Whichever list the current place calls for.
///
/// Its own View and not a computed property on BrowseView. A computed property
/// is inlined into the enclosing body and shares its invalidation boundary, so
/// every arrival of the realtime feed — one every twenty-five seconds while the
/// popover is open — rebuilt the whole screen rather than the one list that
/// reads it.
private struct Lists: View {
    let cache: Cache
    let clock: Clock
    let live: Realtime?
    let browser: Browser

    var body: some View {
        switch browser.place {
        case .transit:
            TransitList(cache: cache, date: clock.date, pick: browser.choose)
        case .routes(let mode):
            RouteList(cache: cache, date: clock.date, mode: mode, pick: browser.choose)
        case .direction(_, let route):
            DirectionList(
                cache: cache, date: clock.date, route: route, pick: browser.choose(headsign:))
        case .stops(_, let route, let headsign):
            StopList(
                cache: cache, date: clock.date, route: route, headsign: headsign,
                pick: browser.choose)
        case .board(_, let route, let headsign, let stop):
            BoardView(
                cache: cache, clock: clock, stop: stop, route: route, headsign: headsign,
                live: live)
        }
    }
}

private struct TransitList: View {
    let cache: Cache
    let date: String
    let pick: (Mode) -> Void

    @State private var counts: [Mode: Int] = [:]

    var body: some View {
        VStack(spacing: 0) {
            Listing {
                ForEach(Mode.allCases, id: \.self) { mode in
                    Row(
                        label: Text(mode.title), detail: detail(mode),
                        action: { pick(mode) }
                    ) {
                        Image(systemName: mode.symbol)
                            .font(.system(size: 12))
                            .foregroundStyle(.secondary)
                            .frame(width: 19)
                    }
                }
            }

            TransitFooter()
        }
        .task {
            for mode in Mode.allCases {
                counts[mode] = (try? await cache.routes(mode, on: date).count) ?? 0
            }
        }
    }

    /// How much runs today, which is the one thing worth knowing before
    /// choosing between two words.
    private func detail(_ mode: Mode) -> String? {
        guard let count = counts[mode] else { return nil }
        switch mode {
        case .bus: return "\(Format.count(count, "route")) running today"
        case .rail: return "\(Format.count(count, "line")) · scheduled times only"
        }
    }
}

/// The two rows about the program rather than about a journey, at the far end
/// of the first screen and under a line.
///
/// They were above, directly after the modes, where they read as two more
/// things you might be taking. Down here nothing competes with the question at
/// the top, and they are still on the one screen looked at before every other.
///
/// Its own View, and it reads its own environment and loads its own state: as a
/// computed property on TransitList it shared that view's invalidation
/// boundary, so counting today's routes — five queries, arriving one at a time
/// — redrew these two rows five times, and reading the key redrew the modes.
private struct TransitFooter: View {
    /// Both absent in a preview, which draws this screen without a running app.
    @Environment(Schedule.self) private var schedule: Schedule?
    @Environment(Navigation.self) private var navigation: Navigation?

    var body: some View {
        if schedule != nil || navigation != nil {
            Rule()
            VStack(spacing: 1) {
                if let schedule {
                    ScheduleRow(freshness: schedule.freshness) { schedule.build() }
                }
                if let navigation {
                    Row(
                        label: Text("Settings"),
                        detail: schedule.map {
                            $0.key != nil ? "live times on" : "scheduled times only"
                        },
                        action: { navigation.settings = true }
                    ) {
                        Image(systemName: "gearshape")
                            .font(.system(size: 12))
                            .foregroundStyle(.secondary)
                            .frame(width: 19)
                    }
                }
            }
            // The inset a Listing gives its rows, so a hovered row down here
            // reaches exactly as far as one up there.
            .padding(.horizontal, 4)
            .padding(.vertical, 4)
        }
    }
}

private struct RouteList: View {
    let cache: Cache
    let date: String
    let mode: Mode
    let pick: (Route) -> Void

    @State private var sections = Sections()

    var body: some View {
        Query(
            key: mode,
            vacancy: Vacancy(
                title: "Nothing runs today",
                note: "No route of this kind is scheduled on today\u{2019}s service.",
                symbol: "calendar")
        ) {
            try await cache.routes(mode, on: date)
        } content: { routes in
            let groups = Service.group(routes, in: mode)
            Listing {
                ForEach(groups) { group in
                    // One section needs no heading: rail is three lines and
                    // calling them "Lines" under a screen already called Lines
                    // is the same word twice.
                    if groups.count > 1 {
                        SectionHeading(
                            service: group.service, count: group.routes.count,
                            folded: sections.isFolded(group.service)
                        ) {
                            withAnimation(.smooth(duration: 0.2)) {
                                sections.toggle(group.service)
                            }
                        }
                    }
                    if groups.count == 1 || !sections.isFolded(group.service) {
                        ForEach(group.routes) { route in
                            Row(label: .route(route), action: { pick(route) }) {
                                Badge(
                                    route.shortName, colour: route.colour,
                                    service: route.service(in: mode))
                            }
                            // 29 of the 158 routes with two ends are wider than
                            // the 226 points a row gives their name, and the
                            // widest is 373. Tightening the arrow recovers 3 of
                            // them and shrinking the type recovers a dozen at
                            // the cost of every row that did not need it, so
                            // neither is the answer: the answer is that the
                            // whole name is one hover away.
                            .help(route.spoken)
                        }
                    }
                }
            }
        }
    }
}

private struct DirectionList: View {
    let cache: Cache
    let date: String
    let route: Route
    let pick: (String) -> Void

    var body: some View {
        Query(
            key: route.id,
            vacancy: Vacancy(
                title: "No trips today",
                note: "This route is in the schedule but does not run on today\u{2019}s service.",
                symbol: "calendar")
        ) {
            try await cache.directions(of: route.shortName, on: date)
        } content: { directions in
            Listing {
                ForEach(directions) { direction in
                    // "toward" is carried here so the trail behind it reads as
                    // a sentence: Bus › 7 › Toward Carleton.
                    Row(
                        label: Text("Toward \(direction.name)"),
                        detail: "\(Format.count(direction.trips, "trip")) today",
                        action: { pick(direction.headsign) }
                    ) {
                        // The route's own colour, carried one screen further in.
                        //
                        // No circle: a ring at this size leaves the arrow inside
                        // it thin, and a filled one closes over the arrow
                        // entirely — on rail that is a solid red dot one screen
                        // after the solid red Line 1 badge.
                        Image(systemName: "arrowshape.right.fill")
                            .font(.system(size: 15, weight: .regular))
                            .foregroundStyle(Color.badge(route.colour).fill)
                            .frame(width: 19)
                    }
                }
            }
        }
    }
}

private struct StopList: View {
    let cache: Cache
    let date: String
    let route: Route
    let headsign: String
    let pick: (Stop) -> Void

    var body: some View {
        Query(
            key: headsign,
            vacancy: Vacancy(
                title: "No stops",
                note: "The schedule has no calls for this direction today.",
                symbol: "signpost.right")
        ) {
            try await cache.stops(of: route.shortName, toward: headsign, on: date)
        } content: { stops in
            Listing {
                ForEach(stops) { stop in
                    // One line, not two. The code was a second line under every
                    // name, which cost 14pt a row and let six stops onto a
                    // screen; at the right margin it costs the name about 48pt
                    // of width and lets eleven on. 26 names of 3,993 are newly
                    // cut by that, all of them long intersections that read
                    // perfectly well cut.
                    Row(
                        label: Text(stop.name),
                        // A code where you can board, and where you cannot, why.
                        value: stop.boards ? stop.codeLine : "Drop-off only",
                        // Not a door. The route does go here — it is where the
                        // route goes — so leaving it out would draw a line that
                        // stops short of its own terminus. There is simply no
                        // board to open: every bus that calls is one nobody can
                        // get on, and the row dims and loses its chevron off
                        // this one fact.
                        action: stop.boards ? { pick(stop) } : nil
                    ) {
                        EmptyView()
                    } mark: {
                        // Nothing rather than a held-open slot: four rows in
                        // seven have no platform, and a column of empty boxes
                        // is worse than a ragged one.
                        if !stop.platform.isEmpty { PlatformMark(platform: stop.platform) }
                    }
                    // The nine longest stop names in the feed do not fit
                    // either, and a row that only drops off spends its margin
                    // saying so rather than on its code.
                    .help(stop.spoken)
                }
            }
        }
    }
}

/// The scrolling box every list sits in, with the inset a macOS source list has.
