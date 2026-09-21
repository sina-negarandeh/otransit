// The kept boards, under the two modes on the first screen.
//
// Under and not over them. Above, a third pin pushes Bus and O-Train down and
// the furniture is never in the same place twice; under, the modes are nailed
// to the top and the pins grow into the space that was already empty. The rule
// above them is what the eye jumps to, so the group reads as answers rather
// than as more of the mode list.
//
// Each row carries its next bus, so the question is answered before anything is
// pressed. Same badge and same urgency colours as a board, because it is a
// board, one row long.

import OTransitKit
import SwiftUI

struct KeptList: View {
    let kept: [Kept]
    let live: Realtime?
    let clock: Clock
    let open: (Kept) -> Void

    var body: some View {
        if !kept.isEmpty {
            Rule()
            VStack(spacing: 1) {
                ForEach(kept) { board in
                    // Taken apart here so a row is total. A board always has
                    // both crumbs, and a row that had to say "unless it does
                    // not" would wrap its whole body in a branch to say it.
                    if let crumbs = board.crumbs {
                        KeptRow(
                            kept: board, stop: crumbs.stop, direction: crumbs.direction,
                            next: board.next(live: live, clock: clock),
                            clock: clock, open: { open(board) })
                    }
                }
            }
            // The inset a Listing gives its rows, so a hovered row here reaches
            // exactly as far as one above.
            .padding(.horizontal, 4)
            .padding(.vertical, 4)
        }
    }
}

/// One kept board: the badge, where you are standing, and where it is going.
///
/// The same crumbs the bottom of a board shows and in the same words, at the
/// weight of a row rather than of chrome. What you kept looks like what you
/// were looking at when you kept it, and the platform plate is the only thing
/// that tells two pins at one station apart.
///
/// Stacked rather than strung out. On one line it read route, arrow,
/// destination, chevron, stop, plate — six things and two of them punctuation,
/// in 230 points. The stop moved onto its own line above the direction, which
/// is the shape every other row in the app already has: the name, and a
/// quieter line under it saying more about it.
///
/// Not a `Row`. That type's label is a `Text` on purpose, because a composed
/// `Text` truncates as one run; this label is a stack, and wants the stop name
/// to lose its tail while the plate beside it survives. It is the same door,
/// though, which is what `door` is for — and the same crumbs the bar draws,
/// which is what `CrumbLabel` is for.
private struct KeptRow: View {
    let kept: Kept
    /// Where you are standing.
    let stop: Crumb
    /// Where it is going.
    let direction: Crumb
    let next: Arrival?
    let clock: Clock
    let open: () -> Void

    var body: some View {
        // The mode crumb is not drawn: the badge already says whether this is a
        // bus or a train. The badge is drawn from the route rather than from
        // its crumb, the way the bar draws it, because a badge has never been
        // one of these labels.
        HStack(spacing: 8) {
            Badge(
                kept.route.shortName, colour: kept.route.colour,
                service: kept.route.service(in: kept.mode))

            // Where you are standing, and under it where the bus is going.
            //
            // The stop on top because that is the harder of the two to work
            // out from the other, and because it is what tells two pins at one
            // pole apart. Stacked rather than strung along one line: a trail
            // needs a separator between every step, and two of those on a row
            // this narrow is more punctuation than words. A line break does
            // the same work and costs no glyphs.
            VStack(alignment: .leading, spacing: 1) {
                HStack(spacing: 3) {
                    CrumbLabel(crumb: stop, metrics: .row)
                    if let platform = stop.platform {
                        PlatformMark(platform: platform, side: 14)
                    }
                }
                CrumbLabel(crumb: direction, metrics: .detail, dimmed: true)
            }

            Spacer(minLength: 4)

            // Never truncated. The wait is the answer this row exists to give,
            // and "4 hr 38 m…" is not an answer. Everything to its left gives
            // way instead.
            Text(wait)
                .font(.mono(.caption))
                .foregroundStyle(tint.map(AnyShapeStyle.init) ?? AnyShapeStyle(.secondary))
                .lineLimit(1)
                .fixedSize()
            Disclosure()
        }
        .padding(.horizontal, 8)
        // Five and not the eight a one-line row took. Two lines are 29 points
        // where one was 20, and three rows of the difference came out of the
        // mode list above, which had been absorbing it as stretch: the gap
        // under "166 routes running today" went from 21 points to 3. Five puts
        // that back to 9, which is a margin rather than a coincidence.
        .padding(.vertical, 5)
        .door(open)
    }

    private var wait: String {
        guard let next else { return "none today" }
        if next.status == .cancelled { return "\u{2014}" }
        return Format.wait(minutes: Format.minutes(next.at - clock.now))
    }

    private var tint: Color? {
        guard let next else { return nil }
        if next.status == .cancelled { return .red }
        return Urgency(minutes: Format.minutes(next.at - clock.now)).colour
    }
}
