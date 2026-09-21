// The three bars every screen has, and nothing else.
//
// They are drawn once, around whatever list is in the middle, rather than by
// each screen. A person who has learned where the way back is, or where the way
// out is, never has to look for either again — and a screen cannot forget to
// draw one.

import OTransitKit
import SwiftUI

/// The popover's size.
///
/// Three things need it and they must agree: the scene that draws the real
/// popover, the preview window, and the shot that photographs one. A screen
/// designed against one height and shipped at another is a screen with a
/// scrollbar nobody asked for.
enum Popover {
    static let width: CGFloat = 320
    static let height: CGFloat = 400
}

/// Where you are, with the way back beside it.
struct TopBar: View {
    let screen: Screen
    /// Nil on the first screen, which has nothing behind it.
    let back: (() -> Void)?
    /// What the feed behind this screen is doing, where the screen has one.
    /// `.none` on every screen that is not a board, and on rail, which has no
    /// realtime to hear.
    var hearing: Hearing = .none

    @State private var hovering = false

    var body: some View {
        HStack(spacing: 4) {
            Group {
                if let back {
                    Button(action: back) {
                        Image(systemName: "chevron.backward")
                            .font(.system(size: 11, weight: .semibold))
                            .foregroundStyle(.secondary)
                            .frame(width: 22, height: 20)
                            .band(hovering, corner: 5)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .onHover { hovering = $0 }
                    .keyboardShortcut("[", modifiers: .command)
                } else {
                    Color.clear.frame(width: 22, height: 20)
                }
            }

            // The question, where reading starts.
            //
            // It used to sit at the bottom under the noun for the same screen,
            // and the two said the same word: "Stations" over "Which station?".
            // Naming the screen twice cost the trail a quarter of its bar — 73
            // of 298 points — which is the difference between a board's trail
            // fitting and not.
            Text(screen.prompt)
                .font(.system(size: 13, weight: .semibold))
                .lineLimit(1)
                .frame(maxWidth: .infinity)

            // The same width as the back button either way, so the name sits
            // on the popover's centre line rather than the centre of what is
            // left over — whether or not there is anything to say here.
            LiveMark(hearing: hearing)
                .frame(width: 22, height: 20)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 7)
    }
}

/// Whether the board under it is being fed.
///
/// The only thing in the app that says so. The poller keeps the last good
/// answer when the endpoint refuses and backs off up to about two hours, and
/// without this a board four minutes out of date is drawn exactly like one from
/// four seconds ago. A mark with a single state would say only that the program
/// has a network; this one has to be able to say no.
struct LiveMark: View {
    let hearing: Hearing

    var body: some View {
        switch hearing {
        case .none:
            Color.clear
        case .live:
            symbol
                .foregroundStyle(Color.accentColor)
                .help("Live — arrival times are being updated")
        case .stale(let seconds):
            symbol
                .foregroundStyle(.tertiary)
                .help("Last heard \(Format.wait(minutes: Format.minutes(seconds))) ago")
        }
    }

    private var symbol: some View {
        Image(systemName: "dot.radiowaves.up.forward")
            .font(.system(size: 12, weight: .regular))
            .symbolRenderingMode(.hierarchical)
    }
}

/// What this screen is asking, and the path that got here.
struct PathBar: View {
    let crumbs: [Crumb]
    /// Called with the number of crumbs to keep — tapping "Bus" on a stop list
    /// keeps one and drops the rest.
    let jump: (Int) -> Void

    var body: some View {
        // The first screen has no path, and an empty bar there is 35 points of
        // nothing between two rules — which reads as something that failed to
        // load rather than as a bar with nothing to say.
        if crumbs.isEmpty {
            EmptyView()
        } else {
            // The width the crumbs have, so they can be told to fill it.
            //
            // A scroll view puts its content where the scroll offset says, and
            // the offset is pinned to the far end so the deepest crumb is the
            // one showing. That is right when the trail overflows and wrong
            // when it does not: a short trail was pushed against the right edge
            // by the same pin. Giving the stack the viewport as a minimum width
            // leaves it leading-aligned until there is more of it than there is
            // room.
            GeometryReader { bar in
                TrailRow(crumbs: crumbs, jump: jump, width: bar.size.width)
            }
            .frame(height: Surface.barHeight)
        }
    }
}

/// The crumbs themselves, laid out in the width the bar turned out to have.
///
/// Its own View and not a function returning one. A GeometryReader runs its
/// content closure on every layout pass, and as a function that meant every
/// crumb was rebuilt on each of them. As a View with `width` among its inputs,
/// SwiftUI can skip the whole trail when the width it was handed has not moved.
private struct TrailRow: View {
    let crumbs: [Crumb]
    let jump: (Int) -> Void
    let width: CGFloat

