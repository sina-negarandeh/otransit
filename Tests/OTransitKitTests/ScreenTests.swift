// The two names each screen has, and the path bar under them.

import Testing

@testable import OTransitKit

@Suite("Screen")
struct ScreenTests {
    @Test("the top name and the bottom prompt are different words")
    func twoNames() {
        // Not a repetition: one says which screen this is, the other is the
        // sentence the person is in the middle of.
        #expect(Screen.stops(.bus).prompt == "Which stop?")
    }

    @Test("rail has lines and stations where a bus has routes and stops")
    func vocabulary() {
        #expect(Screen.routes(.rail).prompt == "Which line?")
        #expect(Screen.stops(.rail).prompt == "Which station?")
    }

    @Test("the words that do not depend on the mode do not change with it")
    func shared() {
        for mode in Mode.allCases {
            #expect(Screen.direction(mode).prompt == "Which way?")
            // Not a question. You have finished asking.
            #expect(Screen.board(mode).prompt == "Departures")
        }
    }

    @Test("the first screen is named for what OC Transpo calls them")
    func networks() {
        // "O-Train network" and "bus network" are the operator's own words for
        // the two things this screen chooses between. "Transit" was mine.
        #expect(Screen.transit.prompt == "What are you taking?")
    }

    @Test("rail is offered before bus, as the operator lists them")
    func order() {
        // Three lines, then two hundred routes — the order octranspo.com puts
        // its own networks in.
        #expect(Mode.allCases == [.rail, .bus])
    }

    @Test("only the first screen has nothing behind it")
    func back() {
        #expect(!Screen.transit.hasBack)
        #expect(Screen.routes(.bus).hasBack)
        #expect(Screen.board(.rail).hasBack)
    }
}

@Suite("Trail")
struct TrailTests {
    private let seven = Route(shortName: "7", longName: "St-Laurent", colour: "0075c9")
    private let stop = Stop(id: "3060", code: "3060", name: "ST-LAURENT D", platform: "")

    private var full: Place { .board(.bus, seven, "Carleton", stop) }

    @Test("a full path reads as a sentence")
    func fullPath() {
        // What it is read aloud as. Two of these are drawn as marks — a tram or
        // a bus for the network, an arrow for "toward" — because the bar cannot
        // be made to fit and those two marks are already what the screens they
        // came from put beside the same words.
        #expect(
            Trail.crumbs(of: full).map(\.spoken)
                == ["Bus", "7", "Toward Carleton", "ST-LAURENT D"])
        // And what is actually set in type beside them.
        #expect(Trail.crumbs(of: full).map(\.label) == ["", "7", "Carleton", "ST-LAURENT D"])
    }

    @Test("a mark stands in for a word only where a screen already taught it")
    func marks() {
        let crumbs = Trail.crumbs(of: full)
        #expect(crumbs.map(\.symbol) == ["bus.fill", nil, "arrowshape.right.fill", nil])
        #expect(Trail.crumbs(of: .routes(.rail)).map(\.symbol) == ["tram.fill"])
    }

    @Test("the route's colour reaches the crumb for its direction")
    func colour() {
        // The arrow on a direction crumb is the same arrow the direction screen
        // leads its rows with, and that one is tinted. One mark meaning one
        // thing must not be two colours.
        let crumbs = Trail.crumbs(of: .stops(.bus, seven, "Carleton"))
        #expect(crumbs.map(\.tint) == [nil, nil, "0075c9"])
        // The same colour the badge before it is filled with.
        #expect(crumbs[1].plate?.colour == "0075c9")
    }

    @Test("only the route crumb is a badge")
    func onlyTheRouteIsABadge() {
        // Two crumbs carry the route's colour and only one is drawn as a badge.
        // Which one is a thing the crumb says rather than a thing the screen
        // drawing it works out: a colour with a shape used to mean a pill and a
        // colour without one a tint, which is a rule every drawing site had to
        // know and none of them stated.
        let crumbs = Trail.crumbs(of: .stops(.bus, seven, "Carleton"))
        #expect(crumbs.map { $0.plate != nil } == [false, true, false])
        #expect(crumbs.map(\.isStop) == [false, false, false])
    }

    @Test("a question not yet answered adds no crumb")
    func partial() {
        #expect(Trail.crumbs(of: .transit).isEmpty)
        #expect(Trail.crumbs(of: .routes(.rail)).map(\.spoken) == ["O-Train"])
        #expect(Trail.crumbs(of: .direction(.bus, seven)).map(\.spoken) == ["Bus", "7"])
    }

    // The suite used to carry a test for a stop chosen before a route, which
    // four independent optionals could express and this cannot: there is no
    // Place with a hole in the middle to write down. The guard chain that test
    // covered went with it.

    @Test("crumbs are told apart by position, not by name")
    func identity() {
        #expect(Trail.crumbs(of: full).map(\.id) == [0, 1, 2, 3])
    }

    @Test("a crumb for every answer, however far down")
    func depthMatchesCrumbs() {
        // back() is spelled `keep(crumbs.count - 1)`, so a place whose depth
        // disagreed with the number of crumbs drawn would step back by the
        // wrong amount. They are one count now, but this says so.
        for place in [
            Place.transit, .routes(.bus), .direction(.bus, seven),
            .stops(.bus, seven, "Carleton"), full,
        ] {
            #expect(Trail.crumbs(of: place).count == place.depth)
        }
    }
}

