// Which sections of the route list are folded, and the heading that folds them.
//
// Kept in the app's own preferences, so a person who folds the school routes
// away finds them folded tomorrow. A fold that resets every time the popover
// closes is a fold nobody makes twice.

import OTransitKit
import SwiftUI

@MainActor
@Observable
final class Sections {
    // Named afresh, because what is stored under it changed shape: the old
    // key holds display labels and this one holds Service.key. Anything saved
    // by an earlier version is left where it is and ignored, which costs a
    // person one fold and never reads a stale label as a live one.
    private static let key = "foldedServices"

    private var folded: Set<String>

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        // Nothing saved is a first run, not "nothing folded": School starts
        // folded, and only a person unfolding it should change that.
        if let saved = defaults.array(forKey: Self.key) as? [String] {
            folded = Set(saved)
        } else {
            folded = Set(Service.allCases.filter(\.startsClosed).map(\.key))
        }
    }

    private let defaults: UserDefaults

    func isFolded(_ service: Service) -> Bool { folded.contains(service.key) }

    func toggle(_ service: Service) {
        if folded.contains(service.key) {
            folded.remove(service.key)
        } else {
            folded.insert(service.key)
        }
        defaults.set(Array(folded).sorted(), forKey: Self.key)
    }
}

/// The heading of one section: what kind of service, how many, and whether the
/// routes under it are showing.
struct SectionHeading: View {
    let service: Service
    let count: Int
    let folded: Bool
    let toggle: () -> Void

    @State private var hovering = false

    var body: some View {
        Button(action: toggle) {
            HStack(spacing: 6) {
                // On the leading edge, which is where the route badges below
                // it start: the heading and the list it governs share a spine,
                // and the mark reads as the legend for the column under it.
                ServiceMark(service: service)
                // At full strength. Ten points and secondary put the name of a
                // kind of service a step below the routes it contains.
                Text(service.name)
                    .font(.system(size: 11, weight: .semibold))
                Spacer(minLength: 6)
                Text("\(count)")
                    .font(.mono(.caption2))
                    .foregroundStyle(.tertiary)
                Image(systemName: "chevron.forward")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(.secondary)
                    // Down when open, right when folded: the direction a
                    // disclosure points is the direction it would take you.
                    .rotationEffect(.degrees(folded ? 0 : 90))
            }
            .padding(.horizontal, 8)
            // Even, so the band drawn around this has its contents in the
            // middle. The air that separates one section from the one above is
            // taken outside the band instead, where it is space between two
            // things rather than a lopsided margin inside one.
            .padding(.vertical, 6)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .band(hovering)
        .padding(.top, 6)
        .onHover { hovering = $0 }
        // The count is the point of a folded heading: it says what is inside
        // without showing it, so nothing becomes unfindable by being folded.
        .help(folded ? "Show \(count) \(service.name.lowercased()) routes" : "Hide them")
    }
}
