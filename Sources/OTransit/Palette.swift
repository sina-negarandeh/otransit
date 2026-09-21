// The colour rule, as colours.
//
// The kit decides how urgent a wait is and this decides what that looks like,
// so the rule is testable and the shade is not something a test has an opinion
// about. Red at two minutes or less, amber at six, green at fifteen, dim past
// that: the board answers "do I need to leave now" before a number is read.

import OTransitKit
import SwiftUI

extension Urgency {
    var colour: Color {
        switch self {
        case .imminent: .red
        case .soon: .orange
        case .comfortable: .green
        // Secondary rather than a grey of its own, so it is the system's idea
        // of quiet in both appearances.
        case .later: .secondary
        // Already gone. It cannot reach the board, which filters what has
        // passed, but a colour rule with a hole in it breaks the day something
        // else calls it.
        case .gone: .secondary
        }
    }
}

extension Color {
    /// A route badge's two colours: the fill the feed asked for, and something
    /// legible on it.
    ///
    /// 84 of the 175 routes ship `route_color` as FFFFFF, which is the feed's
    /// way of saying it has none — the school, rural and special services.
    /// Drawn literally that is a white pill: a glare on a dark popover and
    /// invisible on a light one, on nearly half the network. Those take the
    /// surface's own neutral instead, which is legible on either.
    ///
    /// The feed's `route_text_color` is not read. It is not legible on its own
    /// background often enough to be trusted, and luminance is.
    static func badge(_ hex: String) -> (fill: Color, text: Color) {
        let clean = hex.trimmingCharacters(in: .whitespaces)
        guard clean.count == 6, let value = Int(clean, radix: 16) else {
            return (unsetBadge, .primary)
        }
        let red = Double((value >> 16) & 0xFF) / 255
        let green = Double((value >> 8) & 0xFF) / 255
        let blue = Double(value & 0xFF) / 255

        // The standard luminance, which is what decides both questions: whether
        // this is a colour at all, and what can be read on it.
        let luminance = 0.299 * red + 0.587 * green + 0.114 * blue
        guard luminance <= 0.93 else { return (unsetBadge, .primary) }

        return (
            Color(.sRGB, red: red, green: green, blue: blue),
            luminance > 0.6 ? .black : .white
        )
    }

    /// What a route with no colour of its own is drawn in.
    private static var unsetBadge: Color { Color.primary.opacity(0.14) }
}

extension Status {
    /// How much of this row's time to believe.
    ///
    /// A shape style rather than a colour, because two of the five are the
    /// system's own quiet grey and nothing else is that grey in both
    /// appearances.
    var colour: AnyShapeStyle {
        switch self {
        case .cancelled: AnyShapeStyle(.red)
        case .onTime: AnyShapeStyle(.green)
        case .late: AnyShapeStyle(.orange)
        // Early and scheduled are both quiet: one is a bus you have less time
        // for than you thought, the other is no news at all, and neither is
        // something to look at first.
        case .early, .scheduled: AnyShapeStyle(.tertiary)
        }
    }
}

/// The band under the pointer, and the rule between two things.
///
/// Both were written out wherever they were needed, and drifted: the same
/// gesture was drawn five ways — .08 at radius 6 on a row, .06 at 5 on a
/// section heading, .08 at 5 on the back button, .08 at 4 on a trail crumb, and
/// .07 with square corners across the whole width on Quit. One of them is now
/// all of them.
///
/// The band is wider than the rules it sits between, which is the whole of the
/// effect: a rule that runs edge to edge is the widest line on the screen, and
/// a row highlighted inside it reads as sitting *under* something. Pull the
/// rules in to the content they separate and the row becomes the widest thing
/// under the pointer, which is what it is.
enum Surface {
    /// How much of the label colour a hovered band carries.
    static let band = 0.075

    /// The popover's own corner.
    ///
    /// Everything rounded inside it is measured from this rather than chosen,
    /// so the curves nest: a band inset four points from the edge rounds at
    /// twelve minus four, and its curve runs parallel to the window's instead
    /// of crossing it. Two arcs that nearly agree read worse than two that
    /// plainly differ, which is why the number is derived and not picked.
    ///
    /// SwiftUI has two types for this and neither replaces the subtraction.
    /// Both were tried, drawn, and measured off the shot before this was
    /// written down.
    ///
    /// `ConcentricRectangle` does not read a `containerShape`. It takes the
    /// radius of the real container it is drawn in, and a plain window reports
    /// none — so inside one it is a square-cornered rectangle, and declaring a
    /// container of 40 beside it changed nothing. It would come right in a real
    /// popover and square everywhere else, which is a corner nobody can check.
    ///
    /// `ContainerRelativeShape` does read one: against a declared container of
    /// 40 the error box came back at 14pt, so the derivation works. What it
    /// derives is the honest answer, and the honest answer is not always the
    /// one to draw — that box sits 24 points inside the popover, and 12 − 24
    /// clamps to square. Concentricity binds a surface that hugs the container's
    /// corner, and says nothing about a panel floating well inside it.
    ///
    /// So: derived where a surface is near the edge, chosen where it is not.
    static let corner: CGFloat = 12

    /// How far a rule stops short of the popover's edge. The band is inset
    /// less than this, on purpose.
    static let ruleInset: CGFloat = 12

    /// The height the path bar and the quit row share.
    static let barHeight: CGFloat = 35

    /// How far the contents of the popover sit from its edge.
    static let inset: CGFloat = 4

    /// The corner for something `inset` points inside the popover.
    static func inner(_ inset: CGFloat) -> CGFloat { max(2, corner - inset) }
}

extension View {
    /// Lights this view as the thing under the pointer.
    ///
    /// The fill is the same everywhere, which is what was actually wrong: five
    /// opacities for one gesture. The corner is not, and should not be — a 7pt
    /// radius on the 15pt chip in the trail is a capsule, and on the 20pt back
    /// button it is a lozenge. A corner belongs to the size of the thing it is
    /// rounding, so the two small chips ask for their own.
    func band(_ showing: Bool, corner: CGFloat = Surface.inner(Surface.inset)) -> some View {
        background(
            showing ? Color.primary.opacity(Surface.band) : .clear,
            in: RoundedRectangle(cornerRadius: corner, style: .continuous)
        )
    }
}

/// A line between two things, stopping short of the edge so the band can pass
/// it. `Divider()` cannot: it draws the full width it is given.
struct Rule: View {
    var body: some View {
        Rectangle()
            .fill(Color(nsColor: .separatorColor))
            .frame(height: 0.5)
            .padding(.horizontal, Surface.ruleInset)
    }
}
