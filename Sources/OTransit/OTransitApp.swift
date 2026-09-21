// The menu bar, and the one window behind it.
//
// This is the shell. It owns the status item, the popover and the clock, and it
// decides nothing about transit: every question it asks is answered by
// OTransitKit, which has no window and is the half the tests can reach.

import OTransitKit
import SwiftUI

@main
struct OTransitApp: App {
    @State private var schedule = Schedule()
    /// Does nothing unless OTRANSIT_PREVIEW is set. See Preview.swift.
    @NSApplicationDelegateAdaptor(Preview.self) private var preview

    var body: some Scene {
        MenuBarExtra {
            RootView(schedule: schedule)
                // A popover sizes to its content, and a list of departures has
                // no natural width. This is the width the longest stop name in
                // the feed fits in without wrapping.
                .frame(width: Popover.width, height: Popover.height)
        } label: {
            // An SF Symbol, so it is drawn as a template: black on a light menu
            // bar and white on a dark one, at the weight the bar is set to,
            // without this asking about either.
            Image(systemName: "tram.circle")
        }
        .menuBarExtraStyle(.window)
    }
}
