// Whether the timetable on disk is the one the city is publishing, said on the
// first screen, always.
//
// This is here because of a silent failure. A cache goes stale when the city
// republishes, the live feed's trip ids stop naming trips it holds, and every
// row on every board falls back to its scheduled time. The board still fills.
// Nothing looks wrong. The only clue is that a city full of buses is suddenly
// running exactly on time.
//
// So the state is on the first screen whatever it is, and the row is a door in
// every one of them: the answer to "is this current" and the way to do
// something about it must not be two different places, and someone who wants a
// fresh copy for a reason of their own should not have to make the app think it
// needs one.

import OTransitKit
import SwiftUI

struct ScheduleRow: View {
    let freshness: Freshness
    let update: () -> Void

    var body: some View {
        Row(
            label: Text("Schedule"),
            detail: detail,
            // Always a door. See above.
            action: update
        ) {
            Group {
                if case .checking = freshness {
                    ProgressView().controlSize(.small).scaleEffect(0.7)
                } else {
                    Image(systemName: symbol)
                        .font(.system(size: 12))
                        .foregroundStyle(tint)
                }
            }
            .frame(width: 19)
        }
    }

    private var detail: String {
        switch freshness {
        case .unknown, .checking:
            "checking for a newer timetable"
        case .current(let built):
            built.map { "up to date · built \(Self.day($0))" } ?? "up to date"
        case .stale(let published):
            // What to do, not what is wrong. The row is the button.
            published.map { "new timetable from \(Self.day($0)) · update" }
                ?? "a newer timetable is out · update"
        case .unreachable(let built):
            // Not "out of date": we do not know that. The cache may be perfect
            // and the café's wifi may be bad.
            built.map { "can't check · built \(Self.day($0))" } ?? "can't check"
        case .unstamped(let built):
            // The server answered and this cache has nothing to compare with
            // it. Saying "can't check" here would blame a network that is
            // working; rebuilding is what actually settles it.
            built.map { "can't compare · built \(Self.day($0)) · update" }
                ?? "can't compare · update"
        }
    }

    private var symbol: String {
        switch freshness {
        case .unknown, .checking: "arrow.triangle.2.circlepath"
        case .current: "checkmark.circle"
        case .stale: "arrow.down.circle.fill"
        case .unreachable: "wifi.slash"
        case .unstamped: "questionmark.circle"
        }
    }

    /// Green when there is nothing to do, amber for the one state worth acting
    /// on, and the secondary grey for the two that are still questions.
    ///
    /// The same green the board uses for a bus that is on time, because it is
    /// saying the same thing: this is fine, read no further. Not red for stale:
    /// nothing is broken, and a red mark on the first screen reads as a fault
    /// in the program rather than a timetable worth refreshing.
    private var tint: Color {
        switch freshness {
        case .current: .green
        case .stale: .orange
        case .unknown, .checking, .unreachable, .unstamped: .secondary
        }
    }

    /// A day, in the reader's own format. "18 Sep" where this is written and
    /// "Sep 18" where it is not.
    private static func day(_ date: Date) -> String {
        date.formatted(.dateTime.day().month(.abbreviated))
    }
}
