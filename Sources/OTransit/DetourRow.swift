// What the city has published about the route this board is for.
//
// Route-level, because that is all the feed can say. It names stops too, but
// only in prose and only beside the alternates it sends you to, so nothing here
// claims your stop is closed. It says this route has a detour, prints what was
// published, and opens the notice.
//
// The mark is the road sign for a detour rather than a warning triangle, and it
// is amber rather than red. Forty of the hundred and seventy-six routes running
// today carry one, so this is a common state and not an alarm. Red belongs to
// the two-minute wait, where it means leave now.
//
// The title is printed as published and truncated to one line. A description
// runs to about 2,800 characters of HTML, which has no home in 320 points, so
// the whole notice is one tap away instead. The full title is on hover, for the
// two thirds of them that are longer than the row.

import OTransitKit
import SwiftUI

struct DetourRow: View {
    let notices: [Notice]

    /// How many notices a board will give up room for.
    ///
    /// Each row is about forty points of the four hundred this popover has, and
    /// the feed decides how many there are, not this app. Three on one route is
    /// today's worst case; a construction season that tagged a trunk route six
    /// times would push the departures off the screen, and the times are what
    /// the screen is for. The rest stay one tap away.
    private static let atMost = 2

    var body: some View {
        if !notices.isEmpty {
            VStack(spacing: 0) {
                ForEach(notices.prefix(Self.atMost)) { notice in
                    Line(notice: notice)
                }
                if notices.count > Self.atMost {
                    More(count: notices.count - Self.atMost)
                }
            }
            .padding(.horizontal, 4)
            .padding(.top, 4)
        }
    }

    /// How many notices are not drawn, for the rare route that has more.
    private struct More: View {
        let count: Int

        var body: some View {
            Text(Format.count(count, "more detour"))
                .font(.system(size: 10.5))
                .foregroundStyle(.tertiary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 32)
                .padding(.bottom, 5)
        }
    }

    /// One notice, which opens the page it came from where it has one.
    private struct Line: View {
        let notice: OTransitKit.Notice
        @State private var hovering = false

        var body: some View {
            // A row with no link is not a link.
            //
            // It used to fall back to the feed's own address, which serves RSS:
            // tapping it put a wall of XML in the browser, or offered to
            // download it. A notice this app cannot open is still worth saying.
            if let link = notice.link {
                Link(destination: link) {
                    Headline(notice: notice, hovering: hovering)
                }
                .buttonStyle(.plain)
                .onHover { hovering = $0 }
            } else {
                Headline(notice: notice, hovering: false)
            }
        }
    }

    /// What one notice says, and the mark that says what kind of thing it is.
    ///
    /// Its own View and not a computed property on Line. A computed property is
    /// inlined into the enclosing body and shares its invalidation boundary, so
    /// every movement of the pointer would rebuild the icon, the words and the
    /// arrow, none of which read the pointer.
    private struct Headline: View {
        let notice: OTransitKit.Notice
        let hovering: Bool

        var body: some View {
            HStack(spacing: 7) {
                Image(systemName: "arrow.triangle.turn.up.right.diamond.fill")
                    .font(.system(size: 12))
                    .foregroundStyle(.orange)
                    .frame(width: 19)

                Text(notice.headline)
                    .font(.system(size: 11.5))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.tail)

                Spacer(minLength: 0)

                if notice.link != nil {
                    Image(systemName: "arrow.up.forward")
                        .font(.system(size: 9, weight: .semibold))
                        .foregroundStyle(.tertiary)
                }
            }
            .padding(.horizontal, 6)
            .padding(.vertical, 6)
            .band(hovering, corner: Surface.inner(Surface.inset))
            .contentShape(Rectangle())
            // The mark carries no words, so it is given some: a screen reader
            // otherwise hears the headline with nothing to say it is a detour.
            .accessibilityElement(children: .combine)
            .accessibilityLabel("Detour. \(notice.title)")
            // The whole of it, for the two thirds that do not fit the row.
            .help(notice.title)
        }
    }
}
