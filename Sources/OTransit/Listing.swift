// The parts every list screen is built from: the scroller they sit in, what a
// list says when it is empty, and the load that fills one.
//
// None of this knows anything about transit. It lived in BrowseView.swift
// because that is where the first list needed it, which made a file named for
// one screen the home of the infrastructure behind five.

import OTransitKit
import SwiftUI

struct Listing<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        ScrollView {
            VStack(spacing: 1) { content }
                // Less than a rule's inset, so a hovered row reaches past the
                // lines above and below it.
                .padding(.horizontal, 4)
                .padding(.top, 4)
                // So the last row scrolls clear of the bar rather than being
                // sliced in half by it.
                .padding(.bottom, 6)
        }
    }
}

/// What a screen says when it has nothing to show.
///
/// One value rather than three loose parameters. The title, the line under it
/// and the symbol are one description of one situation, and passing them
/// separately meant every caller could get two of the three right.
struct Vacancy {
    let title: String
    var note: String?
    var symbol = "clock"
}

/// The three states any screen built from a query has, so no screen invents
/// its own: what went wrong, nothing to show, or the thing itself.
struct Outcome<Item, Content: View>: View {
    let rows: [Item]
    let failure: String?
    let vacancy: Vacancy
    @ViewBuilder let content: Content

    var body: some View {
        if let failure {
            ScrollView {
                Text(failure)
                    .font(.mono(.caption))
                    .foregroundStyle(.secondary)
                    .padding()
            }
        } else if rows.isEmpty {
            // Indistinguishable from "still loading" for the first instant,
            // which is the right trade: these queries are indexed, and a
            // spinner that flashes for 20ms is noise.
            Empty(title: vacancy.title, note: vacancy.note, symbol: vacancy.symbol)
        } else {
            content
        }
    }
}

/// A screen that is a question put to the cache.
///
/// It owns the asking: the rows, the failure, and the task that fills them.
/// Three screens used to keep that state themselves, which meant three copies
/// of two `@State` properties, a `do`/`catch` that stringified the error, and a
/// `.task(id:)` — the same eight lines with different nouns. `Outcome` rendered
/// the three states but owned none of them, which is why the duplication
/// survived having an abstraction next to it.
struct Query<Item, Key: Equatable, Content: View>: View {
    /// What the answer depends on. A new one asks again.
    let key: Key
    let vacancy: Vacancy
    let ask: () async throws -> [Item]
    @ViewBuilder let content: ([Item]) -> Content

    @State private var rows: [Item] = []
    @State private var failure: String?

    var body: some View {
        Outcome(rows: rows, failure: failure, vacancy: vacancy) {
            content(rows)
        }
        .task(id: key) {
            do {
                rows = try await ask()
                failure = nil
            } catch {
                failure = String(describing: error)
            }
        }
    }
}
