// Every shape of bilingual name the feed actually contains, and the shapes it
// contains that must survive untouched.

import Testing

@testable import OTransitKit

@Suite("Names")
struct NameTests {
    @Test("the French half after a tilde is dropped")
    func tilde() {
        #expect("PARLIAMENT ~ PARLEMENT".english == "PARLIAMENT")
        #expect("DOW'S LAKE ~ LAC DOW".english == "DOW'S LAKE")
        #expect("AIRPORT ~ AÉROPORT".english == "AIRPORT")
        #expect("ORLÉANS PARK & RIDE ~ PARC-O-BUS ORLÉANS".english == "ORLÉANS PARK & RIDE")
        #expect("Petrie Island ~ Île Petrie".english == "Petrie Island")
        #expect("CFIA ~ ACIA".english == "CFIA")
    }

    @Test("a slash inside the English half is kept, because it is an intersection")
    func slashWithin() {
        // Both halves carry one. Splitting on the slash instead of the tilde
        // would cut this in the middle of the English.
        #expect(
            "South Keys / Hunt Club Loop ~ South Keys / Boucle Hunt Club".english
                == "South Keys / Hunt Club Loop")
        #expect(
            "Carling Campus/Abbott POC ~ Complexe Carling/Abbott POC".english
                == "Carling Campus/Abbott POC")
    }

    @Test("a translated compass direction is dropped, and the English one stays")
    func compass() {
        // BELL H.S and BELL H.S WEST are two different stops, so the direction
        // is not noise the way the French after it is.
        #expect("BELL H.S  WEST / OUEST".english == "BELL H.S WEST")
        #expect("CARLETON O-TRAIN SOUTH / SUD".english == "CARLETON O-TRAIN SOUTH")
        #expect("UOTTAWA O-TRAIN EAST / EST".english == "UOTTAWA O-TRAIN EAST")
        #expect("LIMEBANK O-TRAIN NORTH / NORD".english == "LIMEBANK O-TRAIN NORTH")
    }

    @Test("an ordinary intersection is left alone")
    func intersection() {
        // 5,428 stop names carry a slash and mean a corner by it. A rule that
        // read the slash itself would rename every one of them.
        #expect("ST-LAURENT / OGILVIE".english == "ST-LAURENT / OGILVIE")
        #expect("ALTA VISTA / ROLLAND".english == "ALTA VISTA / ROLLAND")
        #expect("RIDEAU / AUGUSTA".english == "RIDEAU / AUGUSTA")
    }

    @Test("the feed's doubled spaces are closed up")
    func spacing() {
        #expect("FRANK BENDER  / INNES".english == "FRANK BENDER / INNES")
        #expect("PROVENCE /  INNES".english == "PROVENCE / INNES")
        #expect(
            "DOW'S LAKE O-TRAIN /  O-TRAIN LAC DOW".english
                == "DOW'S LAKE O-TRAIN / O-TRAIN LAC DOW")
    }

    @Test("a route's ends and title are shortened on both sides of the arrow")
    func route() {
        let fifteen = Route(
            shortName: "15", longName: "Blair <> Parliament ~ Parlement", colour: "0057b8")
        let ends = try! #require(fifteen.ends)
        #expect(ends.from == "Blair")
        #expect(ends.to == "Parliament")

        // 615 has no ends to split, so the shortening has to reach the whole
        // name too and not only the halves.
        let six15 = Route(shortName: "615", longName: "Parliament ~ Parlement", colour: "FFFFFF")
        #expect(six15.ends == nil)
        #expect(six15.title == "Parliament")
    }

    @Test("a direction shows English and matches on what the feed wrote")
    func direction() {
        let toward = Direction(headsign: "Airport ~ Aéroport", trips: 42)
        #expect(toward.name == "Airport")
        // The key is untouched: it is what the stop query and the board filter
        // compare against, and a shortened one would match nothing.
        #expect(toward.headsign == "Airport ~ Aéroport")
        #expect(toward.id == "Airport ~ Aéroport")
    }

    @Test("the feed's shouting is undone")
    func plainNames() {
        #expect("ST-LAURENT / OGILVIE".titled == "St-Laurent / Ogilvie")
        #expect("BILLINGS BRIDGE".titled == "Billings Bridge")
        #expect("LA VÉRENDRYE".titled == "La Vérendrye")
        #expect("ORLÉANS PARK & RIDE".titled == "Orléans Park & Ride")
        #expect("417 RAMP / MOODIE (MOODIE A)".titled == "417 Ramp / Moodie (Moodie A)")
    }

    @Test("an apostrophe breaks a word only when one letter comes before it")
    func apostrophes() {
        // Foundation's own capitalisation gets both of these wrong, in
        // opposite directions.
        #expect("TUNNEY'S PASTURE".titled == "Tunney's Pasture")
        #expect("MOONEY'S BAY".titled == "Mooney's Bay")
        #expect("D'ARCY MCGEE".titled == "D'Arcy McGee")
        #expect("O'CONNOR / GLADSTONE".titled == "O'Connor / Gladstone")
        #expect("L'ÉGLISE".titled == "L'Église")
    }

    @Test("letters said one at a time keep their capitals")
    func initialisms() {
        #expect("RING / TOH GENERAL CAMPUS".titled == "Ring / TOH General Campus")
        #expect("CHEO / SMYTH".titled == "CHEO / Smyth")
        #expect("NCC / NRC".titled == "NCC / NRC")
        // Initials are recognised by their shape — every run between the stops
        // is one letter — so none of these had to be listed.
        #expect("A.Y. JACKSON".titled == "A.Y. Jackson")
        #expect("MERIVALE H.S".titled == "Merivale H.S")
        #expect("Q.C.H. / CAMPEAU".titled == "Q.C.H. / Campeau")
        #expect("MCBEAN / BURKE ST E".titled == "McBean / Burke St E")
    }

    @Test("an abbreviation that is not initials is not shouted")
    func abbreviations() {
        // Same punctuation, different shape: two letters before the stop.
        #expect("DUNNING RD.".titled == "Dunning Rd.")
        #expect("ST-LAURENT / AD. 1055".titled == "St-Laurent / Ad. 1055")
    }

    @Test("letters straight after a digit are the tail of an ordinal")
    func ordinals() {
        #expect("8TH LINE / BYRON".titled == "8th Line / Byron")
        #expect("MCCURDY / 1ST CASTLEFRANK".titled == "McCurdy / 1st Castlefrank")
        #expect("OGILVIE / 2ND ELMRIDGE".titled == "Ogilvie / 2nd Elmridge")
        #expect("QUEENSDALE / FIRST (1ST)".titled == "Queensdale / First (1st)")
    }

    @Test("a letter after a digit that is not an ordinal keeps its capital")
    func designations() {
        // These are platform designations, the same values the plate draws.
        // A rule that lowered every letter run after a digit put a 4a in the
        // name beside a 4A on the plate, on one row.
        #expect("HUNT CLUB LOOP - STOP 1A".titled == "Hunt Club Loop - Stop 1A")
        #expect("HUNT CLUB LOOP - STOP 2A".titled == "Hunt Club Loop - Stop 2A")
        #expect("JEANNE D'ARC 4A".titled == "Jeanne D'Arc 4A")
        #expect("LINCOLN FIELDS 3B".titled == "Lincoln Fields 3B")
        #expect("TERRY FOX 4C".titled == "Terry Fox 4C")
    }

    @Test("Mc is lifted and Mac is left alone")
    func scottishNames() {
        #expect("MCARTHUR".titled == "McArthur")
        #expect("MCCURDY / MCINTOSH".titled == "McCurdy / McIntosh")
        // MACFARLANE wants MacFarlane and MACY does not, and nothing in either
        // name says which, so neither is guessed at.
        #expect("MACFARLANE / MERIVALE".titled == "Macfarlane / Merivale")
        #expect("KIRKWOOD / MACY".titled == "Kirkwood / Macy")
    }

    @Test("the one name that starts lowercase on purpose")
    func university() {
        #expect("UOTTAWA".titled == "uOttawa")
    }

    @Test("shortening and casing compose, in that order")
    func together() {
        #expect("PARLIAMENT ~ PARLEMENT".english.titled == "Parliament")
        #expect("BELL H.S  WEST / OUEST".english.titled == "Bell H.S West")
        #expect("DOW'S LAKE ~ LAC DOW".english.titled == "Dow's Lake")
    }
}
