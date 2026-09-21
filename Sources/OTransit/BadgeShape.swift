// The shape OC Transpo gives each kind of service, and the badge drawn in it.
//
// A circle is an O-Train line, a pointed flag a Frequent route, a capsule a
// Connexion route, a rectangle a Local one. Riders read those shapes off every
// map and every pole in the city, so a badge that borrows them says what kind
// of route this is before its number is read — and it keeps saying it in
// greyscale, which colour alone does not.

import OTransitKit
import SwiftUI

/// The Frequent badge: an elongated hexagon, pointed at both ends.
struct Hexagon: Shape {
    /// How far in from each end the points begin, as a share of the height.
    /// Flat enough that three digits still sit between them.
    static let tipShare: CGFloat = 0.38

    func path(in rect: CGRect) -> Path {
        let tip = rect.height * Self.tipShare
        var path = Path()
        path.move(to: CGPoint(x: rect.minX + tip, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX - tip, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.midY))
        path.addLine(to: CGPoint(x: rect.maxX - tip, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.minX + tip, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.minX, y: rect.midY))
        path.closeSubpath()
        return path
    }
}

extension Service {
    /// The outline this kind of service is drawn in.
    ///
    /// Erased, because a switch over four different shapes is four different
    /// types and a badge has one slot to put them in.
    var badge: AnyShape {
        switch self {
        case .line: AnyShape(Circle())
        // A night route is a Frequent route running late, and the operator
        // draws it as the same hexagon in a different fill.
        case .frequent, .night: AnyShape(Hexagon())
        case .connexion: AnyShape(Capsule())
        case .local, .other, .school, .event, .shopper, .replacement:
            AnyShape(RoundedRectangle(cornerRadius: 2, style: .continuous))
        }
    }
}

struct Badge: View {
    let name: String
    let colour: String
    let service: Service

    /// Every badge is this tall, so a column of them has one baseline whatever
    /// shape each is. A line's circle takes its diameter from it.
    private static let height: CGFloat = 20

    /// And this wide at least — one width for every bus shape, so the names
    /// beside them start in the same place down the list. It is set by the
    /// hexagon, the widest of them: the points eat into it from both ends, and
    /// a badge sized only to its digits would not look elongated at all.
    static let width: CGFloat = 38

    init(_ name: String, colour: String, service: Service) {
        self.name = name
        self.colour = colour
        self.service = service
    }

    var body: some View {
        let (fill, ink) = Color.badge(colour)
        Text(name)
            .font(.mono(.caption, weight: .bold))
            .foregroundStyle(ink)
            // Room for a point at each end, so the hexagon's corners do not
            // crowd the digits between them.
            .padding(.horizontal, service == .frequent ? Self.height * Hexagon.tipShare : 5)
            .frame(
                minWidth: service == .line ? Self.height : Self.width,
                minHeight: Self.height
            )
            .background(fill, in: service.badge)
    }
}

/// A kind of service, wearing its own badge.
///
/// The shape and colour every route under a heading has, and the numbers they
/// are drawn from where there are any — so the heading is a legend for the
/// list beneath it rather than a word above it. Drawn rather than reusing
/// `Badge`: that one is sized for a route number at full height, and this sits
/// in a line of 11pt text.
struct ServiceMark: View {
    let service: Service

    /// Short enough to sit in a heading, and the same height as the badges in
    /// the trail so every small badge in the app is one size.
    private static let height: CGFloat = 15

    var body: some View {
        let (fill, ink) = Color.badge(service.colour)
        Text(service.numbering)
            .font(.system(size: 9, weight: .bold, design: .monospaced))
            .foregroundStyle(ink)
            // 301-305 and 400-459 are seven characters in a box drawn for four.
            // They give a little rather than widening the mark, because the
            // width is the whole point of it.
            .lineLimit(1)
            .minimumScaleFactor(0.75)
            .padding(.horizontal, service == .frequent ? Self.height * Hexagon.tipShare : 4)
            // Exactly as wide as a route's badge, so the name of a section and
            // the names of the routes inside it start at the same place. They
            // were ragged: a hexagon with nothing in it is 28 points and
            // 301-305 is 46, so every heading began its word somewhere else.
            .frame(width: Badge.width, height: Self.height)
            .background(fill, in: service.badge)
    }
}
