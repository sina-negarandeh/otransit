// Reading the published export without unpacking it.
//
// The archive is about 30 MB and holds several hundred of CSV, and every byte
// read exactly once. So nothing is written to disk: an entry is inflated in
// 256 KB pieces straight into whoever asked for it, and the pieces are handed
// over as they appear. The Rust and Go ports unpack to a directory first, which
// costs the disk space twice and a pass over it.
//
// Only what a zip must have is read here. No encryption, no multi-disk, no
// data descriptors: the sizes come from the central directory, which the
// export writes and every zip writer writes.

import Compression
import Foundation

public struct Zip {
    public struct Entry: Sendable {
        public let name: String
        /// 0 stored, 8 deflated. Nothing else has ever appeared in this export,
        /// and anything else is refused rather than guessed at.
        let method: Int
        let compressedSize: Int
        let localHeader: Int
        /// What the archive says the uncompressed bytes add up to.
        let crc: UInt32
    }

    public enum Failure: Error, CustomStringConvertible {
        case notAZip
        case truncated
        case unsupported(String, method: Int)
        case missing(String)
        case corrupt(String)

        public var description: String {
            switch self {
            case .notAZip: "that file is not a zip archive"
            case .truncated: "the archive ends in the middle of itself"
            case .unsupported(let name, let method):
                "\(name) is compressed by method \(method), which this does not read"
            case .missing(let name): "the archive has no \(name)"
            case .corrupt(let name): "\(name) did not decompress"
            }
        }
    }

    private let data: Data
    public let entries: [Entry]

    /// Maps the archive and reads its central directory.
    ///
    /// Mapped rather than read: the pages are touched once, in order, and the
    /// kernel is better placed than this to decide when to let them go.
    public init(url: URL) throws {
        self.data = try Data(contentsOf: url, options: .mappedIfSafe)
        self.entries = try Self.directory(of: data)
    }

    public func entry(named name: String) throws -> Entry {
        guard let found = entries.first(where: { $0.name == name || $0.name.hasSuffix("/" + name) })
        else { throw Failure.missing(name) }
        return found
    }

    /// Inflates one entry, handing over each piece as it is produced.
    ///
    /// The pieces are not lines and not rows: a decompressor emits whatever it
    /// has, and a CSV row routinely straddles two of them. The reader above this
    /// is built to be fed that way.
    public func inflate(_ entry: Entry, into sink: (UnsafeRawBufferPointer) throws -> Void) throws {
        try data.withUnsafeBytes { raw in
            let start = try Self.dataStart(raw, entry)
            guard start + entry.compressedSize <= raw.count else { throw Failure.truncated }
            let body = UnsafeRawBufferPointer(rebasing: raw[start..<(start + entry.compressedSize)])

            // Checksummed on the way past.
            //
            // The archive is written to disk and read back in a separate step,
            // so transport security says nothing about what is inflated here. A
            // truncated entry that still decodes yields a short CSV, and a short
            // CSV is loaded without complaint — rows with missing fields are
            // tolerated by design — leaving a cache missing trips that draws
            // exactly like a quiet Sunday.
            var sum = CRC32()
            // A local function and not a closure in a `let`: the latter is
            // inferred escaping, and `sink` is not.
            func checked(_ chunk: UnsafeRawBufferPointer) throws {
                sum.update(chunk)
                try sink(chunk)
            }

            switch entry.method {
            case 0: try checked(body)
            case 8: try Self.deflate(body, entry.name, checked)
            default: throw Failure.unsupported(entry.name, method: entry.method)
            }

            guard sum.checksum == entry.crc else { throw Failure.corrupt(entry.name) }
        }
    }

