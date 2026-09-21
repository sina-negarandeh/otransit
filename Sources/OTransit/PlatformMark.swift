// The platform, drawn the way a station paints it.

import SwiftUI

/// The platform label on a plate.
///
/// `a.square.fill` is this shape and would draw nine of the eleven single
/// characters OC Transpo uses — but none of the fourteen pairs, and 57 of the
/// 213 stops with a platform are a pair. So the plate is drawn rather than
/// borrowed, on the symbol's own geometry: a square cornered at a quarter of
/// its side. `4C` is then the same object as `A` instead of a different one.
///
/// The label is a hole rather than a pale glyph, which is what the symbol does
/// and what a painted sign does. It means the plate is correct in both
/// appearances, and over the popover's material, without being told which.
struct PlatformMark: View {
    let platform: String

    /// How big to draw it. 18 is the cap height of the 13pt row label, so the
    /// plate sits in the line rather than on top of it; the trail asks for 15,
    /// which is the height the badges down there already are. Everything else
    /// is a fraction of this, so one number moves the whole thing.
    var side: CGFloat = 18

    /// Drawn only where there is a platform to draw. A stop without one shows
    /// nothing at all rather than an empty slot holding the space open.
    var body: some View {
        RoundedRectangle(cornerRadius: side / 4, style: .continuous)
            .fill(.primary)
            .frame(width: side, height: side)
            .overlay {
                // Two characters are set smaller to stay inside the square. The
                // alternative is a square that is sometimes a rectangle, which
                // reads as a mistake rather than as a second size.
                Text(platform)
                    .font(
                        .system(
                            size: side * (platform.count > 1 ? 0.5 : 0.667), weight: .bold,
                            design: .rounded)
                    )
                    .tracking(platform.count > 1 ? side * -0.017 : 0)
                    // Painted, not punched out. The symbol knocks the letter
                    // through to whatever is behind it, which here is the row —
                    // so the letter picked up the hover tint and shifted shade
                    // under the pointer, and the group needed to composite,
                    // which cost the plate the vibrancy the text beside it
                    // keeps. A sign is painted anyway: dark plate, light letter,
                    // and the other way round in the dark.
                    .foregroundStyle(Color(nsColor: .textBackgroundColor))
            }
            .accessibilityLabel("Platform \(platform)")
    }
}
