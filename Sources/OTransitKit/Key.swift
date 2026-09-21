// The subscription key: where it is read from, and what counts as one.
//
// A missing key is not an error. It means scheduled times only, which is a
// working program, so nothing here reports one.
//
// One place only, and it is this app's own: ~/Library/Application Support/
// <bundle id>/.env. The Rust and Go ports read ~/.config/otransit/.env and
// share a key between them; this does not, because a program that reads another
// program's files is not standalone, and standalone was the requirement.

import Foundation

public enum Key {
    /// Why a key was not stored.
    public enum Refusal: Error, CustomStringConvertible {
        case implausible

        public var description: String {
            switch self {
            case .implausible: "a key cannot contain a space or a line break"
            }
        }
    }

    /// The names a key has been written under. Several, because a file copied
    /// from either of the other ports has to work here without being edited.
    private static let names = [
        "OC_TRANSPO_SUBSCRIPTION_KEY",
        "OCT_SUBSCRIPTION_KEY",
        "OCTRANSPO_SUBSCRIPTION_KEY",
        "SUBSCRIPTION_KEY",
    ]

    /// What the committed template holds. Someone who copied the template and
    /// never edited it has no key, and "scheduled times only" is a better
    /// answer than asking the endpoint about it.
    private static let placeholder = "your_key_here"

    /// The key, or nil when there is none.
    ///
    /// The file is a parameter with the real one as its default, so a test can
    /// exercise this against a file it made. Without that the only way to test
    /// writing would be to write over the key the person using this app put
    /// there, which is not a trade any test is worth.
    public static func read(from url: URL = Paths.key) -> String? {
        guard let text = try? String(contentsOf: url, encoding: .utf8) else { return nil }
        return find(in: text)
    }

    /// The key in a .env file's text.
    ///
    /// Parsed rather than sourced: this is a file a person edits by hand, so it
    /// carries comments, blank lines, quotes and trailing spaces, and none of
    /// those are part of a key.
    ///
    /// Newlines are trimmed as well as spaces, and the split is on any line
    /// ending rather than on `\n`. A file saved with CRLF otherwise leaves a
    /// carriage return on the end of the value, which goes into an HTTP header
    /// and fails every request — and a failing key and an absent key look
    /// exactly alike from the board, which reads `scheduled` either way.
    static func find(in text: String) -> String? {
        for line in text.split(whereSeparator: \.isNewline) {
            let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.hasPrefix("#"), let split = trimmed.firstIndex(of: "=") else { continue }

            let name = String(trimmed[trimmed.startIndex..<split])
                .trimmingCharacters(in: .whitespacesAndNewlines)
                .replacingOccurrences(of: "export ", with: "")
            guard names.contains(name) else { continue }

            let value = clean(String(trimmed[trimmed.index(after: split)...]))
            if !value.isEmpty { return value }
        }
        return nil
    }

    /// Whether a key is stored. Not the key itself: a screen that only needs to
    /// say which of two states it is in has no business holding one.
    public static var isSet: Bool { read() != nil }

    /// Whether this could be a key at all.
    ///
    /// The one check that matters is whitespace. A key with a space or a
    /// newline in it goes into an HTTP header, fails every request, and looks
    /// from the board exactly like having no key — every row reads `scheduled`
    /// either way. This app has already had one silent failure of that shape;
    /// it is not worth a second from a stray newline in a paste.
    ///
    /// Nothing here checks the length or the alphabet. The portal's keys are 32
    /// hex characters today, and a program that refuses to store anything else
    /// breaks the day that changes. The endpoint is the authority on whether a
    /// key works.
    public static func plausible(_ candidate: String) -> Bool {
        let trimmed = candidate.trimmingCharacters(in: .whitespacesAndNewlines)
        return !trimmed.isEmpty && !trimmed.contains(where: \.isWhitespace)
            && trimmed != placeholder
    }

    /// Stores the key, replacing whatever was there.
    ///
    /// The file is rewritten whole rather than edited: this app owns it, it
    /// holds one value, and a rewrite cannot leave two lines naming the same
    /// key with different values. An empty key is a removal, so clearing the
    /// field and saving does what it looks like it does.
    public static func write(_ candidate: String, to url: URL = Paths.key) throws {
        let trimmed = candidate.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return remove(from: url) }
        // Checked here and not only at the field that collects it. A key with a
        // newline in the middle writes a second line into this file, and the
        // parser reads back everything before it — a silently truncated key
        // that fails every request and draws exactly like having none.
        guard plausible(trimmed) else { throw Refusal.implausible }

        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let file =
            "# otransit — the OC Transpo subscription key.\n"
            + "#\n"
            + "# Free, from https://nextrip-public-api.developer.azure-api.net\n"
            + "# Without one, every time on screen is a scheduled one.\n"
            + "\(names[0])=\(trimmed)\n"
        try file.write(to: url, atomically: true, encoding: .utf8)
        // After the write and not before: writing atomically replaces the file,
        // and the replacement does not inherit what was set on the old one.
        try? FileManager.default.setAttributes(
            [.posixPermissions: 0o600], ofItemAtPath: url.path(percentEncoded: false))
    }

    /// Forgets the key. A file that is not there is the state this produces, so
    /// removing one that never existed is not a failure.
    public static func remove(from url: URL = Paths.key) {
        try? FileManager.default.removeItem(at: url)
    }

    private static func clean(_ raw: String) -> String {
        var value = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        // Quotes are the shell's, not the key's.
        if value.count >= 2, let first = value.first, first == "\"" || first == "'",
            value.last == first
        {
            value = String(value.dropFirst().dropLast())
        }
        value = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return value == placeholder ? "" : value
    }
}
