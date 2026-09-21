// Finding a journey worth drawing a picture of.
//
// A screenshot of a board needs a route, a direction and a stop that agree with
// each other and that something is actually due at. Neither the app nor the
// tests need this: the app draws where a person went, and a test builds its own
// fixture. Only the two tools that photograph screens do, which is why it is
// here and not in the kit.
//
// It answers with a `Place`, which is the type the rest of the program already
// uses for "where you are, and the answers that led there". Handing back the
// three pieces loose and letting each caller reassemble them is how two copies
// of the same reassembly ended up disagreeing.

import OTransitKit

enum Example {
    /// A board worth drawing, as a place.
    ///
    /// Two things are wanted and they pull apart. A route with a detour draws
    /// the screen that feature is for, and a route whose last bus has gone
    /// draws "Nothing more today" whatever else is true of it. Preferring the
    /// detour alone put an empty board in the set the first time it ran, at one
    /// in the morning.
    ///
    /// So candidates are ordered by preference and tried in turn, and the first
    /// that still has something due wins.
    static func board(
        in cache: Cache, detours: Detours?, at clock: Clock, mode: Mode = .bus
    ) async throws -> Place? {
        let routes = try await cache.routes(mode, on: clock.date)

        // Detoured first, and the cache's own order within each group. A sort
        // would have to name the comparison the cache already applied.
        let detoured = { (route: Route) in detours?.names(route.shortName) ?? false }
        let preferred = routes.filter(detoured) + routes.filter { !detoured($0) }

        var fallback: Place?
        for route in preferred.prefix(tries) {
            let directions = try await cache.directions(of: route.shortName, on: clock.date)
            guard let toward = directions.first?.headsign else { continue }
            let stops = try await cache.stops(of: route.shortName, toward: toward, on: clock.date)
            guard let first = stops.first else { continue }

            if let due = try await awaited(in: stops, on: route, toward: toward, cache, clock) {
                return .board(mode, route, toward, due)
            }
            // Nothing due on this one. Kept in case nothing is due anywhere,
            // which after the last bus of the day is the true answer.
            fallback = fallback ?? .board(mode, route, toward, first)
        }
        return fallback
    }

    /// How many routes are tried before settling for an empty board.
    ///
    /// Late in the evening most routes have finished, so the first candidates
    /// are often all done. Twelve is enough to walk past them and cheap enough
    /// to give up after: each one costs a direction query and up to eight
    /// departure scans.
    private static let tries = 12

    /// The first of these stops this route is still due at, in this direction.
    ///
    /// Route and direction both, because that is what the board draws. Asking
    /// only "is anything due here" picks a stop another route still serves:
    /// route 5 was chosen at 23:45 because buses were still calling at Waller
    /// and Laurier, and none of them was a 5. The board then drew "Nothing more
    /// today" under a stop the search had just called busy.
    ///
    /// Bounded, because each of these is a two-service-day scan of three and a
    /// half million rows. Eight is enough to find one and cheap enough to give
    /// up after.
    ///
    /// Spread along the route rather than taken from the front of it. Late in
    /// the evening the last trip has already passed the first stops while it is
    /// still due at the last ones.
    private static let looksAt = 8

    private static func awaited(
        in stops: [Stop], on route: Route, toward: String, _ cache: Cache, _ clock: Clock
    ) async throws -> Stop? {
        let step = max(1, stops.count / looksAt)
        let sampled = stride(from: 0, to: stops.count, by: step).prefix(looksAt).map { stops[$0] }

        for stop in sampled {
            let due = try await cache.departures(
                at: stop.id, on: clock.date, after: clock.yesterday)
            let mine = due.filter {
                $0.route == route.shortName && $0.headsign == toward && $0.scheduled >= clock.now
            }
            if !mine.isEmpty { return stop }
        }
        return nil
    }
}
