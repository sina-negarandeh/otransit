// A crumb's mark and its words, at whichever size is drawing them.
//
// Two places draw crumbs, and not in the same arrangement. The bottom bar
// strings them along one scrolling line with a separator between each; a kept
// row stacks two of them, the stop over the direction, with no separator at
// all. What a crumb *is* survives both: a mark in the route's colour and the
// words beside it. That much was written out twice, down to the same comment
// copied across explaining why the tint is not two colours, and what actually
// differed between the two was four numbers.

import OTransitKit
import SwiftUI

/// One crumb, less whatever hangs off it.
///
/// The platform plate is deliberately not here. In the bar it sits outside the
/// hover highlight and in a row it sits inside the name, and that is a fact
/// about the two places rather than about a crumb.
///
/// Its own View and not a computed property. The bar redraws this on every
/// hover and none of it reads the pointer: the mark, its colour and the words
/// are the same whether or not the pointer is over them.
struct CrumbLabel: View {
    let crumb: Crumb
    let metrics: Metrics
    /// Set on the crumb that is the supporting fact rather than the point.
    ///
    /// Which one that is depends on the arrangement: in the bar it is every
    /// crumb except the one you are standing on, and in a kept row it is the
    /// direction under the stop's name. Both want the same grey.
    var dimmed = false

    var body: some View {
        HStack(spacing: metrics.gap) {
            if let symbol = crumb.symbol {
                Image(systemName: symbol)
                    .font(.system(size: metrics.symbol, weight: metrics.symbolWeight))
                    .foregroundStyle(Color.crumb(crumb.tint))
            }
            if !crumb.label.isEmpty {
                Text(crumb.label)
                    .font(.system(size: metrics.label))
                    .lineLimit(1)
                    .truncationMode(.tail)
            }
        }
        .foregroundStyle(dimmed ? AnyShapeStyle(.secondary) : AnyShapeStyle(.primary))
    }

    /// The four numbers that separate one place drawing a crumb from another.
    struct Metrics {
        let label: CGFloat
        let symbol: CGFloat
        let symbolWeight: Font.Weight
        let gap: CGFloat

        /// In the bottom bar, where a crumb is the furniture around a screen.
        static let chrome = Metrics(label: 10.5, symbol: 10, symbolWeight: .medium, gap: 4)

        /// In a list row, where a crumb is what the row is called and is read
        /// at the same weight as any other row's name.
        static let row = Metrics(label: 12, symbol: 9, symbolWeight: .semibold, gap: 3)

        /// On the line under that name, where a crumb is the supporting fact.
        /// The same eleven points every detail line in the app is set in.
        static let detail = Metrics(label: 11, symbol: 8.5, symbolWeight: .semibold, gap: 3)
    }
}
