// Looking at a screen without clicking the menu bar.
//
// A menu bar popover cannot be opened from a script, and no screen here is
// finished the first time it is drawn. So when OTRANSIT_PREVIEW names a state,
// the app puts that screen in an ordinary window instead and behaves like an
// ordinary app for as long as it is running.
//
// Nothing in the shipped path reads this. With the variable unset the delegate
// returns immediately and the app is the agent Info.plist says it is.

import AppKit
import OTransitKit
import SwiftUI

@MainActor
final class Preview: NSObject, NSApplicationDelegate {
    private var window: NSWindow?

    func applicationDidFinishLaunching(_ notification: Notification) {
        if Headless.asked {
            Task { exit(await Headless.run()) }
            return
        }

        guard let name = ProcessInfo.processInfo.environment["OTRANSIT_PREVIEW"] else { return }
        // The catalogue is Screens'. A name that asks for several — `browse` is
        // four — shows the first; the rest are what `otransit shot` is for.
        Task {
            guard let screen = try? await Screens.named(name).first else { return }
            show(name, screen.1)
        }
    }

    /// Puts a view in an ordinary window at the popover's exact size.
    ///
    /// An agent app has no windows and cannot be brought to the front. For as
    /// long as this is a preview it is an ordinary app, so the window can be
    /// focused and photographed. One place opens it, so the size here and the
    /// size the popover actually uses cannot drift apart.
    private func show(_ name: String, _ content: some View) {
        NSApp.setActivationPolicy(.regular)
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: Popover.width, height: Popover.height),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false)
        window.title = "otransit · \(name)"
        window.contentView = NSHostingView(rootView: content)
        window.isReleasedWhenClosed = false
        window.center()
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        self.window = window
    }
}
