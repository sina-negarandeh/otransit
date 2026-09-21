// Getting the published schedule and turning it into a cache.
//
// One call does the whole thing: download, build beside the live file, and move
// the finished one over it. The move is last and it is atomic, so a download
// that fails halfway cannot leave a person without a schedule — they still have
// the one they had this morning.

import Foundation

public enum Feed {
    /// The realtime endpoint. `beta` is in the path, so its shape will change.
    static let tripsURL = URL(
        string:
            "https://nextrip-public-api.azure-api.net/octranspo/gtfs-rt-tp/beta/v1/TripUpdates?format=json"
    )!

    /// How long the realtime fetch is given. It is slow under load, and its
    /// answer is the one a person is actually looking at.
    private static let tripsTimeout: TimeInterval = 45

    /// One fetch of the trip updates.
    ///
    /// A missing key is not an error here — the caller decides not to ask.
    public static func trips(key: String) async throws -> Realtime {
        var request = URLRequest(url: tripsURL, timeoutInterval: tripsTimeout)
        request.setValue(key, forHTTPHeaderField: "Ocp-Apim-Subscription-Key")
        let (data, response) = try await URLSession.shared.data(for: request)
        if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
            throw Failure.refused(http.statusCode)
        }
        return try Realtime(json: data)
    }

    /// Where OC Transpo publishes the export. No key and no terms to accept.
    ///
    /// About 30 MB today. The number is not a constant — the export has been
    /// three times this — so nothing is sized against it and the progress bar
    /// reads the length off the response.
    public static let export = URL(
        string: "https://oct-gtfs-emasagcnfmcgeham.z01.azurefd.net/public-access/GTFSExport.zip")!

    public enum Failure: Error, CustomStringConvertible {
        case refused(Int)

        public var description: String {
            switch self {
            case .refused(let status): "the server answered \(status)"
            }
        }
    }

    /// Where OC Transpo publishes its notices. No key.
    ///
    /// Not GTFS-Realtime. That specification has a ServiceAlerts feed and this
    /// API does not serve one: only TripUpdates exists. The detours are RSS
    /// from a content system instead.
    public static let updates = URL(string: "https://www.octranspo.com/en/feeds/updates-en/")!

    /// What the updates feed says today.
    ///
    /// Given a short timeout and nothing waits on it. A board without its
    /// detour is still a board; a board that took ten seconds to appear is not.
    public static func detours() async throws -> Detours {
        var request = URLRequest(url: updates, timeoutInterval: 10)
        request.cachePolicy = .reloadIgnoringLocalCacheData
        let (data, response) = try await URLSession.shared.data(for: request)
        if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
            throw Failure.refused(http.statusCode)
        }
        return try Detours.read(data)
    }

    /// How long the freshness question is given. It is one round trip with no
    /// body, and nothing waits on the answer: a slow one is worth abandoning.
    private static let checkTimeout: TimeInterval = 10

    /// What the server is publishing, learned without downloading it.
    ///
    /// A HEAD, so the answer is headers and nothing else — about 30 MB is not
    /// spent to find out whether 30 MB is worth spending. The entity tag is
    /// compared as an opaque string and never parsed: what it means is the
    /// server's business, and all this needs is whether it is the same one.
    public static func published() async throws -> Publication {
        var request = URLRequest(url: export, timeoutInterval: checkTimeout)
        request.httpMethod = "HEAD"
        // The cache must not answer this: a stored 200 would report whatever
        // was published the last time it was asked, which is the very thing
        // being checked.
        request.cachePolicy = .reloadIgnoringLocalCacheData

        let (_, response) = try await URLSession.shared.data(for: request)
        guard let http = response as? HTTPURLResponse else { throw Failure.refused(0) }
        guard (200..<300).contains(http.statusCode) else {
            throw Failure.refused(http.statusCode)
        }
        return Publication(
            etag: http.value(forHTTPHeaderField: "ETag") ?? "",
            modified: httpDate(http.value(forHTTPHeaderField: "Last-Modified")))
    }

    /// An HTTP date, which is one fixed format in one fixed language. Parsed
    /// against a fixed locale so a machine set to another one reads it too.
    static func httpDate(_ text: String?) -> Date? {
        guard let text else { return nil }
        let form = DateFormatter()
        form.locale = Locale(identifier: "en_US_POSIX")
        form.timeZone = TimeZone(identifier: "GMT")
        form.dateFormat = "EEE, dd MMM yyyy HH:mm:ss zzz"
        return form.date(from: text)
    }

    /// Downloads the feed and builds `destination` from it.
    public static func build(
        into destination: URL, progress: @escaping @Sendable (Building) -> Void
    ) async throws {
        try Paths.prepare()
        progress(.checking)

        // What the cache on disk was built from, so the server can be asked
        // whether anything has moved on rather than told to send it all again.
        let known = stamp(of: destination)

        let archive = Paths.support.appendingPathComponent("export.zip")
        let transfer = try await download(to: archive, knowing: known, progress: progress)
        guard case .fetched(let tag) = transfer else {
            // 304: the file on disk is the file being published. Nothing to
            // unpack, nothing to build, nothing to swap.
            return
        }
        // The archive is worth nothing once it has been read, and it is 30 MB.
        defer { try? FileManager.default.removeItem(at: archive) }

        progress(.unpacking)
        let building = Paths.building
        try? FileManager.default.removeItem(at: building)

        // Off the cooperative pool: this is twenty seconds of CPU and SQLite,
        // and running it there would hold a thread the whole program shares.
        try await Task.detached(priority: .userInitiated) {
            let zip = try Zip(url: archive)
            let db = try Database(path: building.path(percentEncoded: false), readOnly: false)
            try Ingest.build(from: zip, into: db, progress: progress)
            if let tag {
                try db.prepare("INSERT OR REPLACE INTO meta VALUES (?,?)")
                    .run([.text("etag"), .text(tag)])
            }
        }.value

        // Atomic, and last. Until this line the live cache is untouched.
        _ = try FileManager.default.replaceItemAt(destination, withItemAt: building)
        try? Paths.excludeFromBackup(destination)
    }

    /// What a download came back with.
    private enum Transfer {
        /// The export, with the tag it was published under.
        case fetched(etag: String?)
        /// The server says what is on disk is what it holds.
        case unchanged
    }

    /// Downloads the export, reporting how much has arrived.
    ///
    /// `knowing` is the tag the cache on disk was built from. Sent back as
    /// `If-None-Match`, it lets the server answer 304 in one round trip with no
    /// body — which is the whole reason the tag is stored. Rebuilding an
    /// unchanged export costs a 30 MB transfer and three and a half million
    /// inserts, and this is what makes pressing Update cheap when it is.
    private static func download(
        to file: URL, knowing: String?, progress: @escaping @Sendable (Building) -> Void
    ) async throws -> Transfer {
        var request = URLRequest(url: export)
        if let knowing { request.setValue(knowing, forHTTPHeaderField: "If-None-Match") }
        // Ours is the only cache in this: URLSession's would answer from its own
        // store and never let the 304 be seen.
        request.cachePolicy = .reloadIgnoringLocalCacheData

        let watcher = Watcher { received, expected in
            progress(.downloading(received: received, total: expected > 0 ? expected : nil))
        }
        let (temporary, response) = try await URLSession.shared.download(
            for: request, delegate: watcher)

        let http = response as? HTTPURLResponse
        if http?.statusCode == 304 {
            // The body is empty and the temporary file is URLSession's to clean
            // up; nothing here may move it over an archive.
            return .unchanged
        }
        if let http, !(200..<300).contains(http.statusCode) {
            throw Failure.refused(http.statusCode)
        }
        try? FileManager.default.removeItem(at: file)
        try FileManager.default.moveItem(at: temporary, to: file)
        return .fetched(etag: http?.value(forHTTPHeaderField: "ETag"))
    }

    /// The tag a cache was built from, read without opening a `Cache`.
    ///
    /// This is the download path, so it has to answer for a file that is
    /// missing, unreadable, or built to a shape this version no longer knows —
    /// and the answer in every one of those cases is the same: nothing to
    /// compare, so ask for the whole thing.
    private static func stamp(of cache: URL) -> String? {
        let path = cache.path(percentEncoded: false)
        guard FileManager.default.fileExists(atPath: path),
            let db = try? Database(path: path),
            let found = try? db.prepare("SELECT value FROM meta WHERE key = 'etag'")
                .rows([], { $0.text(0) }).first,
            !found.isEmpty
        else { return nil }
        return found
    }

    /// Turns the delegate's callbacks into the one line the popover shows.
    ///
    /// Unchecked, and safe: it holds a sendable closure and nothing else, and
    /// URLSession calls it on one queue of its own.
    private final class Watcher: NSObject, URLSessionDownloadDelegate, @unchecked Sendable {
        private let report: @Sendable (Int64, Int64) -> Void

        init(report: @escaping @Sendable (Int64, Int64) -> Void) {
            self.report = report
        }

        func urlSession(
            _ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData: Int64,
            totalBytesWritten: Int64, totalBytesExpectedToWrite: Int64
        ) {
            report(totalBytesWritten, totalBytesExpectedToWrite)
        }

        /// Required by the protocol. The async call above is what takes the
        /// file, so there is nothing to do with it here.
        func urlSession(
            _ session: URLSession, downloadTask: URLSessionDownloadTask,
            didFinishDownloadingTo location: URL
        ) {}
    }
}