    /// The air either side of every separator.
    ///
    /// One number, spent by the stack and by nothing else, so that every step
    /// of "Bus › 7 › toward Carleton" is spaced the same. Padding applied
    /// inside a crumb cannot do this: a plain crumb's padding is transparent
    /// and reads as space, and a badge's is inside its fill and does not — so
    /// the two ended up 6pt and 2pt from the chevron between them.
    private static let gap: CGFloat = 5

    /// The far end of the trail, as something a scroll view can be pointed at.
    private static let end = "trail.end"

    /// How wide the crumbs came out, measured rather than guessed.
    @State private var trailWidth: CGFloat = 0

    /// Whether there is any trail off the leading edge to fade toward.
    private func scrolls(in width: CGFloat) -> Bool { trailWidth > width - 22 + 1 }

    var body: some View {
        HStack(spacing: 9) {
            // A scroll view, because an HStack of fixed-size crumbs asks for
            // the sum of their widths and every parent above it obliges: a
            // long station name made the whole popover wider than 320 and the
            // departure rows spilled out of both edges. A scroll view accepts
            // the width it is offered instead, and the crumbs that do not fit
            // stay reachable rather than being thrown away.
            ScrollViewReader { trail in
                ScrollView(.horizontal) {
                    HStack(spacing: Self.gap) {
                        ForEach(Array(crumbs.enumerated()), id: \.element.id) { position, crumb in
                            if position > 0 {
                                Image(systemName: "chevron.forward")
                                    // The step between two crumbs. At seven
                                    // points and tertiary it read as spacing
                                    // rather than as a separator.
                                    .font(.system(size: 9, weight: .bold))
                                    .foregroundStyle(.secondary)
                            }
                            Segment(
                                crumb: crumb,
                                current: position == crumbs.count - 1,
                                jump: { jump(position + 1) })
                        }
                        // Something to aim at.
                        //
                        // Scrolling to the last crumb's own id does not work:
                        // a row of this ForEach is two views, a separator and a
                        // segment, sharing one identity, and that is not a
                        // thing with an edge. This is — a hairline of nothing
                        // at the far end, pulled back over the gap before it so
                        // it takes up no room.
                        Color.clear
                            .frame(width: 1, height: 1)
                            .padding(.leading, -Self.gap)
                            .id(Self.end)
                    }
                    .frame(minWidth: width - 22, alignment: .leading)
                    // How wide the crumbs actually came out, which is the only
                    // way to know whether any of them are off the left edge.
                    .background {
                        GeometryReader { stack in
                            Color.clear.preference(
                                key: TrailWidth.self, value: stack.size.width)
                        }
                    }
                }
                .scrollIndicators(.hidden)
                // Held at the end rather than only opened there.
                //
                // The trail cannot be made to fit: one headsign alone is 180 of
                // the 225 points this bar has, so it scrolls on some screens
                // whatever else is done to it. What can be decided is which end
                // is showing when you have not touched it, and that is the deep
                // end — the first crumbs are recoverable from the screen itself,
                // and the last one is what you came here for.
                //
                // The anchor gets the first frame right on its own. What it
                // does not do is say where a scroll view goes when a crumb is
                // added to content it has already laid out — which is what
                // going one screen deeper does — so the position is asked for
                // again every time the trail changes.
                .defaultScrollAnchor(.trailing)
                .onChange(of: crumbs, initial: true) { showEnd(of: trail) }
                .onPreferenceChange(TrailWidth.self) { trailWidth = $0 }
            }

            // A fade only where there is something behind it.
            //
            // It says "the trail starts further back than you can see", and it
            // was saying that on every screen — including the ones whose whole
            // path fits with room to spare, where it dimmed the first crumb to
            // announce nothing. It dims rather than erases, because the fade is
            // on the viewport and not on the content: scrolling back to the
            // first crumb would otherwise still leave it half gone.
            .mask {
                LinearGradient(
                    stops: [
                        .init(color: .black.opacity(scrolls(in: width) ? 0.3 : 1), location: 0),
                        .init(color: .black, location: 0.06),
                        .init(color: .black, location: 1),
                    ], startPoint: .leading, endPoint: .trailing)
            }
        }
        .padding(.horizontal, 11)
        .frame(maxHeight: .infinity)
    }

