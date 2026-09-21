// A CSV reader that is fed bytes and gives back rows.
//
// Push rather than pull, because the bytes arrive from a decompressor in
// whatever sizes it chooses and a row can straddle two of them. The parser
// holds the half of a row it has seen and finishes it when the rest arrives.
//
// Nothing here is general. It reads what GTFS is written in: comma separated,
// optionally quoted, "" for a quote inside a quoted field, and either line
// ending. A file it cannot read is a file the export does not contain.

import Foundation

public final class CSVParser {
    private var field: [UInt8] = []
    private var row: [String] = []
    private var quoted = false
    /// Set just after a quote inside a quoted field, so the next byte decides
    /// whether it closed the field or was the first of an escaped pair.
    private var pendingQuote = false
    private var atStart = true

    private let onRow: ([String]) -> Void

    public init(onRow: @escaping ([String]) -> Void) {
        self.onRow = onRow
        field.reserveCapacity(64)
        row.reserveCapacity(16)
    }

    public func feed(_ bytes: UnsafeRawBufferPointer) {
        for byte in bytes {
            // A byte order mark, which some exports write and no parser should
            // hand back as part of the first column's name.
            if atStart {
                atStart = false
                if byte == 0xEF { continue }
            }
            if byte == 0xBB || byte == 0xBF, field.isEmpty, row.isEmpty, !quoted { continue }

            if pendingQuote {
                pendingQuote = false
                if byte == 0x22 {
                    field.append(byte)
                    continue
                }
                quoted = false
            }

            if quoted {
                if byte == 0x22 {
                    pendingQuote = true
                } else {
                    field.append(byte)
                }
                continue
            }

            switch byte {
            case 0x22 where field.isEmpty: quoted = true
            case 0x2C: endField()
            case 0x0A: endRow()
            case 0x0D: break  // The LF that follows ends the row.
            default: field.append(byte)
            }
        }
    }

    /// Ends the last row, which a file that does not end in a newline still has.
    public func finish() {
        if !field.isEmpty || !row.isEmpty { endRow() }
    }

    private func endField() {
        row.append(String(decoding: field, as: UTF8.self))
        field.removeAll(keepingCapacity: true)
    }

    private func endRow() {
        endField()
        onRow(row)
        row.removeAll(keepingCapacity: true)
    }
}

/// Which column is which, by the name in the header row.
///
/// GTFS does not fix column order, and two exports of the same feed have
/// disagreed about it. Reading by position is the bug that puts a stop's
/// latitude in its name six months after anyone last looked.
public struct Header {
    private let index: [String: Int]

    public init(_ names: [String]) {
        var index: [String: Int] = [:]
        for (i, name) in names.enumerated() {
            index[name.trimmingCharacters(in: .whitespaces)] = i
        }
        self.index = index
    }

    /// The value of a column, or "" when this export does not carry it. Optional
    /// columns are most of GTFS, and an absent `platform_code` is not an error.
    public func string(_ row: [String], _ column: String) -> String {
        guard let at = index[column], at < row.count else { return "" }
        return row[at]
    }

    public func int(_ row: [String], _ column: String) -> Int? {
        Int(string(row, column).trimmingCharacters(in: .whitespaces))
    }

    public func has(_ column: String) -> Bool { index[column] != nil }
}
