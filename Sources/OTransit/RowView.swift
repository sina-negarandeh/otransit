// One line of a list, and the vocabulary a line is written in.
//
// Every screen that offers a choice is made of these, so what a row is gets
// decided once: where the name sits, what may stand beside it, what appears at
// the margin, and what it means for a row to be a door.

import OTransitKit
import SwiftUI

struct Row<Leading: View, Mark: View>: View {
    let label: Text
    var detail: String?
    var detailIsValue = false
    /// A value at the far margin: a stop's code, or — where the row is not a
    /// door — the reason it is not one.
    ///
    /// One slot and one rule, because they are the same slot answering the same
    /// question. On a row you can open it is a value and is set in SF Mono; on
    /// one you cannot it is a sentence and is set in the interface face, which
    /// is the rule the rest of the app already follows.
    var value: String?
    /// What tapping does, or nil where this row goes nowhere.
    ///
    /// Nil is the whole of "shown, but not a door": no disclosure, no hover, no
    /// tap, and dimmed. That state used to be spelled at the call site as an
    /// opacity, an `allowsHitTesting`, and an if/else swapping the chevron for
    /// a label — four separate ways of saying one thing, none of which knew
    /// about the others.
    var action: (() -> Void)?
    @ViewBuilder let leading: Leading
    /// Something that belongs to the name rather than to the row, set beside it
    /// where the name ends. A platform is the case: "Tunney's Pasture, platform
    /// 2" is one fact, and holding it out at the right margin makes it look
    /// like a second column instead.
    @ViewBuilder let mark: Mark

    @State private var hovering = false

    private var isDoor: Bool { action != nil }

    var body: some View {
        HStack(spacing: 10) {
            leading
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    label
                        .font(.system(size: 13))
                        .lineLimit(1)
                        .truncationMode(.tail)
                    // The mark has a fixed size and the name does not, so a name
                    // too long for the row loses its tail and the mark stays
                    // whole. The nine names in the feed long enough for that to
                    // happen have no platform, so it never arises.
                    mark
                }
                if let detail {
                    Text(detail)
                        .font(detailIsValue ? .mono(.caption) : .system(size: 11))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            if let value {
                Text(value)
                    .font(isDoor ? .mono(.caption) : .system(size: 11))
                    .foregroundStyle(isDoor ? AnyShapeStyle(.secondary) : AnyShapeStyle(.tertiary))
                    .lineLimit(1)
            }
            // Every row that goes somewhere says so the same way, so no screen
            // has to remember to add it.
            if isDoor { Disclosure() }
        }
        .padding(.horizontal, 8)
        .padding(.vertical, detail == nil ? 8 : 7)
        .band(hovering)
        .opacity(isDoor ? 1 : 0.45)
        .contentShape(Rectangle())
        .allowsHitTesting(isDoor)
        .onHover { hovering = $0 }
        .onTapGesture { action?() }
    }
}

extension Row where Mark == EmptyView {
    /// A row whose name carries nothing beside it, which is most of them.
    init(
        label: Text, detail: String? = nil, detailIsValue: Bool = false, value: String? = nil,
        action: (() -> Void)? = nil, @ViewBuilder leading: () -> Leading
    ) {
        self.init(
            label: label, detail: detail, detailIsValue: detailIsValue, value: value,
            action: action, leading: leading, mark: { EmptyView() })
    }
}

/// The disclosure at the end of a row that goes somewhere.
struct Disclosure: View {
    var body: some View {
        Image(systemName: "chevron.forward")
            // Nine points at tertiary is a smudge at this size. A disclosure
            // has to be read as a way in, not inferred from the space it sits in.
            .font(.system(size: 12, weight: .bold))
            .foregroundStyle(.secondary)
    }
}

extension Text {
    /// A route's name, with the feed's "<>" drawn as the symbol it stands for.
    ///
    /// "Blair <> Tunney's Pasture" is the feed drawing an arrow in a CSV field.
    /// `arrow.left.arrow.right` is that arrow: a line running both ways between
    /// two ends, which is what the two angle brackets are trying to say. It is
    /// a `Text` and not a view of its own so a row can lay it out as one run of
    /// type — truncating a composed Text keeps the tail on the same line, and a
    /// stack of three views would not.
    ///
    /// Smaller and secondary, so it separates the two names rather than
    /// competing with them.
    static func route(_ route: Route) -> Text {
        guard let ends = route.ends else {
            // A loop or a shuttle, named for the one place it serves.
            return Text(route.title)
        }
        // Interpolated rather than concatenated: `Text + Text` is deprecated
        // from macOS 26. Interpolating a styled `Text` keeps what the sum was
        // for — the arrow carries its own size and colour, and the whole thing
        // is still one run of type that truncates on one line.
        let arrow = Text(Image(systemName: "arrow.left.arrow.right"))
            .font(.system(size: 9, weight: .semibold))
            .foregroundStyle(.secondary)
        return Text("\(ends.from)  \(arrow)  \(ends.to)")
    }
}