@Suite("Place")
struct PlaceTests {
    private let seven = Route(shortName: "7", longName: "St-Laurent", colour: "0075c9")
    private let stop = Stop(id: "3060", code: "3060", name: "ST-LAURENT D", platform: "")

    @Test("stepping back drops one answer and keeps the rest")
    func back() {
        let board = Place.board(.bus, seven, "Carleton", stop)
        #expect(board.back == .stops(.bus, seven, "Carleton"))
        #expect(board.back?.back == .direction(.bus, seven))
        #expect(board.back?.back?.back == .routes(.bus))
        #expect(board.back?.back?.back?.back == .transit)
        // The first screen has nothing behind it.
        #expect(Place.transit.back == nil)
    }

    @Test("keeping a count drops everything past it")
    func keeping() {
        let board = Place.board(.bus, seven, "Carleton", stop)
        #expect(board.keeping(4) == board)
        #expect(board.keeping(2) == .direction(.bus, seven))
        #expect(board.keeping(0) == .transit)
        // Asking to keep more than there is keeps what there is.
        #expect(Place.routes(.bus).keeping(3) == .routes(.bus))
    }

    @Test("every place names the screen that draws it")
    func screens() {
        #expect(Place.transit.screen == .transit)
        #expect(Place.routes(.rail).screen == .routes(.rail))
        #expect(Place.board(.bus, seven, "Carleton", stop).screen == .board(.bus))
    }
}

@Suite("Route names")
struct RouteNameTests {
    private func route(_ long: String) -> Route {
        Route(shortName: "1", longName: long, colour: "d62839")
    }

    @Test("a route between two places splits at the feed's arrow")
    func twoEnds() {
        let ends = route("Blair <> Tunney's Pasture").ends
        #expect(ends?.from == "Blair")
        #expect(ends?.to == "Tunney's Pasture")
    }

    @Test("the space around the arrow is not part of either end")
    func trimmed() {
        // Every name in the feed today is written " <> ", and a view that kept
        // the spaces would draw them either side of the symbol as well.
        #expect(route("Waller <> Bayshore").ends?.from == "Waller")
    }

    @Test("a name with no arrow has no ends")
    func loop() {
        // The loops and shuttles: named for the one place they serve.
        #expect(route("Hurdman").ends == nil)
        #expect(route("Uplands / Greenboro").ends == nil)
        #expect(route("Parliament ~ Parlement").ends == nil)
    }

    @Test("a slash or a tilde inside one end is not a second arrow")
    func punctuation() {
        // 105 is "Airport ~ Aéroport <> Hurdman / St-Laurent & N Rideau": one
        // arrow, and two ends that each carry their own punctuation. The tilde
        // is a language break and goes; the slash is an intersection and stays.
        let ends = route("Airport ~ Aéroport <> Hurdman / St-Laurent & N Rideau").ends
        #expect(ends?.from == "Airport")
        #expect(ends?.to == "Hurdman / St-Laurent & N Rideau")
    }
}

@Suite("Service")
struct ServiceTests {
    private func route(_ name: String, _ colour: String) -> Route {
        Route(shortName: name, longName: "A <> B", colour: colour)
    }

    @Test("the three coloured kinds come from the colour")
    func byColour() {
        #expect(route("7", "0057B8").service(in: .bus) == .frequent)
        #expect(route("234", "B66B94").service(in: .bus) == .connexion)
        #expect(route("110", "6D6E70").service(in: .bus) == .local)
    }