    /// Streams raw DEFLATE through the system's decompressor.
    ///
    /// `COMPRESSION_ZLIB` is Apple's name for RFC 1951 with no wrapper, which is
    /// exactly what a zip entry holds. The zlib and gzip headers a name like
    /// that suggests are not there and must not be looked for.
    private static func deflate(
        _ body: UnsafeRawBufferPointer, _ name: String,
        _ sink: (UnsafeRawBufferPointer) throws -> Void
    ) throws {
        let stream = UnsafeMutablePointer<compression_stream>.allocate(capacity: 1)
        defer { stream.deallocate() }
        guard
            compression_stream_init(stream, COMPRESSION_STREAM_DECODE, COMPRESSION_ZLIB)
                == COMPRESSION_STATUS_OK
        else { throw Failure.corrupt(name) }
        defer { compression_stream_destroy(stream) }

        let capacity = 256 * 1024
        let out = UnsafeMutablePointer<UInt8>.allocate(capacity: capacity)
        defer { out.deallocate() }

        stream.pointee.src_ptr = body.baseAddress!.assumingMemoryBound(to: UInt8.self)
        stream.pointee.src_size = body.count
        stream.pointee.dst_ptr = out
        stream.pointee.dst_size = capacity

        while true {
            // Everything there is to read is already in front of it, so the
            // finalize flag is set from the start: there is no more source
            // coming and the decompressor may end whenever the data does.
            let status = compression_stream_process(
                stream, Int32(COMPRESSION_STREAM_FINALIZE.rawValue))
            let produced = capacity - stream.pointee.dst_size
            if produced > 0 {
                try sink(UnsafeRawBufferPointer(start: out, count: produced))
                stream.pointee.dst_ptr = out
                stream.pointee.dst_size = capacity
            }
            switch status {
            case COMPRESSION_STATUS_OK: continue
            case COMPRESSION_STATUS_END: return
            default: throw Failure.corrupt(name)
            }
        }
    }

    /// Where an entry's bytes begin. The local header's extra field is allowed
    /// to differ in length from the central one's, so the local header is the
    /// only thing that can answer this.
    private static func dataStart(_ raw: UnsafeRawBufferPointer, _ entry: Entry) throws -> Int {
        guard entry.localHeader + 30 <= raw.count,
            u32(raw, entry.localHeader) == 0x0403_4B50
        else { throw Failure.truncated }
        return entry.localHeader + 30 + u16(raw, entry.localHeader + 26)
            + u16(raw, entry.localHeader + 28)
    }

    private static func directory(of data: Data) throws -> [Entry] {
        try data.withUnsafeBytes { raw in
            let end = try endRecord(raw)
            var at = end.offset
            var entries: [Entry] = []
            entries.reserveCapacity(end.count)

            for _ in 0..<end.count {
                guard at + 46 <= raw.count, u32(raw, at) == 0x0201_4B50 else {
                    throw Failure.truncated
                }
                let nameLength = u16(raw, at + 28)
                let extraLength = u16(raw, at + 30)
                let commentLength = u16(raw, at + 32)
                guard at + 46 + nameLength <= raw.count else { throw Failure.truncated }

                let name = String(
                    decoding: UnsafeRawBufferPointer(
                        rebasing: raw[(at + 46)..<(at + 46 + nameLength)]),
                    as: UTF8.self)
                var compressed = u32(raw, at + 20)
                var header = u32(raw, at + 42)

                // Zip64 keeps the real numbers in an extra field and writes all
                // ones where they used to be. The export is under 4 GB today,
                // and a writer is free to use zip64 anyway.
                if compressed == 0xFFFF_FFFF || header == 0xFFFF_FFFF {
                    let extra = at + 46 + nameLength
                    // The extra fields end where the record says they do, and
                    // never past the file however sure the record sounds.
                    guard extra + extraLength <= raw.count else { throw Failure.truncated }
                    var cursor = extra
                    while cursor + 4 <= extra + extraLength {
                        let id = u16(raw, cursor)
                        let size = u16(raw, cursor + 2)
                        if id == 0x0001 {
                            var field = cursor + 4
                            // Every one of these is eight bytes read out of a
                            // length the archive chose, so each is checked
                            // against the end of this field before it is read.
                            // Nothing else in this file reads unbounded, and a
                            // zip is a download: a size too small for the
                            // values it claims would otherwise read off the end
                            // of the mapping.
                            let limit = cursor + 4 + size
                            func next() throws -> Int {
                                guard field + 8 <= limit else { throw Failure.truncated }
                                defer { field += 8 }
                                return u64(raw, field)
                            }
                            // In order, and only those written: uncompressed,
                            // compressed, then the header offset.
                            if u32(raw, at + 24) == 0xFFFF_FFFF { _ = try next() }
                            if compressed == 0xFFFF_FFFF { compressed = try next() }
                            if header == 0xFFFF_FFFF { header = try next() }
                            break
                        }
                        cursor += 4 + size
                    }
                }

                entries.append(
                    Entry(
                        name: name, method: u16(raw, at + 10), compressedSize: compressed,
                        localHeader: header, crc: UInt32(truncatingIfNeeded: u32(raw, at + 16))))
                at += 46 + nameLength + extraLength + commentLength
            }
            return entries
        }
    }

