// What the app has: a schedule, or a reason it does not have one yet.
//
// One observable object, held by the scene and read by every view. It is on the
// main actor because it is what SwiftUI reads; the work it starts is not, and
// the cache it opens is an actor of its own.

import Foundation
import OTransitKit
import SwiftUI

@MainActor
@Observable
final class Schedule {
    /// What the app can be. A first run is `missing` and nothing else, which is
    /// also what a person who deleted the file gets: there is one path in, so
    /// there is one path to get it wrong on.
    enum State {
        case missing(Absent)
        case building(Building)
        case ready(Cache)
        case failed(String)
    }

    /// Why there is no schedule to read.
    enum Absent {
        /// A first run, or a file someone deleted.
        case never
        /// A cache built by an older version, whose columns this one does not
        /// read. It is thrown away rather than migrated: the source is a
        /// download the city publishes and rebuilding takes about ten seconds.
        case outdated
    }

    private(set) var state: State

    /// The instant every screen is drawn for. Held rather than read per view,
    /// so a board and the row above it cannot disagree about what time it is.
    private(set) var clock = Clock(at: .now)

    /// The subscription key, or nil where there is none.
    ///
    /// Owned here because three views wanted the answer and each went to the
    /// disk for it, caching what it found: the file was doing the job of a
    /// shared variable, and nothing told the others when it changed. Settings
    /// writes through `use` and `forget`; everything else reads this.
    private(set) var key: String?

    /// The last thing the realtime feed said. Kept while the endpoint is down:
    /// a stale prediction carrying an honest age beats no prediction.
    private(set) var live: Realtime?

    /// What the poller is doing. Model, not a timer — see Poller.
    private var poller = Poller.silent()

    /// Whether the cache is the export the city is publishing.
    ///
    /// Held and shown rather than worked out when something goes wrong. A stale
    /// cache does not look broken: the board fills, every row reads `scheduled`,
    /// and nothing says why. See Freshness.
    private(set) var freshness: Freshness = .unknown

    /// What the updates feed says today, or nil when nothing has answered.
    ///
    /// Route-level only. The feed names stops as well, but it names them in
    /// prose together with the alternates it tells you to use, and there is no
    /// safe way to separate "your stop is closed" from "your stop is on the
    /// detour". So a board says the route has one and shows what was published.
    private(set) var detours: Detours?

    /// When to ask the updates feed again. A detour is published hours before
    /// it starts and stands for days, so this is slow.
    private var listening = Cadence.silent()
    private static let listenEvery = 30 * 60

    /// When to ask that question again. The same model the realtime poller
    /// uses, on the same clock — this was four fields and two constants here,
    /// in Dates while the poller counted seconds since 1970.
    private var asking = Cadence.silent()

    /// How often the clock is moved on. Fast enough that a wait drawn in whole
    /// minutes is never wrong for long, slow enough to be nothing.
    private static let tickEvery = 15

    /// How long an answer about the published feed is trusted for. The city
    /// republishes every few days, so this only has to be short enough to catch
    /// it within one sitting.
    private static let askEvery = 15 * 60

    /// What the first failed attempt costs, doubling after that. A network that
    /// was down a minute ago is worth asking again; one that is down now is not
    /// worth asking every second.
    private static let retryEvery = 60

    /// How well the feed is being heard, as of the clock the screens are drawn
    /// for. Read off the poller rather than stored, so it cannot disagree with
    /// the times beside it.
    var hearing: Hearing { poller.hearing(at: clock.epoch) }

    init() {
        state = Self.opened()
        key = Key.read()
        asking = .every(Self.askEvery, backingOffFrom: Self.retryEvery, from: clock.epoch)
        listening = .every(Self.listenEvery, backingOffFrom: Self.retryEvery, from: clock.epoch)
    }

    /// A schedule stopped in one state, for looking at.
    ///
    /// Design work needs to see a screen more than once, and most of these
    /// states are reached by waiting: the download is over in half a minute and
    /// the failure needs the network to be down. This asks for one directly.
    ///
    /// The cadence is left silent, which is what holds the state still: a screen
    /// asked to show a stale schedule would otherwise check the real server a
    /// moment after it appeared and redraw itself as whatever today happens to
    /// be, which is the one thing a state asked for by name must not do.
    init(
        showing state: State, freshness: Freshness = .unknown, key: String? = nil,
        detours: Detours? = nil
    ) {
        self.state = state
        self.freshness = freshness
        self.key = key
        self.detours = detours
    }

    /// Opens the cache if there is one, and says what that left the app as.
    ///
    /// A missing file is not an error: it is what a first run looks like, and
    /// the answer is a download and not a complaint. Returned rather than
    /// assigned, so `init` can reach a real state directly instead of starting
    /// at an `opening` nobody ever saw and correcting it a line later.
    private static func opened() -> State {
        guard FileManager.default.fileExists(atPath: Paths.cache.path(percentEncoded: false)) else {
            return .missing(.never)
        }
        do {
            return .ready(try Cache(path: Paths.cache))
        } catch CacheError.outdated {
            // Not a failure to report: a file this version cannot read is the
            // same situation as no file, and the answer is the same download.
            return .missing(.outdated)
        } catch {
            return .failed(String(describing: error))
        }
    }

    /// Re-opens the cache, after a build has replaced it.
    func open() { state = Self.opened() }

