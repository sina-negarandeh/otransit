// A screen that is about one thing, and the pieces it is made of.
//
// Not a list and not a bar: the whole middle of the popover given over to one
// statement — no schedule yet, a download running, a failure, nothing more
// today. They share a layout so the mark does not move between them.

import SwiftUI

/// The layout the four states share: anchored below the top edge rather than
/// centred, so the mark does not move between them as the text below it grows
/// and shrinks.
struct Centred<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        VStack(spacing: 0) { content }
            .frame(maxWidth: .infinity, alignment: .center)
            .padding(.horizontal, 24)
            .padding(.top, 64)
            .frame(maxHeight: .infinity, alignment: .top)
    }
}

/// The symbol at the top of a screen that is about one thing.
///
/// Set in a soft tile rather than floating on the background: a glyph alone is
/// a picture of a thing, and the same glyph on a surface is part of a system.
struct Mark: View {
    let symbol: String
    var working = false
    var tint: Color = .secondary

    var body: some View {
        Image(systemName: symbol)
            .font(.system(size: 25, weight: .regular))
            .symbolRenderingMode(.hierarchical)
            .foregroundStyle(tint)
            // Breathes while it works, so a long download has something alive
            // on it that is not the number.
            .symbolEffect(.pulse, isActive: working)
            .frame(width: 60, height: 60)
            // Drawn, and deliberately not `.glassEffect`.
            //
            // From macOS 26 the popover is itself glass, and glass inside glass
            // is the one thing the material is documented not to do: the inner
            // layer has the outer one behind it rather than the desktop, finds
            // nothing to refract, and comes out as a hairline around an icon.
            // That was tried here and photographed, which is why this comment
            // exists rather than the effect.
            //
            // A fill and a hairline, at percentages of the label colour, follow
            // the appearance on their own.
            .background(
                Color.primary.opacity(0.06),
                in: RoundedRectangle(cornerRadius: 16, style: .continuous)
            )
            .overlay {
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .strokeBorder(Color.primary.opacity(0.07), lineWidth: 0.5)
            }
    }
}

/// A screen with nothing on it: the same mark, title and note the first-run and
/// failure screens use.
///
/// `ContentUnavailableView` was here, and it is built for a window: a 28pt
/// title centred in the whole height, which in a 320-point popover is the
/// largest text in the app announcing the least. This says the same thing in
/// the voice of the screen it is standing in for.
struct Empty: View {
    let title: String
    var note: String?
    var symbol = "clock"

    var body: some View {
        Centred {
            Mark(symbol: symbol)

            Text(title)
                .font(.system(size: 15, weight: .semibold))
                .padding(.top, 18)

            if let note {
                Text(note)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, 6)
            }
        }
    }
}
