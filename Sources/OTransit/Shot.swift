// Photographing a screen, without anyone looking at it.
//
// The Rust and Go ports can prove what they draw: both have a replay shell that
// runs the whole program against a fixed clock and prints every frame, so a
// test can make a claim about a screen without a terminal. A menu bar popover
// has no such shell. It cannot be opened from a script, and a screenshot of one
// needs a person to click it and the machine's permission to record the screen.
//
// This is the nearest honest equivalent. The app draws its own views into its
// own window and copies that window's backing store to a PNG. Nothing here
// reads the display, so it needs no permission and disturbs nothing: the window
// is positioned off every screen and never comes to the front.
//
// It is how a change to a screen is checked, and how the README gets its
// pictures without staging one.
//
// What it cannot photograph is glass. A Liquid Glass surface is composited by
// the window server out of what lies behind it, and `cacheDisplay` copies only
// what this process drew — so a glass layer comes back as the hairline of its
// own edge and nothing else. That is a limit of the method and not a fault in
// the screen: a shot showing a control as invisible is evidence about this
// harness, and the only way to judge a material is to open the popover. Layout,
// wording, colour and every row of real data are photographed faithfully.

import AppKit
import OTransitKit
import SwiftUI

@MainActor
enum Shot {
    /// The screens worth a picture, in the order a person meets them. `app` and
    /// `browse` read the cache on disk; the rest are states asked for directly,
    /// because waiting for a download to fail is not a way to check a screen.
    /// How long a screen is given to finish drawing before it is photographed.
    ///
    /// A view that loads is not finished when it appears: the browse screen
    /// asks the cache for every route running today, and photographed too early
    /// it is an empty list. This is slow enough for that query and fast enough
    /// that a full set is a few seconds.
    private static let settle = Duration.milliseconds(1_400)

    static func run(_ names: [String], into directory: URL) async -> Int32 {
        do {
            try FileManager.default.createDirectory(
                at: directory, withIntermediateDirectories: true)
        } catch {
            note("cannot write to \(directory.path(percentEncoded: false)): \(error)")
            return 1
        }

        var failures = 0
        for name in names {
            do {
                for (file, screen) in try await Screens.named(name) {
                    let url = directory.appendingPathComponent("\(file).png")
                    let size = try await capture(screen, to: url)
                    note("  \(file).png  \(Int(size.width))×\(Int(size.height))")
                }
            } catch {
                note("\(name): \(error)")
                failures += 1
            }
        }
        return failures == 0 ? 0 : 1
    }

    /// Draws the view in a window of the popover's exact size and copies what
    /// the window drew.
    ///
    /// `cacheDisplay` reads this process's own backing store rather than the
    /// display, which is why this works with the screen locked, over ssh, and
    /// without the recording permission a screenshot needs.
    private static func capture(_ content: some View, to url: URL) async throws -> CGSize {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: Popover.width, height: Popover.height),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false)
        // An opaque backing, which is not a detail of the photograph but a
        // condition of it.
        //
        // From macOS 26 the label colours are vibrant: they are resolved against
        // whatever is behind them rather than being fixed greys. Drawn into a
        // transparent window with nothing behind, they resolve to nothing, and
        // the picture comes back holding only the text that named its own colour
        // — the green minutes and the route badge — on an empty page. A real
        // popover always has a desktop behind it. This is the nearest still
        // backdrop to one.
        window.isOpaque = true
        window.backgroundColor = .windowBackgroundColor

        // Held to the popover's size, because the scene holds it to that size
        // too. Left to grow, a screen photographs at whatever height its content
        // wants and the picture is of a popover that does not exist.
        // As in Preview: this window is ordered out and never closed, and an
        // NSWindow that releases itself on close is a different lifecycle rule
        // for two windows built for the same job.
        window.isReleasedWhenClosed = false
        window.contentView = NSHostingView(
            rootView: ZStack {
                Color(nsColor: .windowBackgroundColor)
                content
            }.frame(width: Popover.width, height: Popover.height))
        // Off every display, and never made key. An app photographing itself
        // must not take the focus of whoever is using the machine.
        window.setFrameOrigin(NSPoint(x: -30_000, y: -30_000))
        window.orderFront(nil)
        defer { window.orderOut(nil) }

        try await Task.sleep(for: settle)

        guard let view = window.contentView,
            let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds)
        else {
            throw ShotError("the window drew nothing")
        }
        view.cacheDisplay(in: view.bounds, to: bitmap)

        guard let png = bitmap.representation(using: .png, properties: [:]) else {
            throw ShotError("the drawing would not encode as a PNG")
        }
        try png.write(to: url)
        return CGSize(width: bitmap.pixelsWide, height: bitmap.pixelsHigh)
    }

    private static func note(_ line: String) {
        FileHandle.standardError.write(Data((line + "\n").utf8))
    }
}

struct ShotError: Error, CustomStringConvertible {
    let description: String
    init(_ description: String) { self.description = description }
}
