// Storing and forgetting the subscription key.
//
// Every test here writes to a file it made in a temporary directory and never
// to the real one: the real one holds the key belonging to whoever is running
// this, and a test suite that clears it would be a test suite that turns live
// times off on a machine it does not own.

import Foundation
import Testing

@testable import OTransitKit

@Suite("Key store")
struct KeyStoreTests {
    /// A file in a directory of this test's own, gone when it returns.
    private func inTemporary(_ body: (URL) throws -> Void) throws {
        let directory = URL(fileURLWithPath: NSTemporaryDirectory())
            .appendingPathComponent("otransit-key-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        try body(directory.appendingPathComponent(".env"))
    }

    @Test("a key written is the key read back")
    func roundTrip() throws {
        try inTemporary { file in
            try Key.write("0123456789abcdef0123456789abcdef", to: file)
            #expect(Key.read(from: file) == "0123456789abcdef0123456789abcdef")
        }
    }

    @Test("a key is written where a person can still read it")
    func readable() throws {
        try inTemporary { file in
            try Key.write("abc123", to: file)
            let text = try String(contentsOf: file, encoding: .utf8)
            // The name either other port writes, so the file stays a .env and
            // not a private format.
            #expect(text.contains("OC_TRANSPO_SUBSCRIPTION_KEY=abc123"))
            // And where to get one, for whoever opens it next.
            #expect(text.contains("developer.azure-api.net"))
        }
    }

    @Test("only the owner can read the file it is kept in")
    func permissions() throws {
        try inTemporary { file in
            try Key.write("abc123", to: file)
            let mode =
                try FileManager.default.attributesOfItem(
                    atPath: file.path(percentEncoded: false))[.posixPermissions] as? NSNumber
            #expect(mode?.int16Value == 0o600)
        }
    }

    @Test("writing replaces rather than appends")
    func replaces() throws {
        try inTemporary { file in
            try Key.write("first", to: file)
            try Key.write("second", to: file)
            #expect(Key.read(from: file) == "second")
            // One line naming the key, not two with different values.
            let lines = try String(contentsOf: file, encoding: .utf8)
                .split(whereSeparator: \.isNewline)
                .filter { $0.contains("SUBSCRIPTION_KEY=") }
            #expect(lines.count == 1)
        }
    }

    @Test("saving nothing is removing")
    func emptyRemoves() throws {
        try inTemporary { file in
            try Key.write("abc123", to: file)
            try Key.write("   ", to: file)
            #expect(Key.read(from: file) == nil)
            #expect(!FileManager.default.fileExists(atPath: file.path(percentEncoded: false)))
        }
    }

    @Test("removing a key that was never there is not a failure")
    func removeAbsent() throws {
        try inTemporary { file in
            Key.remove(from: file)
            #expect(Key.read(from: file) == nil)
        }
    }

    @Test("a key with whitespace in it is refused before it is stored")
    func plausible() {
        #expect(Key.plausible("0123456789abcdef"))
        // The failure this guards: either of these goes into an HTTP header,
        // fails every request, and draws exactly like having no key at all.
        #expect(!Key.plausible("0123 456789abcdef"))
        #expect(!Key.plausible("0123\n456789abcdef"))
        #expect(!Key.plausible("0123\t456789abcdef"))
        // A newline on the end is not the same thing: it is what a paste
        // carries, and it is trimmed rather than refused.
        #expect(Key.plausible("0123456789abcdef\n"))
        #expect(!Key.plausible(""))
        #expect(!Key.plausible("   "))
        // The committed template, copied and never edited.
        #expect(!Key.plausible("your_key_here"))
        // A trailing newline from a paste is trimmed, not refused.
        #expect(Key.plausible("  0123456789abcdef  "))
    }

    @Test("a key that cannot be one is refused rather than stored")
    func refuses() throws {
        try inTemporary { file in
            try Key.write("good0123456789ab", to: file)
            // A newline in the middle writes a second line into the file, and
            // the parser reads back only what came before it: a silently
            // truncated key that fails every request and draws exactly like
            // having none at all.
            #expect(throws: Key.Refusal.self) {
                try Key.write("0123\n456789abcdef", to: file)
            }
            #expect(throws: Key.Refusal.self) {
                try Key.write("0123 456789abcdef", to: file)
            }
            // And the key that was already there is still there.
            #expect(Key.read(from: file) == "good0123456789ab")
        }
    }

    @Test("a pasted key is stored without the spaces it was pasted with")
    func trims() throws {
        try inTemporary { file in
            try Key.write("  abc123\n", to: file)
            #expect(Key.read(from: file) == "abc123")
        }
    }
}
