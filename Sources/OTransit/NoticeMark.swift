// What a published notice looks like before a word of it is read.
//
// Two screens draw this: the route list, where it is a mark beside a name, and
// the board, where it heads the notice itself. They agreed on the symbol and
// then each spelled the colour out, which is how a mark ends up two colours in
// the first place.
//
// Not in Palette.swift. That file is the colour rule and nothing else, and two
// of the three things here are English sentences.

import OTransitKit
import SwiftUI

extension Notice.Kind {
    /// The mark drawn beside a route this was published about.
    ///
    /// Two shapes and one colour. Orange says the operator has published
    /// something about this route, and the shape says which sort — the same
    /// division the route badges make, where the shape is the kind of service
    /// and the colour is which route. Red is not free for the louder of the
    /// two: it means a cancelled trip on a board, one screen further in.
    var symbol: String {
        switch self {
        // A route sent somewhere else.
        case .detour: "arrow.triangle.turn.up.right.diamond.fill"
        // A route running where it always does, under a limitation.
        case .alert: "exclamationmark.triangle.fill"
        }
    }

    /// The one colour both marks are drawn in.
    ///
    /// Amber and not red. Forty of the hundred and seventy-six routes running
    /// today carry something, so this is a common state and not an alarm. Red
    /// belongs to the two-minute wait, where it means leave now.
    var tint: Color { .orange }

    /// What a screen reader says in place of the mark.
    var spoken: String {
        switch self {
        case .detour: "Has a detour"
        case .alert: "Has a service alert"
        }
    }

    /// Why the mark is there, for the pointer.
    var reason: String {
        switch self {
        case .detour: "OC Transpo has published a detour for this route"
        case .alert: "OC Transpo has published a service alert for this route"
        }
    }
}
