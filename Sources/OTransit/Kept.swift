// One kept board, with what calls there today.
//
// The pin says which board. The calls are what the cache answered when the
// popover opened, and the row works the wait out from them, the clock and the
// live feed — exactly as a board does, because it is a board one row long.

import OTransitKit

struct Kept: Identifiable, Equatable {
    let pin: Pin
    /// The route as today's timetable has it, so the badge carries the colour
    /// and the kind the route list gives it. A pin stores a short name and a
    /// short name is not a badge.
    let route: Route
    let mode: Mode
    /// The stop as today's timetable has it, not as the file remembers it. The
    /// file keeps a name so a person can read it; the platform plate and the
    /// spelling come from the cache, which is the thing that gets updated.
    let stop: Stop
    /// The service day these calls were read for, written YYYYMMDD.
    ///
    /// Carried because the clock a row draws against is not the clock they were
    /// resolved against: a popover left open crosses midnight into a day these
    /// do not describe, and a screen asked for by name draws at an hour of its
    /// own choosing.
    let date: String
    let calls: [Departure]

    var id: String { pin.id }

    /// The board this pin names.
    var place: Place { .board(mode, route, pin.headsign, stop) }

    /// The two crumbs a row draws: where you are standing, and where it is
    /// going.
    ///
    /// Not the route's. A badge is not one of these labels and is drawn from
    /// `route` directly, the way the bar draws it — so asking for it here
    /// would be guarding the whole row on a value nothing reads.
    ///
    /// Nil never happens: `place` is always a board and a board's trail always
    /// has both. It is optional because `Trail` answers for any place and has
    /// no way to say so, and one branch where the list makes a row is cheaper
    /// than two inside every row.
    ///
    /// Asked for once and not per crumb. Each `Trail.crumbs` walks the place
    /// back to the first screen and builds an array, and a row that read
    /// `stop` and `direction` as separate computed properties did that twice
    /// on every redraw, three rows at a time.
    var crumbs: (stop: Crumb, direction: Crumb)? {
        let all = Trail.crumbs(of: place)
        guard let stop = all.first(where: \.isStop),
            let direction = all.first(where: \.isDirection)
        else { return nil }
        return (stop, direction)
    }

    /// The next one, or nil once the day is over here.
    ///
    /// Nil too when the clock has moved on to a day these calls are not about,
    /// which is a true answer: nothing here knows what runs tomorrow.
    func next(live: Realtime?, clock: Clock) -> Arrival? {
        guard clock.date == date else { return nil }
        // Already this route in this direction, narrowed when they were read.
        return Board.rows(from: calls, live: live, stop: stop.id, clock: clock)
            .first { $0.at >= clock.now }
    }
}
