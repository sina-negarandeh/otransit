// What a kept board is, and the file the kept ones live in.
//
// A pin is a board and not a stop, so two pins can name one pole. Every case
// here is one the feed produces: opposite directions on a single platform, a
// name carrying slashes and commas, a file somebody edited by hand.

import Foundation
import Testing

@testable import OTransitKit

@Suite("Pins")
struct PinTests {
    private let rideau = Pin(
        stop: "3021", code: "3021", name: "UOTTAWA A", route: "56",
        headsign: "Tunney's Pasture")
    private let back = Pin(
        stop: "3021", code: "3021", name: "UOTTAWA B", route: "56",
        headsign: "King Edward")

    @Test("two directions of one route at one pole are two pins")
    func identity() {
        // The stop alone is not enough. Nine routes serve the single pole at
        // TRANSITWAY / TERMINAL and they run to eight destinations.
        #expect(rideau.id != back.id)
        #expect(rideau != back)
        // And the same board is the same pin whatever its name is spelled as,
        // because nothing resolves through the name.
        let renamed = Pin(
            stop: "3021", code: "3021", name: "U OTTAWA A", route: "56",
            headsign: "Tunney's Pasture")
        #expect(renamed.id == rideau.id)
    }

    @Test("a pin written is the pin read back")
    func roundTrip() {
        let read = Pins.parse(Pins.render([rideau, back]))
        #expect(read == [rideau, back])
    }

    @Test("order is the order they were kept")
    func order() {
        // Not by next departure. A list that reorders itself is a list nobody
        // builds muscle memory for.
        #expect(Pins.parse(Pins.render([back, rideau])) == [back, rideau])
    }

    @Test("a name with commas and slashes needs no escaping")
    func awkwardNames() {
        let awkward = Pin(
            stop: "8852", code: "8852", name: "ST-LAURENT / AD. 1055, BAY 3",
            route: "12", headsign: "Blair")
        #expect(Pins.parse(Pins.render([awkward])) == [awkward])
    }

    @Test("one mangled line does not lose the pins above it")
    func tolerant() {
        // This is a file a person can edit. A line they broke costs that line.
        let file = """
            3021\t3021\tUOTTAWA A\t56\tTunney's Pasture
            this line is not a pin
            \t\t\t\t
            8852\t8852\tST-LAURENT\t12\tBlair
            """
        let read = Pins.parse(file)
        #expect(read.count == 2)
        #expect(read.first?.route == "56")
        #expect(read.last?.route == "12")
    }

    @Test("blank lines and comments are not broken lines")
    func skips() {
        let file = """
            # the boards I keep

            3021\t3021\tUOTTAWA A\t56\tTunney's Pasture

            """
        #expect(Pins.parse(file) == [rideau])
    }

    @Test("an empty list leaves no file behind")
    func emptied() throws {
        let file = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("otransit-pins-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: file) }

        try Pins.write([rideau], to: file)
        #expect(Pins.read(from: file) == [rideau])

        // Unpinning the last one removes the file rather than leaving an empty
        // one, so nothing has to tell those two apart later.
        try Pins.write([], to: file)
        #expect(!FileManager.default.fileExists(atPath: file.path(percentEncoded: false)))
        #expect(Pins.read(from: file).isEmpty)
    }

    @Test("no file yet is no pins, and not a failure")
    func absent() {
        let missing = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("otransit-pins-\(UUID().uuidString)")
        #expect(Pins.read(from: missing).isEmpty)
    }

    @Test("the room a screen has is what was measured for it")
    func room() {
        // A kept row is 41 points and three of them come to 130, which is what
        // the first screen has for them without squeezing the mode list above.
        #expect(Pins.room == 3)
    }

    @Test("room is answered against what can be drawn")
    func fits() {
        #expect(Pins.fits(drawable: 0))
        #expect(Pins.fits(drawable: Pins.room - 1))
        #expect(!Pins.fits(drawable: Pins.room))
        // Counted against what draws and not against the lines in the file, so
        // pins nothing can resolve today do not block one that would draw.
        #expect(!Pins.fits(drawable: Pins.room + 1))
    }

    @Test("a screen shows what fits and the file keeps the rest")
    func capped() {
        let many = (1...Pins.room + 2).map {
            Pin(stop: "\($0)", code: "\($0)", name: "Stop \($0)", route: "1", headsign: "Blair")
        }
        let shown = Pins.drawable(many)
        #expect(shown.count == Pins.room)
        // The first of them, in the order they were kept.
        #expect(shown.map(\.stop) == ["1", "2", "3"])
        // And nothing was taken out of the file to achieve it.
        #expect(Pins.parse(Pins.render(many)).count == Pins.room + 2)
    }
}
