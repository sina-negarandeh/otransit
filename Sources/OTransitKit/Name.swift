// Dropping the French half of a bilingual name.
//
// The city names some places in both languages, and the feed carries both in
// one field. This app is in English, so it shows the English half: the
// alternative is a 40-character station name in a 320-point popover, half of
// which repeats the other half.

import Foundation

extension String {
    /// This name with the French half of it removed, and any of the feed's
    /// doubled spaces closed up.
    ///
    /// Two shapes, both measured against the whole feed rather than guessed at.
    ///
    /// `PARLIAMENT ~ PARLEMENT` — a tilde separates the languages, English
    /// first. That holds everywhere it appears: 4 station names, 13 headsigns
    /// and 14 route ends, with no counterexample. The tilde means nothing else
    /// in the feed, so splitting on it cannot catch anything that is not this.
    ///
    /// `BELL H.S  WEST / OUEST` — a compass direction translated after a slash.
    /// A slash on its own means an intersection and appears in 5,428 stop
    /// names, so a name is only shortened when the English direction sits
    /// immediately before the slash and its own translation immediately after.
    /// All 40 names in the feed ending in one of those four French words match
    /// that, and nothing else does.
    ///
    /// One name is not a translation at all — `Mother Teresa ~ Mother Teresa
    /// H.S` — and loses its `H.S` here. It is still the school's name.
    public var english: String {
        var out = self
        if let tilde = out.firstIndex(of: "~") { out = String(out[..<tilde]) }
        for (english, french) in Self.compass {
            let tail = " / \(french)"
            if out.uppercased().hasSuffix((english + tail).uppercased()) {
                out = String(out.dropLast(tail.count))
                break
            }
        }
        return out.split(whereSeparator: \.isWhitespace).joined(separator: " ")
    }

    /// Checked longest-first, so `SOUTH / SUD` is never read as an `EST`.
    private static let compass = [
        ("WEST", "OUEST"), ("NORTH", "NORD"), ("SOUTH", "SUD"), ("EAST", "EST"),
    ]
}

extension String {
    /// This name in the case a person writes it.
    ///
    /// The feed shouts: `TUNNEY'S PASTURE`, `ST-LAURENT / MCARTHUR`. Headsigns
    /// and route names in the same feed are written properly, so the shouting
    /// is a property of the stops table and not of the operator, and undoing it
    /// is not second-guessing anyone.
    ///
    /// Four things a plain capitalisation gets wrong, each measured against all
    /// 3,820 names a screen can show:
    ///
    /// - `TUNNEY'S` becomes `Tunney'S`, because Foundation counts an apostrophe
    ///   as a word break. It is one in `D'ARCY` and not in `TUNNEY'S`, and the
    ///   difference is whether what comes before it is a single letter.
    /// - `TOH` becomes `Toh`. Nine words in the whole feed are initialisms with
    ///   no stops in them; they are listed. Every token of five letters or more
    ///   is an ordinary word, so the list cannot grow much.
    /// - `A.Y.`, `H.S`, `Q.C.H.` and `É.S.` are initials, and they are the same
    ///   shape as each other: every letter run between the stops is one letter
    ///   long. `AD.` and `RD.` are not that shape, and become `Ad.` and `Rd.`
    /// - `8TH LINE` becomes `8Th Line`. Letters straight after a digit are the
    ///   tail of an ordinal — but only when they spell one: `STOP 1A` and
    ///   `JEANNE D'ARC 4A` end in platform designations, which keep their
    ///   capitals.
    ///
    /// `MC` is lifted to `McArthur`. `MAC` is left alone: `MACFARLANE` wants
    /// `MacFarlane` and `MACY` does not, and nothing in the name says which.
    public var titled: String {
        var out = ""
        var token = ""
        var before: Character?

        func flush() {
            guard !token.isEmpty else { return }
            out += Self.cased(token, following: before)
            token = ""
        }

        for character in self {
            if character.isLetter || character == "'" || character == "." {
                token.append(character)
            } else {
                flush()
                out.append(character)
                before = character
            }
        }
        flush()
        return out
    }

    /// The nine words in the feed that are said as letters.
    private static let initialisms: Set<String> = [
        "CHEO", "HS", "IKEA", "NCC", "NRC", "OC", "OCDC", "RCMP", "TOH",
    ]

    /// The four endings an ordinal can have. Only these are lowered after a
    /// digit: every other letter run following one is a designation and keeps
    /// its capitals — `HUNT CLUB LOOP - STOP 1A`, `JEANNE D'ARC 4A`, the same
    /// values the platform plate draws. Lowering all of them put a `4a` in a
    /// name beside a `4A` on its plate, on one row.
    private static let ordinals: Set<String> = ["ST", "ND", "RD", "TH"]

    private static func cased(_ token: String, following before: Character?) -> String {
        if before?.isNumber == true {
            return Self.ordinals.contains(token.uppercased())
                ? token.lowercased() : token.uppercased()
        }
        // The university writes itself this way, and it is the only name in the
        // feed that starts lowercase on purpose.
        if token.uppercased() == "UOTTAWA" { return "uOttawa" }
        if Self.initialisms.contains(token.uppercased()) { return token.uppercased() }
        // One letter, or letters separated by stops: A, W., A.Y., D.R.D.C.
        let runs = token.split(separator: ".")
        if !runs.isEmpty, runs.allSatisfy({ $0.count == 1 }) { return token.uppercased() }

        let parts = token.split(separator: "'", omittingEmptySubsequences: false).map(String.init)
        return parts.enumerated()
            .map { index, part in
                guard !part.isEmpty else { return part }
                // After an apostrophe only a one-letter prefix takes a capital:
                // D'Arcy and O'Connor, but Tunney's and Mooney's.
                guard index == 0 || parts[index - 1].count == 1 else { return part.lowercased() }
                return Self.lifted(part)
            }
            .joined(separator: "'")
    }

    private static func lifted(_ part: String) -> String {
        let lower = part.lowercased()
        if lower.hasPrefix("mc"), lower.count > 2 {
            let rest = lower.dropFirst(2)
            return "Mc" + rest.prefix(1).uppercased() + rest.dropFirst()
        }
        return lower.prefix(1).uppercased() + lower.dropFirst()
    }
}