    @Test("white is not the absence of a colour")
    func white() {
        // "A symbol with a white background means the route only runs during
        // certain times of the day, or on certain days of the week." 84 routes
        // carry FFFFFF, and it was read as "uncoloured" for far too long.
        #expect(route("13", "FFFFFF").service(in: .bus) == .other)
        #expect(route("117", "FFFFFF").service(in: .bus) == .other)
    }

    @Test("the numbering rules the operator states")
    func numbers() {
        #expect(route("602", "FFFFFF").service(in: .bus) == .school)
        #expect(route("699", "FFFFFF").service(in: .bus) == .school)
        #expect(route("404", "FFFFFF").service(in: .bus) == .event)
        #expect(route("456", "FFFFFF").service(in: .bus) == .event)
        // Shopper is 301 to 305 exactly — one round trip a week each.
        #expect(route("301", "FFFFFF").service(in: .bus) == .shopper)
        #expect(route("305", "FFFFFF").service(in: .bus) == .shopper)
        #expect(route("306", "FFFFFF").service(in: .bus) == .other)
    }

    @Test("a letter outranks the colour")
    func letters() {
        // A night route is an extension of a Frequent one and carries its
        // colour. Asked by colour it would come back Frequent, and the
        // operator gives it a symbol of its own.
        #expect(route("N39", "0057B8").service(in: .bus) == .night)
        #expect(route("R1", "D30F1D").service(in: .bus) == .replacement)
        // E1 is the Shuttle Express, which the operator documents on the same
        // page as R1. It carries Line 1's exact colour and is a bus.
        #expect(route("E1", "D30F1D").service(in: .bus) == .replacement)
        #expect(route("E1", "D30F1D").service(in: .rail) == .line)
    }

    @Test("rail is a line whatever it is coloured")
    func rail() {
        #expect(route("1", "D30F1D").service(in: .rail) == .line)
        #expect(route("2", "508128").service(in: .rail) == .line)
    }
}

@Suite("Grouping")
struct GroupingTests {
    private func route(_ name: String, _ colour: String) -> Route {
        Route(shortName: name, longName: "A <> B", colour: colour)
    }

    @Test("sections come in the operator's order, with the leftovers last")
    func order() {
        let groups = Service.group(
            [
                route("602", "FFFFFF"), route("13", "FFFFFF"), route("7", "0057B8"),
                route("110", "6D6E70"), route("234", "B66B94"), route("301", "FFFFFF"),
            ], in: .bus)
        // The three that run all day and carry their own colour, then the ones
        // the operator names by a number, and Other after all of them. Listed
        // fourth it read as a kind of service; it is what is left when every
        // kind has taken its own.
        #expect(
            groups.map(\.service) == [.frequent, .connexion, .local, .school, .shopper, .other])
        // Event, Night and Replacement are defined and dormant: one runs for
        // events, one overnight, one when a line is shut. No heading is drawn
        // for a kind nothing is.
        #expect(!groups.map(\.service).contains(.event))
    }

    @Test("rail is one section with no heading to draw")
    func railGroups() {
        let groups = Service.group([route("1", "D30F1D"), route("2", "508128")], in: .rail)
        #expect(groups.count == 1)
        #expect(groups.first?.service == .line)
    }

    @Test("only school starts folded")
    func folded() {
        #expect(Service.school.startsClosed)
        for other in Service.allCases where other != .school {
            #expect(!other.startsClosed)
        }
    }
}

@Suite("Crumbs")
struct CrumbPlatformTests {
    private let seven = Route(shortName: "7", longName: "Carleton <> St-Laurent", colour: "0057b8")

    @Test("the stop crumb carries its platform, and only it does")
    func platform() {
        let crumbs = Trail.crumbs(
            of: .board(
                .bus, seven, "Carleton",
                Stop(id: "3025", code: "3025", name: "ST-LAURENT", platform: "D")))
        // The board is the one screen with nowhere else to say it: the name has
        // been a station's name since the platform moved onto a plate, so
        // without this the platform you chose is gone when you arrive.
        #expect(crumbs.map(\.platform) == [nil, nil, nil, "D"])
    }

    @Test("a stop with no platform carries none rather than an empty one")
    func none() {
        let crumbs = Trail.crumbs(
            of: .board(
                .bus, seven, "Carleton",
                Stop(id: "6698", code: "6698", name: "ST-LAURENT / OGILVIE", platform: "")))
        #expect(crumbs.last?.platform == nil)
    }
}

