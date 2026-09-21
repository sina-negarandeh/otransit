// Where this app keeps what it has. Everything it keeps, it keeps here.
//
// One directory, named for the bundle, holding a cache this app downloaded and
// built and a pins file this app wrote. It reads nothing another program put
// anywhere, and writes nothing another program reads: uninstalling is deleting
// one folder, and no other copy of otransit on this machine can be broken by
// anything that happens in it.

import Foundation

public enum Paths {
    public static let bundleID = "com.sinanegarandeh.otransit"

    /// Application Support, and not Caches.
    ///
    /// The cache is derived data, which is what Caches is for, and macOS is
    /// free to delete what it finds there when the disk fills. Rebuilding costs
    /// a download of about 30 MB, and a menu bar app that quietly loses its
    /// schedule while you are asleep is a menu bar app showing nothing at the
    /// moment you look at it. So it lives where the system leaves it alone.
    public static var support: URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[
            0]
        return base.appendingPathComponent(bundleID, isDirectory: true)
    }

    /// The SQLite cache built from the published feed.
    public static var cache: URL { support.appendingPathComponent("gtfs.db") }

    /// Where an ingest builds, before it is swapped over the live one. Beside
    /// it and not inside a temporary directory, so the rename at the end cannot
    /// cross a filesystem and become a copy that can half fail.
    public static var building: URL { support.appendingPathComponent("gtfs.db.building") }

    /// The unpacked feed, kept only while an ingest reads it.
    public static var unpacked: URL { support.appendingPathComponent("feed", isDirectory: true) }

    /// The subscription key, in a .env file this app alone reads.
    public static var key: URL { support.appendingPathComponent(".env") }

    /// The kept boards.
    public static var pins: URL { support.appendingPathComponent("pins") }

    /// Creates the directory if this is the first run. Called before anything
    /// is written, and never before something is read.
    public static func prepare() throws {
        try FileManager.default.createDirectory(at: support, withIntermediateDirectories: true)
    }

    /// Keeps the cache out of every backup.
    ///
    /// This is the half of Caches worth having. Living in Application Support
    /// means the system never deletes the file; this means Time Machine never
    /// copies it either, and a machine does not carry 444 MB of derived data
    /// into every backup it takes when one request rebuilds it. Called on the
    /// file an ingest just finished, because the flag belongs to the inode and
    /// a swapped-in file is a new one.
    public static func excludeFromBackup(_ url: URL) throws {
        var url = url
        var values = URLResourceValues()
        values.isExcludedFromBackup = true
        try url.setResourceValues(values)
    }
}
