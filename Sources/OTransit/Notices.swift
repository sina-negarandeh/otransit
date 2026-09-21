// What the city has published about a route, and when to ask again.
//
// Its own model and not a part of Schedule. It shares nothing with the
// timetable: not the cache, not the key, not the state, only the epoch it is
// handed to decide whether an answer is owed. It was six members inside a
// 450-line class that already held the timetable, the clock, the key and the
// pins, and RootView was already starting it as a task of its own — which is
// the shape of a type asking to be one.

import Foundation
import OTransitKit

@MainActor
@Observable
final class Notices {
    /// What the feed said.
    ///
    /// Not an optional. A fetch that found no news and a fetch that never
    /// happened answer every question alike — no kind for a route, no notices
    /// naming one, and not `unreadable`, because nothing was held. Spelling
    /// that difference out bought two unwrapping methods and no information.
    private(set) var published = Updates.quiet

    /// When to ask the updates feed again.
    ///
    /// A floor and not a schedule. Nothing polls in the background: the feed
    /// is asked once when the popover opens and the task ends, so this only
    /// decides whether a second opening asks again. A minute is enough to stop
    /// a burst of clicking fetching 78 KB each time, and short enough that
    /// whatever is read was read while somebody was looking at it.
    ///
    /// It was half an hour, chosen for detours: one is published hours before
    /// it starts and stands for days, and the feed has one running 511 days
    /// on. That reasoning never applied to an alert. "Service is operating on
    /// the eastbound platforms only" is about the next twenty minutes, and
    /// half an hour is most of its life.
    private var listening: Cadence
    private static let every = 60

    /// What the first refusal costs. Each after it doubles, to about two hours.
    private static let backoff = 60

    init() {
        listening = .every(
            Self.every, backingOffFrom: Self.backoff,
            from: Int(Date.now.timeIntervalSince1970))
    }

    /// Holding one answer and asking nobody for another.
    ///
    /// What a screen asked for by name uses: the picture must not depend on
    /// what the network does while it is being taken.
    init(showing published: Updates) {
        self.published = published
        self.listening = .silent()
    }

    /// Asks the updates feed what it has published.
    ///
    /// Failure is silence. A board without its notice is still a board, and a
    /// row saying the notices could not be fetched would spend the space this
    /// screen has on the program talking about itself.
    ///
    /// Silence, but not a retreat. The two ways this fails are not the same
    /// thing and must not cost the same: a server that refuses is a server to
    /// stop asking, and a feed that came back and would not parse is a server
    /// answering perfectly well. This one does the second about one read in
    /// three, splicing its own gateway's 403 page into the middle of the RSS,
    /// and the next read usually succeeds. Backing off from that would put the
    /// next attempt minutes away over a fault that clears by itself, and it
    /// would do it while somebody is opening the popover to look.
    func listen(at epoch: Int) async {
        guard listening.owed(at: epoch) else { return }

        let settled = Int(Date.now.timeIntervalSince1970)
        do {
            published = try await Feed.notices()
            listening.answered(at: settled)
        } catch is Updates.Failure {
            // The feed answered. What it sent was not a feed.
            listening.answered(at: settled)
        } catch {
            listening.refused(at: settled)
        }
    }
}
