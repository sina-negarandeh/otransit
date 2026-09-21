// The board: what calls here next, when it will actually arrive, and where that
// time came from.
//
// No badge and no headsign. This board is reached by choosing a route and a
// direction, so every row would carry the same two; the trail below already
// says which. What is left is three columns that differ from row to row.

import OTransitKit
import SwiftUI

struct BoardView: View {
    let cache: Cache
    let clock: Clock
    let stop: Stop
    let route: Route
    /// The direction chosen. Without it the board shows both ways at any stop
    /// the route serves twice, which is most of the interesting ones.
    let headsign: String
    let live: Realtime?

    /// Absent in a preview, which draws this screen without a running app.
    @Environment(Schedule.self) private var schedule: Schedule?

    @State private var departures: [Departure] = []
    @State private var failure: String?

    /// What the city has published about this route, if anything.
    private var notices: [Notice] { schedule?.notices(for: route.shortName) ?? [] }

    var body: some View {
        // The one screen that keeps its own loading state. Its emptiness is
        // not the query's — the day's calls are all there and every one of
        // them has gone — so it is derived per render rather than asked for,
        // which is the one thing Query cannot do for it.
        // Outside the outcome, not inside it.
        //
        // The notice is about the route and not about the rows, so a board with
        // nothing left today still has one. Inside, it disappeared exactly when
        // the board was emptiest, which is when someone is working out tomorrow.
        VStack(spacing: 0) {
            DetourRow(notices: notices, clock: clock)

            Outcome(
                rows: rows, failure: failure,
                vacancy: Vacancy(title: "Nothing more today", note: gone)
            ) {
                VStack(spacing: 0) {
                    Heading()
                    ScrollView {
                        // Every remaining trip of the day, not the next dozen. The
                        // board mostly answers "do I need to leave now", which the
                        // first two rows do — but the last trip is a question too,
                        // and a list that stops at twelve cannot answer it. Lazy
                        // because a frequent route at a Transitway stop runs nearly
                        // nine hundred times between five in the morning and one at
                        // night, and none of those rows is worth building until it
                        // is looked at.
                        LazyVStack(spacing: 0) {
                            ForEach(rows) { arrival in
                                Line(
                                    arrival: arrival,
                                    minutes: Format.minutes(arrival.at - clock.now))
                            }
                        }
                        .padding(.bottom, 5)
                    }
                }
            }
        }
        .task(id: stop.id) {
            do {
                departures = try await cache.departures(
                    at: stop.id, on: clock.date, after: clock.yesterday)
            } catch {
                failure = String(describing: error)
            }
        }
    }

    /// When the last one left, for the screen that has nothing else to say.
    ///
    /// Free: the query returned the whole day and the board threw away
    /// everything already past, so the answer is the largest of the pieces it
    /// threw away. "Nothing more today" alone leaves a person wondering whether
    /// they have missed it by a minute or by six hours.
    private var gone: String? {
        let past = today.filter { $0.at < clock.now }
        guard let last = past.map(\.at).max() else { return nil }
        return "The last one left at \(Format.hhmm(last))."
    }

    /// Every call on the chosen route and direction, gone or not.
    ///
    /// The query holds every call at the stop, because it had no reason to know
    /// which of them was drilled through. Both halves of the narrowing matter:
    /// the route alone leaves the opposite direction on the board.
    private var today: [Arrival] {
        Board.rows(
            from: departures, on: route.shortName, toward: headsign,
            live: live, stop: stop.id, clock: clock)
    }

    /// What has not gone yet.
    private var rows: [Arrival] {
        today.filter { $0.at >= clock.now }
    }

    /// The only thing separating the heading from the data. No rules anywhere:
    /// a column of figures is already a column.
    ///
    /// It names three columns rather than decorating them, so it is set as a
    /// heading and not as fine print: at 10pt tertiary it read as a caption
    /// under the title bar instead of as the top of the table.
    private struct Heading: View {
        var body: some View {
            HStack(spacing: 0) {
                ForEach(["Wait", "Arrival", "Status"], id: \.self) { name in
                    Text(name)
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity)
                }
            }
            .padding(.horizontal, 10)
            .padding(.top, 9)
            .padding(.bottom, 8)
        }
    }

    private struct Line: View {
        let arrival: Arrival
        let minutes: Int

        var body: some View {
            HStack(spacing: 0) {
                // There is no counting down to a bus that will not come.
                Text(arrival.status == .cancelled ? "—" : Format.wait(minutes: minutes))
                    .font(.mono(.body, weight: .medium))
                    .foregroundStyle(
                        arrival.status == .cancelled ? Color.red : Urgency(minutes: minutes).colour
                    )
                    // The widest thing on the board. "20 hr 00 min" is 96 of the
                    // 100 points a third of the width comes to, so this column
                    // is the one that runs out first when the text size is
                    // turned up, and it gives way by a little rather than by
                    // losing its last minutes to an ellipsis.
                    .lineLimit(1)
                    .minimumScaleFactor(0.82)
                    .frame(maxWidth: .infinity)

                HStack(spacing: 8) {
                    // What the timetable promised, struck out, where the bus is
                    // no longer doing it. Both times rather than the minutes
                    // between them: a reader who wants the platform at 11:16
                    // should not have to add 3 to 11:13 to get it.
                    if let promised = arrival.promised {
                        Text(Format.hhmm(promised))
                            .font(.mono(.footnote))
                            .strikethrough()
                            .foregroundStyle(.tertiary)
                    }
                    Text(Format.hhmm(arrival.at))
                        .font(.mono(.body))
                        // A prediction is the best answer this program has and
                        // reads plainly. A schedule is dim: it is what the
                        // timetable said, not what the bus is doing.
                        .foregroundStyle(
                            arrival.status.isPredicted
                                ? AnyShapeStyle(.primary) : AnyShapeStyle(.tertiary)
                        )
                }
                .lineLimit(1)
                .minimumScaleFactor(0.82)
                .frame(maxWidth: .infinity)

                // The same size as the two columns beside it. Two points
                // smaller read as a footnote about the row rather than as the
                // third of its three answers.
                Text(arrival.status.text)
                    .font(.mono(.body))
                    .foregroundStyle(arrival.status.colour)
                    // "12 min early" is 96 of the column's 100 points, so this
                    // column now varies in width the way Wait does and gives
                    // way the same way.
                    .lineLimit(1)
                    .minimumScaleFactor(0.82)
                    .frame(maxWidth: .infinity)
            }
            .frame(maxWidth: .infinity)
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
        }
    }
}