@Suite("Service marks")
struct ServiceMarkTests {
    private func route(_ shortName: String, _ colour: String = "FFFFFF") -> Route {
        Route(shortName: shortName, longName: "A <> B", colour: colour)
    }

    @Test("a kind whose rule is its number states that number")
    func numberingMatchesClassification() {
        // The hint on a heading and the switch that fills the section under it
        // are two statements of one fact, written twice. This keeps them the
        // same: every number a hint claims has to classify back to the kind
        // that claimed it.
        for n in 301...305 { #expect(route("\(n)").service(in: .bus) == .shopper) }
        for n in 400...459 { #expect(route("\(n)").service(in: .bus) == .event) }
        for n in 600...699 { #expect(route("\(n)").service(in: .bus) == .school) }
        #expect(route("N45", "0057B8").service(in: .bus) == .night)
    }

    @Test("Connexion's number is a description of the feed, not a rule")
    func connexionIsSortedByColour() {
        // The one hint with nothing behind it. Connexion is decided by colour,
        // so a 200 in any other colour is not one — and a Connexion route
        // numbered outside the 200s would still be one, and the heading over
        // it would be claiming something untrue. All 18 in the feed today run
        // 221 to 299, which is the whole of the guarantee.
        for n in 200...299 {
            #expect(route("\(n)", Service.connexion.colour).service(in: .bus) == .connexion)
        }
        #expect(route("234").service(in: .bus) == .other)
    }

    @Test("a kind with no pattern claims none")
    func silentWhereThereIsNoRule() {
        // Frequent runs 5 to 111, Local 8 to 197, Other 13 to 566. No range
        // describes any of them, and Replacement is lettered rather than
        // numbered — R1 and R2 for a closed line, E1 for the Shuttle Express.
        #expect(Service.frequent.numbering.isEmpty)
        #expect(Service.local.numbering.isEmpty)
        #expect(Service.other.numbering.isEmpty)
        #expect(Service.replacement.numbering.isEmpty)
        #expect(route("R1").service(in: .bus) == .replacement)
        #expect(route("E1").service(in: .bus) == .replacement)
    }

    @Test("a heading's colour is one that would have sorted a route into it")
    func colourMatchesClassification() {
        for kind in [Service.frequent, .connexion, .local] {
            #expect(route("42", kind.colour).service(in: .bus) == kind)
        }
        // White is what the other kinds wear, and white alone decides nothing:
        // the number does. 42 is not in any of the numbered ranges.
        #expect(route("42", Service.school.colour).service(in: .bus) == .other)
    }
}

@Suite("Spoken names")
struct SpokenTests {
    @Test("a route too wide for its row still has a whole name somewhere")
    func spoken() {
        let wide = Route(
            shortName: "75",
            longName: "Tunney's Pasture & N Rideau <> Barrhaven Centre / Cambrian",
            colour: "0057b8")
        // 373 points of name in a row that gives it 226. The arrow is a glyph a
        // tooltip has no use for, so it is read as the word it stands for.
        #expect(wide.spoken == "Tunney's Pasture & N Rideau to Barrhaven Centre / Cambrian")
    }

    @Test("a route with one end says that")
    func loop() {
        let loop = Route(shortName: "615", longName: "Parliament ~ Parlement", colour: "FFFFFF")
        #expect(loop.ends == nil)
        #expect(loop.spoken == "Parliament")
    }
}

@Suite("Stop in full")
struct StopSpokenTests {
    private func stop(_ name: String, _ platform: String, boards: Bool) -> Stop {
        Stop(id: "3027", code: "3027", name: name, platform: platform, boards: boards)
    }

    @Test("a row that only drops off keeps its code somewhere")
    func dropOff() {
        // The row's margin says why it cannot be opened instead of saying the
        // code, because the reason is what a dimmed row owes the reader. The
        // code is still worth having: this stop takes plenty of buses, just not
        // this one.
        #expect(
            stop("Blair", "2", boards: false).spoken == "Blair · #3027 · Platform 2 · drop-off only"
        )
    }

    @Test("an ordinary row says what it is without the apology")
    func boarding() {
        #expect(stop("St-Laurent", "D", boards: true).spoken == "St-Laurent · #3027 · Platform D")
        #expect(
            stop("St-Laurent / Ogilvie", "", boards: true).spoken == "St-Laurent / Ogilvie · #3027")
    }
}
