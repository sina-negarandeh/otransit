// What the trip updates feed says, read from bodies shaped like the real one.
//
// No socket: the fixtures below are trimmed from an actual response. The live
// endpoint is exercised by `otransit realtime`, which is the only thing here
// allowed to touch it.

import Foundation
import Testing

@testable import OTransitKit

@Suite("Realtime")
struct RealtimeTests {
    private func feed(_ json: String) throws -> Realtime {
        try Realtime(json: Data(json.utf8))
    }

    @Test("ids arrive as numbers where the cache holds text")
    func numericIDs() throws {
        // The real feed writes StopId as 10248 and TripId as 15609010. The
        // cache holds both as text. Compared as they arrive, nothing ever
        // matches and every row on the board reads sched.
        let live = try feed(
            """
            {"Header":{"Timestamp":1789362186},"Entity":[{"TripUpdate":{
              "Trip":{"TripId":15609010,"ScheduleRelationship":0},
              "StopTimeUpdate":[{"StopId":10248,"Arrival":{"HasTime":1,"Time":1789362900}}]}}]}
            """)
        #expect(live.arrival(trip: "15609010", stop: "10248") == 1_789_362_900)
        #expect(live.count == 1)
    }

    @Test("a flag written as 1 is still true")
    func numericFlag() throws {
        // HasTime is 1, not true. Read as a Bool it is nil, and the prediction
        // is silently dropped.
        let live = try feed(
            """
            {"Entity":[{"TripUpdate":{"Trip":{"TripId":"t"},
              "StopTimeUpdate":[{"StopId":"s","Arrival":{"HasTime":1,"Time":100}}]}}]}
            """)
        #expect(live.arrival(trip: "t", stop: "s") == 100)
    }

    @Test("no time is not a time")
    func missingTime() throws {
        // Taking the zero would put the trip at the epoch, which draws as a
        // departure that went fifty years ago.
        let live = try feed(
            """
            {"Entity":[{"TripUpdate":{"Trip":{"TripId":"t"},
              "StopTimeUpdate":[{"StopId":"s","Arrival":{"HasTime":0,"Time":0}}]}}]}
            """)
        #expect(live.arrival(trip: "t", stop: "s") == nil)
        #expect(live.count == 0)
    }

    @Test("relationship 3 takes a trip off the board")
    func cancelled() throws {
        let live = try feed(
            """
            {"Entity":[{"TripUpdate":{"Trip":{"TripId":"gone","ScheduleRelationship":3},
              "StopTimeUpdate":[]}},
              {"TripUpdate":{"Trip":{"TripId":"running","ScheduleRelationship":0},
              "StopTimeUpdate":[]}}]}
            """)
        #expect(live.isCancelled("gone"))
        #expect(!live.isCancelled("running"))
    }

    @Test("an update naming no trip is about nothing")
    func anonymous() throws {
        // Kept out rather than stored under the empty id, which a departure
        // with no trip would then match.
        let live = try feed(
            """
            {"Entity":[{"TripUpdate":{"Trip":{},
              "StopTimeUpdate":[{"StopId":"s","Arrival":{"HasTime":1,"Time":100}}]}}]}
            """)
        #expect(live.count == 0)
    }

    @Test("a body that is not a feed is refused, not read as empty")
    func refusesGarbage() {
        // A broken parse looks exactly like a quiet Sunday. Throwing is what
        // tells the two apart.
        #expect(throws: (any Error).self) { try feed("not json") }
        #expect(throws: (any Error).self) { try feed("[1,2,3]") }
    }

    @Test("age is counted in seconds for two minutes, then in minutes")
    func age() throws {
        let live = try feed("{\"Header\":{\"Timestamp\":1000},\"Entity\":[]}")
        #expect(live.note(now: 1000) == "live 0s")
        #expect(live.note(now: 1120) == "live 120s")
        #expect(live.note(now: 1121) == "live 2m old")
        // The endpoint's clock and ours are two clocks. A feed stamped ahead of
        // us is not old.
        #expect(live.note(now: 900) == "live 0s")
    }

    @Test("a feed that did not stamp itself is taken as current")
    func unstamped() throws {
        // The alternative is reporting it as 1970.
        let live = try feed("{\"Entity\":[]}")
        #expect(live.note(now: 1_789_362_186) == "live 0s")
    }
}

extension RealtimeTests {
    @Test("a stop with only a departure still has a prediction")
    func departureOnly() throws {
        // The first stop of every trip carries a departure and no arrival — a
        // vehicle does not arrive at the stop it starts from. 13 of 443 stop
        // updates in one real response were shaped this way, all StopSequence 1,
        // and reading only Arrival made a terminus read "sched" while the feed
        // was telling us the time.
        let live = try Realtime(
            json: Data(
                """
                {"Entity":[{"TripUpdate":{"Trip":{"TripId":"t"},
                  "StopTimeUpdate":[{"StopId":"9869","StopSequence":1,"Arrival":null,
                  "Departure":{"HasTime":1,"Time":1789366783}}]}}]}
                """.utf8))
        #expect(live.arrival(trip: "t", stop: "9869") == 1_789_366_783)
    }

    @Test("arrival wins when a stop has both")
    func arrivalFirst() throws {
        let live = try Realtime(
            json: Data(
                """
                {"Entity":[{"TripUpdate":{"Trip":{"TripId":"t"},
                  "StopTimeUpdate":[{"StopId":"s","Arrival":{"HasTime":1,"Time":100},
                  "Departure":{"HasTime":1,"Time":200}}]}}]}
                """.utf8))
        #expect(live.arrival(trip: "t", stop: "s") == 100)
    }

    @Test("a stop update with neither half is not a prediction")
    func neitherHalf() throws {
        let live = try Realtime(
            json: Data(
                """
                {"Entity":[{"TripUpdate":{"Trip":{"TripId":"t"},
                  "StopTimeUpdate":[{"StopId":"s","Arrival":null,"Departure":null}]}}]}
                """.utf8))
        #expect(live.count == 0)
    }
}