    /// Stores a key and starts asking with it.
    func use(_ candidate: String) throws {
        try Key.write(candidate)
        key = Key.read()
    }

    /// Forgets the key. Every time on screen becomes a scheduled one.
    func forget() {
        Key.remove()
        key = nil
    }

    /// Downloads the published feed and builds the cache from it, reporting
    /// each phase as it goes.
    ///
    /// The work is not on the main actor and the reporting is: `Feed.build` is
    /// handed a callback that hops back here, so a view reads one property and
    /// never a value being written from another thread.
    func build() {
        state = .building(.checking)
        Task {
            do {
                try await Feed.build(into: Paths.cache) { phase in
                    Task { @MainActor in
                        // A late phase from a run that already failed must not
                        // put the spinner back on screen.
                        if case .building = self.state { self.state = .building(phase) }
                    }
                }
                open()
                // Asked rather than assumed. The build may have ended in a 304
                // and changed nothing, and `built` belongs to the cache on disk
                // rather than to the moment this finished.
                await check(force: true)
            } catch {
                state = .failed(String(describing: error))
            }
        }
    }

    /// Keeps every screen on the current minute.
    ///
    /// Separate from the poller below, because the two are on different clocks
    /// and were tangled: the waits on a board count down whether or not there
    /// is a key, and tying them to the poller meant a program with no key never
    /// counted down at all, while one that had just lost its key counted down
    /// every two seconds. This reads nothing and asks nothing — it moves the
    /// clock, and only as often as a board drawn in whole minutes can show.
    func keepTime() async {
        while !Task.isCancelled {
            tick()
            try? await Task.sleep(for: .seconds(Self.tickEvery))
        }
    }

    /// Polls the realtime feed for as long as the popover is open.
    ///
    /// Driven by a `.task`, so it starts when the popover appears and is
    /// cancelled when it goes away: a menu bar app is closed almost all the
    /// time, and asking every twenty-five seconds around the clock would be
    /// asking on behalf of nobody.
    ///
    /// The key is read once, here. It used to be read every time round the
    /// loop, which is a synchronous file read on the main actor thirty times a
    /// minute; the view restarts this task instead when settings says the key
    /// has changed. See `keyChanged`.
    func watch() async {
        guard let key else {
            // No key is a working program: scheduled times only.
            poller = .silent()
            live = nil
            return
        }
        poller = .polling(from: Int(Date.now.timeIntervalSince1970))

        while !Task.isCancelled {
            let now = Int(Date.now.timeIntervalSince1970)
            if poller.owed(at: now) {
                do {
                    let feed = try await Feed.trips(key: key)
                    poller.heard(feed, at: Int(Date.now.timeIntervalSince1970))
                    live = feed
                } catch {
                    // The last good feed stays on screen.
                    poller.refused(
                        String(describing: error), at: Int(Date.now.timeIntervalSince1970))
                }
            }
            // Capped at the cadence so a backoff that reaches two hours does
            // not leave this task unable to notice it has been cancelled.
            let settled = Int(Date.now.timeIntervalSince1970)
            try? await Task.sleep(
                for: .seconds(min(Poller.cadence, max(1, poller.dueIn(at: settled)))))
        }
    }

    /// Moves the clock to now. Called when the popover opens, because a menu bar
    /// app is looked at once an hour and the board must not be drawn for the
    /// time it was last looked at.
    func tick() {
        clock = Clock(at: .now)
    }

    /// Whether the city has published anything about this route.
    ///
    /// Asked here and not through two optionals at each call site. A view has
    /// no business knowing that "no schedule" and "nothing fetched yet" are
    /// different shapes of nothing: both mean no detour.
    func detoured(_ route: String) -> Bool { detours?.names(route) ?? false }

    /// What it published about this route, which is nothing where it published
    /// nothing.
    func notices(for route: String) -> [Notice] { detours?.naming(route) ?? [] }

    /// Asks the updates feed what it has published.
    ///
    /// Failure is silence. A board without its detour is still a board, and a
    /// row saying the notices could not be fetched would spend the space this
    /// screen has on the program talking about itself.
    func updates() async {
        guard listening.owed(at: clock.epoch) else { return }

        let found = try? await Feed.detours()
        if let found { detours = found }
        listening.settled(answered: found != nil, at: Int(Date.now.timeIntervalSince1970))
    }

    /// Asks whether the cache is still the published export.
    ///
    /// One HEAD, and only when there is a cache to be stale: a first run is
    /// already being told to download, and asking there would put a second
    /// answer to the same question on the same screen.
    func check(force: Bool = false) async {
        guard case .ready(let cache) = state else { return }
        let now = clock.epoch
        guard force || asking.owed(at: now) else { return }

        // Only announce the asking when there is nothing to replace. A row that
        // already says "up to date" flickering to "checking" every quarter of an
        // hour is movement reporting no news.
        if case .unknown = freshness { freshness = .checking }

        let held = try? await cache.meta("etag")
        let built = ISO8601DateFormatter().date(from: (try? await cache.meta("built")) ?? "")
        let published = try? await Feed.published()
        // Timed from the answer and not from the asking, so a check that failed
        // does not buy the same quarter of an hour of silence a good one does.
        asking.settled(answered: published != nil, at: Int(Date.now.timeIntervalSince1970))
        freshness = .compare(cached: held, against: published, built: built)
    }
}