    /// The end of central directory record, which is the only thing in a zip
    /// found by looking rather than by being pointed at. It sits last, unless a
    /// comment follows it, and a comment is at most 65535 bytes.
    private static func endRecord(_ raw: UnsafeRawBufferPointer) throws -> (offset: Int, count: Int)
    {
        guard raw.count >= 22 else { throw Failure.notAZip }
        let earliest = max(0, raw.count - 22 - 65535)
        var at = raw.count - 22
        while at >= earliest {
            if u32(raw, at) == 0x0605_4B50 {
                var count = u16(raw, at + 10)
                var offset = u32(raw, at + 16)

                // Zip64 again: the 32-bit record says all ones and the real one
                // is found through a locator immediately before it.
                if count == 0xFFFF || offset == 0xFFFF_FFFF, at >= 20,
                    u32(raw, at - 20) == 0x0706_4B50
                {
                    let zip64 = u64(raw, at - 20 + 8)
                    guard zip64 + 56 <= raw.count, u32(raw, zip64) == 0x0606_4B50 else {
                        throw Failure.truncated
                    }
                    count = u64(raw, zip64 + 32)
                    offset = u64(raw, zip64 + 48)
                }
                return (offset, count)
            }
            at -= 1
        }
        throw Failure.notAZip
    }

    private static func u16(_ raw: UnsafeRawBufferPointer, _ at: Int) -> Int {
        Int(raw.loadUnaligned(fromByteOffset: at, as: UInt16.self).littleEndian)
    }

    private static func u32(_ raw: UnsafeRawBufferPointer, _ at: Int) -> Int {
        Int(raw.loadUnaligned(fromByteOffset: at, as: UInt32.self).littleEndian)
    }

    private static func u64(_ raw: UnsafeRawBufferPointer, _ at: Int) -> Int {
        Int(bitPattern: UInt(raw.loadUnaligned(fromByteOffset: at, as: UInt64.self).littleEndian))
    }
}

/// CRC-32 as the zip format defines it: IEEE 802.3, reflected, with the usual
/// pre- and post-inversion.
///
/// Written out rather than reached for, because the one in the system is inside
/// zlib and there is no module that offers it to Swift without a shim. The
/// table is built once, the first time a checksum is taken.
struct CRC32 {
    private static let table: [UInt32] = (0..<256).map { seed in
        (0..<8).reduce(UInt32(seed)) { value, _ in
            value & 1 != 0 ? 0xEDB8_8320 ^ (value >> 1) : value >> 1
        }
    }

    private var value: UInt32 = 0xFFFF_FFFF

    mutating func update(_ bytes: UnsafeRawBufferPointer) {
        var running = value
        for byte in bytes {
            running = Self.table[Int((running ^ UInt32(byte)) & 0xFF)] ^ (running >> 8)
        }
        value = running
    }

    var checksum: UInt32 { value ^ 0xFFFF_FFFF }
}
