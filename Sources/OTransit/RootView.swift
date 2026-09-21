// What the popover holds: a schedule, or the reason it does not hold one yet.
//
// The way out is placed here and not by each screen, so it cannot be in a
// toolbar on one and under a button on another, and a person who has learned
// where it is never has to look for it again.

import OTransitKit
import SwiftUI

struct RootView: View {
    @Bindable var schedule: Schedule

    /// Which screen the popover is showing, where that is not a place. See
    /// Navigation for why this is a model and not an action in the environment.
    @State private var navigation = Navigation()

    var body: some View {
        VStack(spacing: 0) {
            Group {
                if navigation.settings {
                    SettingsView { navigation.settings = false }
                } else {
                    Schedules(schedule: schedule)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)

            Rule()
            QuitRow()
        }
        // The first screen offers the way in, without four initialisers in
        // between learning about settings.
        .environment(navigation)
        .onAppear { schedule.tick() }
        .environment(schedule)
        .task { await schedule.check() }
        // What each kept board has today. One query a pin, when the popover
        // opens and not while it is looked at.
        .task { await schedule.resolve() }
        // What the city has published about a route. Its own task, because it
        // is a different question on a different clock.
        .task { await schedule.updates() }
        // Moves the clock. Runs whether or not anything is being polled.
        .task { await schedule.keepTime() }
        // Restarted when the key changes, because the key is what it is for.
        // No revision counter: the value is its own identity.
        .task(id: schedule.key) { await schedule.watch() }
    }

}

/// The program itself, in whichever state it is in.
///
/// Its own View and not a computed property on RootView. They read alike and
/// are not: a computed property is inlined into the enclosing body and shares
/// its invalidation boundary, so opening settings would re-evaluate this switch
/// and everything under it.
private struct Schedules: View {
    let schedule: Schedule

    var body: some View {
        Group {
            switch schedule.state {
            case .missing(let why):
                FirstRun(schedule: schedule, why: why)
            case .building(let phase):
                BuildingView(phase: phase)
            case .ready(let cache):
                BrowseView(
                    cache: cache, clock: schedule.clock, live: schedule.live,
                    hearing: schedule.hearing)
            case .failed(let why):
                Failure(why: why, schedule: schedule)
            }
        }
    }
}

/// The first run, and every run after someone deleted the file.
private struct FirstRun: View {
    let schedule: Schedule
    var why: Schedule.Absent = .never

    var body: some View {
        Centred {
            Mark(symbol: "square.and.arrow.down")

            Text(why == .never ? "No schedule yet" : "Schedule needs rebuilding")
                .font(.system(size: 15, weight: .semibold))
                .padding(.top, 18)

            // Someone who has used this before has not lost anything and should
            // not be told they are starting over.
            Text(
                why == .never
                    ? "otransit needs OC Transpo’s timetable before it can show departures."
                    : "This version reads the timetable differently. The download is the same one."
            )
            .font(.system(size: 12))
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .fixedSize(horizontal: false, vertical: true)
            .padding(.top, 6)

            Button("Download Schedule") { schedule.build() }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                .buttonBorderShape(.capsule)
                .keyboardShortcut(.defaultAction)
                .padding(.top, 20)

            // What it costs, and no claim about how often: OC Transpo
            // republishes on its own schedule, so this is not a one-time thing
            // and must not say it is.
            Text("About 30 MB")
                .font(.system(size: 10.5))
                .foregroundStyle(.tertiary)
                .padding(.top, 9)
        }
    }
}

private struct BuildingView: View {
    let phase: Building

    var body: some View {
        Centred {
            // The same mark in the same place as the screen before it, so
            // pressing the button moves nothing except the part it was about.
            Mark(symbol: "square.and.arrow.down", working: true)

            Text(phase.line)
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.top, 18)

            Group {
                if let fraction = phase.fraction {
                    ProgressView(value: fraction)
                        // Eased, because the download reports in bursts and a
                        // bar that jumps reads as a bar that is stuck.
                        .animation(.smooth(duration: 0.45), value: fraction)
                } else {
                    ProgressView()
                }
            }
            .progressViewStyle(.linear)
            .frame(width: 176)
            .padding(.top, 13)

            Text("Building the cache takes a few more seconds.")
                .font(.system(size: 10.5))
                .foregroundStyle(.tertiary)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.top, 9)
        }
    }
}

private struct Failure: View {
    let why: String
    let schedule: Schedule

    var body: some View {
        Centred {
            Mark(symbol: "exclamationmark.triangle", tint: .orange)

            Text("That did not work")
                .font(.system(size: 15, weight: .semibold))
                .padding(.top, 18)

            // What the server actually said, selectable, rather than a friendly
            // paraphrase that hides which feed broke.
            ScrollView {
                Text(why)
                    .font(.mono(.caption))
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(8)
            }
            .frame(height: 74)
            .background(
                Color.primary.opacity(0.05),
                in: RoundedRectangle(cornerRadius: Surface.inner(Surface.inset), style: .continuous)
            )
            .padding(.top, 13)

            Button("Try Again") { schedule.build() }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                .buttonBorderShape(.capsule)
                .padding(.top, 15)
        }
    }
}