    /// Puts the far end of the trail against the trailing edge.
    ///
    /// After the pass that changed the trail, not during it: the crumb being
    /// scrolled past is often the one this pass has just added, and a view that
    /// has not been laid out yet has no position to scroll to.
    private func showEnd(of trail: ScrollViewProxy) {
        Task { @MainActor in
            await Task.yield()
            trail.scrollTo(Self.end, anchor: .trailing)
        }
    }

    private struct Segment: View {
        let crumb: Crumb
        let current: Bool
        let jump: () -> Void
        @State private var hovering = false

        var body: some View {
            Group {
                if let plate = crumb.plate {
                    // A route keeps its own colour here, so the trail and the
                    // badges in the list beneath it are recognisably the same
                    // route. The pill's edge is this crumb's edge, which is why
                    // its padding may stay inside.
                    //
                    // Drawn here rather than through `Badge`: that one is sized
                    // for a route number at full height and this bar is 15
                    // points tall, the same as every other small badge.
                    let (fill, text) = Color.badge(plate.colour)
                    Text(crumb.label)
                        .font(.system(size: 9.5, weight: .bold, design: .monospaced))
                        .foregroundStyle(text)
                        .padding(.horizontal, plate.service == .frequent ? 7 : 4)
                        .frame(minWidth: plate.service == .line ? 15 : 0, minHeight: 15)
                        .background(fill, in: plate.service.badge)
                } else {
                    HStack(spacing: 5) {
                        CrumbLabel(crumb: crumb, metrics: .chrome, dimmed: !current)
                            // Pad, draw the hover highlight, then take the
                            // padding back out of the layout: the highlight
                            // bleeds past the words without making this crumb
                            // wider than they are, so the gap beside it matches
                            // a badge's.
                            .padding(.horizontal, 4)
                            .padding(.vertical, 1)
                            .band(hovering && !current, corner: 4)
                            .padding(.horizontal, -4)
                            .padding(.vertical, -1)
                            // The words, for anything that reads the screen
                            // aloud: a mark that saves the bar thirty points
                            // must not cost the crumb its name.
                            .accessibilityLabel(crumb.spoken)
                        // The stop's plate, at the height the badges beside it
                        // already are. Only the last crumb is ever a stop, and
                        // the last crumb is never hoverable, so it sits outside
                        // the highlight without having to be told to.
                        if let platform = crumb.platform {
                            PlatformMark(platform: platform, side: 15)
                        }
                    }
                }
            }
            .fixedSize()
            // Hit the band, not the glyphs, and across the whole height of
            // the bar. The negative padding that lets the band bleed past a
            // crumb without widening it took the same width out of the hit
            // test, which left a crumb clickable only across its letters.
            .padding(.horizontal, 4)
            .frame(maxHeight: .infinity)
            .contentShape(Rectangle())
            .padding(.horizontal, -4)
            .contentShape(Rectangle())
            .onTapGesture { if !current { jump() } }
            .onHover { hovering = $0 }
        }
    }
}

/// The way out: its own row, at the bottom, on every screen.
///
/// An agent app has no menu bar of its own, so the popover is the only place
/// this can live. A full-width row that lights under the pointer is what a menu
/// item is, which is what someone reaching for the bottom of a menu bar popover
/// expects to find.
struct QuitRow: View {
    @State private var hovering = false

    var body: some View {
        Button {
            NSApplication.shared.terminate(nil)
        } label: {
            HStack {
                Text("Quit otransit")
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 10)
            .frame(height: Surface.barHeight - 2 * Surface.inset)
            // The whole row, not just the words, so the pointer does not have
            // to find them.
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        // The same band as every row above it. This was the one control in the
        // app that did not round, which made the way out look like part of the
        // window rather than a thing to press.
        .band(hovering)
        .padding(.horizontal, 4)
        .padding(.bottom, 4)
        .onHover { hovering = $0 }
        .keyboardShortcut("q", modifiers: .command)
    }
}

/// What the crumbs measured, passed back up from inside the scroll view.
private struct TrailWidth: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = max(value, nextValue())
    }
}
