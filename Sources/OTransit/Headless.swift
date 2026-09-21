// `otransit update`, for a terminal.
//
// The same work the popover's button does, without the popover: it downloads
// the published export, builds the cache and exits. It exists because a menu
// bar app cannot be driven from a script, and the download and ingest are the
// longest, least reversible thing this program does — so there has to be a way
// to run them where the output is readable and the exit code means something.

import Foundation
import OTransitKit
import os

enum Headless {
    /// Whether the process was asked to do something rather than open a menu bar.
    static var asked: Bool { verb != nil }

    static var verb: String? {
        let words = CommandLine.arguments.dropFirst()
        return ["update", "realtime", "detours", "shot"].first { words.contains($0) }
    }

    static func run() async -> Int32 {
        switch verb {
        case "update": await update()
        case "realtime": await realtime()
        case "detours": await detours()
        case "shot": await shot()
        default: 0
        }
    }

    /// `otransit realtime` — one fetch of the trip updates, summarised.
    ///
    /// The endpoint has beta in its path and no test may touch a socket, so
    /// this is the only place the parser meets the real feed. A parser that has
    /// quietly stopped matching draws exactly like a quiet Sunday; this is what
    /// tells the two apart.
    static func realtime() async -> Int32 {
        guard let key = Key.read() else {
            say("no subscription key at \(Paths.key.path(percentEncoded: false))")
            return 1
        }
        do {
            let feed = try await Feed.trips(key: key)
            let now = Int(Date().timeIntervalSince1970)
            say("\(feed.note(now: now)) · \(feed.count) predictions")
            return feed.count > 0 ? 0 : 1
        } catch {
            say("failed: \(error)")
            return 1
        }
    }

    /// `otransit detours` — one fetch of the updates feed, counted.
    ///
    /// The feed is a content system that emits RSS, where an item tagged
    /// `Detours` carrying `affectedRoutes-` is that system's convention and not
    /// a contract. If either spelling changes, every item still parses, nothing
    /// matches, and every screen shows no detour, which looks exactly like a
    /// quiet week. This is what tells those apart: it counts what the feed held
    /// against what this program understood, and refuses when the second is
    /// zero and the first is not.
    static func detours() async -> Int32 {
        do {
            let found = try await Feed.detours()
            say(
                "\(found.items) items, \(found.notices.count) detours, \(found.affected.count) routes"
            )
            if found.unreadable {
                say("the feed held items and this program understood none of them")
                say("check the Detours and affectedRoutes- tags in \(Feed.updates)")
                return 1
            }
            return 0
        } catch {
            say("failed: \(error)")
            return 1
        }
    }

    static func update() async -> Int32 {
        say("building \(Paths.cache.path(percentEncoded: false))")
        let started = Date()
        // Every 250,000 rows is a lot of lines for one file, so only a phase
        // that says something new earns one. Locked rather than a plain var:
        // nothing promises the progress callback arrives on one thread, and
        // Swift 6 is right to refuse the version that assumed it did.
        let last = OSAllocatedUnfairLock(initialState: "")

        do {
            try await Feed.build(into: Paths.cache) { phase in
                let line = phase.line
                let fresh = last.withLock { held -> Bool in
                    guard held != line else { return false }
                    held = line
                    return true
                }
                if fresh { say("  " + line) }
            }
            say(String(format: "schedule updated in %.1fs", Date().timeIntervalSince(started)))
            return 0
        } catch {
            say("failed: \(error)")
            return 1
        }
    }

    /// `otransit shot [screen...] [--into <directory>]` — a PNG of each screen
    /// named, or of every screen there is. See Shot.swift for why this exists.
    ///
    /// The directory is named by a flag and not by position. Taking the first
    /// word as a path made `otransit shot browse` — the obvious way to ask for
    /// one screen — mean "draw all nine into a folder called browse", and
    /// nothing said so.
    static func shot() async -> Int32 {
        let words = Array(CommandLine.arguments.dropFirst())
        guard let start = words.firstIndex(of: "shot") else { return 1 }
        var rest = Array(words[words.index(after: start)...])

        var directory = URL(fileURLWithPath: "shots", isDirectory: true)
        if let flag = rest.firstIndex(of: "--into") {
            let value = rest.index(after: flag)
            guard value < rest.endIndex else {
                say("--into needs a directory after it")
                return 1
            }
            directory = URL(fileURLWithPath: rest[value], isDirectory: true)
            rest.removeSubrange(flag...value)
        }
        let names = rest.isEmpty ? await Screens.all : rest
        say("drawing into \(directory.path(percentEncoded: false))")
        return await Shot.run(names, into: directory)
    }

    private static func say(_ line: String) {
        FileHandle.standardError.write(Data((line + "\n").utf8))
    }
}
