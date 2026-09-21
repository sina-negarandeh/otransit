// What the reader does with the shapes a real export contains.

import Foundation
import Testing

@testable import OTransitKit

@Suite("CSV")
struct CSVTests {
    private func rows(_ text: String) -> [[String]] {
        var out: [[String]] = []
        let parser = CSVParser { out.append($0) }
        let bytes = Array(text.utf8)
        bytes.withUnsafeBytes { parser.feed($0) }
        parser.finish()
        return out
    }

    @Test("plain rows, either line ending, and a last row without one")
    func plain() {
        #expect(rows("a,b\r\n1,2\n3,4") == [["a", "b"], ["1", "2"], ["3", "4"]])
    }

    @Test("a quoted field keeps its commas")
    func quotedComma() {
        // Stop names carry commas, and a reader that splits on every one of them
        // shifts every column after it.
        #expect(rows("\"BANK / SLATER, NS\",3034") == [["BANK / SLATER, NS", "3034"]])
    }

    @Test("a doubled quote inside a quoted field is one quote")
    func escapedQuote() {
        #expect(rows("\"say \"\"hi\"\"\",2") == [["say \"hi\"", "2"]])
    }

    @Test("an empty field is a field")
    func empties() {
        #expect(rows("1,,3\n") == [["1", "", "3"]])
    }

    @Test("a row split across two feeds is one row")
    func straddling() {
        // The decompressor hands over whatever size it likes, and a row that
        // crosses the boundary must not become two.
        var out: [[String]] = []
        let parser = CSVParser { out.append($0) }
        for piece in ["sto", "p_id,name\n10", "01,BILLINGS\n"] {
            let bytes = Array(piece.utf8)
            bytes.withUnsafeBytes { parser.feed($0) }
        }
        parser.finish()
        #expect(out == [["stop_id", "name"], ["1001", "BILLINGS"]])
    }

    @Test("a byte order mark is not part of the first column's name")
    func bom() {
        let header = Header(rows("\u{FEFF}stop_id,stop_name\n")[0])
        #expect(header.has("stop_id"))
    }

    @Test("columns are read by name, because the export reorders them")
    func byName() {
        let header = Header(["stop_name", "stop_id", "stop_lat"])
        let row = ["BILLINGS BRIDGE", "1001", "45.38"]
        #expect(header.string(row, "stop_id") == "1001")
        #expect(header.string(row, "stop_name") == "BILLINGS BRIDGE")
        // A column this export does not carry is absent, not an error.
        #expect(header.string(row, "platform_code") == "")
    }
}
