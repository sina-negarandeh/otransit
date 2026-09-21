// Which screen the popover is showing, where that screen is not a place.
//
// `Place` is the transit hierarchy — what you are taking, which route, which
// way, which stop — and every case of it is an answer about a journey. Settings
// is not one, so it is not a case of that enum and it is held here instead.
//
// A model rather than an action in the environment. The way in used to be a
// closure, then a struct wrapping a closure, and Apple's guidance rules out
// both: a closure cannot be compared, so every view reading it invalidates on
// every pass, and putting it inside a struct keeps the closure and therefore
// keeps the problem. A class is compared by identity, which does not change
// while the app is running.

import SwiftUI

@MainActor
@Observable
final class Navigation {
    /// Whether settings is showing. It replaces the whole popover rather than
    /// sitting beside it: there is no room at 320 points for two things at once.
    var settings = false
}
