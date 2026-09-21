// SQLite, as much of it as this program uses.
//
// SQLite comes from the SDK, so the cache costs no dependency. What it costs
// instead is C: every call here takes or returns a pointer, and none of that
// may escape this file. A query elsewhere names a table and binds a value, and
// if it ever has to reach for an `OpaquePointer` then something belongs in here
// that is not in here yet.
//
// The two types are not Sendable and are not meant to be. A connection is owned
// by the actor that opened it, which is how one thread at a time is guaranteed
// without a mutex.

import Foundation
import SQLite3

/// What SQLite said, and what we were doing when it said it.
public struct SQLiteError: Error, CustomStringConvertible {
    public let doing: String
    public let message: String
    public let code: Int32

    public var description: String { "\(doing): \(message)" }
}

/// A value bound into a statement. Five shapes, because the cache stores five
/// and a sixth would be a column nothing reads.
public enum Value: Sendable, Equatable {
    case text(String)
    case int(Int64)
    case real(Double)
    case null

    /// A schedule time, which is absent for a row the export gave none.
    static func seconds(_ n: Int64?) -> Value { n.map(Value.int) ?? .null }
}

/// One connection.
public final class Database {
    fileprivate let handle: OpaquePointer

    /// Opens the file at `path`. A reader cannot create the file: asking for a
    /// cache that is not there is a question with an answer, and an empty
    /// database created by the asking is a day with no service.
    public init(path: String, readOnly: Bool = true) throws {
        var db: OpaquePointer?
        let flags =
            readOnly
            ? SQLITE_OPEN_READONLY
            : SQLITE_OPEN_READWRITE | SQLITE_OPEN_CREATE
        let rc = sqlite3_open_v2(path, &db, flags | SQLITE_OPEN_NOMUTEX, nil)
        guard rc == SQLITE_OK, let db else {
            let message = db.map { String(cString: sqlite3_errmsg($0)) } ?? "out of memory"
            sqlite3_close_v2(db)
            throw SQLiteError(doing: "opening \(path)", message: message, code: rc)
        }
        self.handle = db
    }

    /// Opens a database that lives only as long as this object. Every test
    /// builds one of these: a test that reads the real cache is a test that
    /// passes or fails on what the city published this morning.
    public static func inMemory() throws -> Database {
        try Database(path: ":memory:", readOnly: false)
    }

    deinit { sqlite3_close_v2(handle) }

    /// Runs statements for their effect. Takes a whole script, because the
    /// schema is one.
    public func execute(_ sql: String) throws {
        var raw: UnsafeMutablePointer<CChar>?
        let rc = sqlite3_exec(handle, sql, nil, nil, &raw)
        guard rc == SQLITE_OK else {
            let message = raw.map { String(cString: $0) } ?? "error \(rc)"
            sqlite3_free(raw)
            throw SQLiteError(doing: "running a statement", message: message, code: rc)
        }
        sqlite3_free(raw)
    }

    public func prepare(_ sql: String) throws -> Statement {
        try Statement(self, sql)
    }

    /// Runs `body` inside a transaction, and rolls back if it throws. Six
    /// million inserts outside one take minutes rather than seconds, because
    /// each would otherwise be its own commit.
    public func transaction<T>(_ body: () throws -> T) throws -> T {
        try execute("BEGIN")
        do {
            let out = try body()
            try execute("COMMIT")
            return out
        } catch {
            // The rollback's own failure is dropped on purpose: the error worth
            // reporting is the one that got us here.
            try? execute("ROLLBACK")
            throw error
        }
    }

    fileprivate var lastMessage: String { String(cString: sqlite3_errmsg(handle)) }
}

/// One prepared statement, stepped for rows or run for its effect.
///
/// It is reset rather than rebuilt between rows, which is the whole reason the
/// ingest finishes: preparing a statement six million times is the cost this
/// type exists to avoid.
public final class Statement {
    private let db: Database
    private let handle: OpaquePointer
    private let sql: String

    fileprivate init(_ db: Database, _ sql: String) throws {
        var stmt: OpaquePointer?
        let rc = sqlite3_prepare_v2(db.handle, sql, -1, &stmt, nil)
        guard rc == SQLITE_OK, let stmt else {
            sqlite3_finalize(stmt)
            throw SQLiteError(doing: "preparing a query", message: db.lastMessage, code: rc)
        }
        self.db = db
        self.handle = stmt
        self.sql = sql
    }

    deinit { sqlite3_finalize(handle) }

    /// SQLite copies a bound string rather than borrowing it. Without this a
    /// Swift string's buffer can be freed before the statement runs, and what
    /// lands in the row is whatever took its place.
    private var transient: sqlite3_destructor_type {
        unsafeBitCast(Int(-1), to: sqlite3_destructor_type.self)
    }

    /// Binds every parameter, in order, having cleared the last row's.
    public func bind(_ values: [Value]) throws {
        sqlite3_reset(handle)
        sqlite3_clear_bindings(handle)
        for (i, value) in values.enumerated() {
            let at = Int32(i + 1)
            let rc =
                switch value {
                case .text(let s): sqlite3_bind_text(handle, at, s, -1, transient)
                case .int(let n): sqlite3_bind_int64(handle, at, n)
                case .real(let d): sqlite3_bind_double(handle, at, d)
                case .null: sqlite3_bind_null(handle, at)
                }
            guard rc == SQLITE_OK else {
                throw SQLiteError(doing: "binding \(sql)", message: db.lastMessage, code: rc)
            }
        }
    }

    /// Advances to the next row, and answers whether there was one.
    public func step() throws -> Bool {
        let rc = sqlite3_step(handle)
        switch rc {
        case SQLITE_ROW: return true
        case SQLITE_DONE: return false
        default: throw SQLiteError(doing: "running \(sql)", message: db.lastMessage, code: rc)
        }
    }

    /// Runs a statement that returns nothing.
    public func run(_ values: [Value] = []) throws {
        try bind(values)
        while try step() {}
    }

    /// Runs the statement and builds a row from each result.
    public func rows<T>(_ values: [Value] = [], _ read: (Statement) -> T) throws -> [T] {
        try bind(values)
        var out: [T] = []
        while try step() { out.append(read(self)) }
        return out
    }

    /// A NULL column reads as the empty string, which is what the cache stores
    /// for an absent name anyway.
    public func text(_ column: Int32) -> String {
        sqlite3_column_text(handle, column).map { String(cString: $0) } ?? ""
    }

    public func int(_ column: Int32) -> Int { Int(sqlite3_column_int64(handle, column)) }
    public func double(_ column: Int32) -> Double { sqlite3_column_double(handle, column) }
}
