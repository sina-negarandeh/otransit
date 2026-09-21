// Two typefaces, and the rule for which is which.
//
// SF Pro is everything a person reads as language: a stop name, a headsign, a
// heading, a sentence explaining what the app is doing. It is the system font,
// so it is also what every unstyled view here already uses, and nothing needs
// to ask for it by name.
//
// SF Mono is everything a person reads as a value: a time, a wait, a route
// badge, a stop code, and the text of an error. These are set monospaced
// because they are read in a column and compared down it. A time in SF Pro
// with tabular figures lines its digits up but not its colon, and a route badge
// in it makes 7 narrower than 44, so a list of them has a ragged edge. The
// point is not that the glyphs are even; it is that the same thing is in the
// same place on every row.

import SwiftUI

extension Font {
    /// SF Mono at one of the system's text styles, so it scales with the
    /// person's text size the way the rest of the interface does. A fixed point
    /// size would not.
    static func mono(_ style: Font.TextStyle, weight: Font.Weight = .regular) -> Font {
        .system(style, design: .monospaced).weight(weight)
    }
}
