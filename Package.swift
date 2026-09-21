// swift-tools-version: 6.4
//
// Two targets, because the program is a model with a shell over it, the same
// way the Rust and Go ports are. OTransitKit decides what a screen holds and
// answers what the cache knows; OTransit draws it in the menu bar. The tests
// depend on the kit alone, which is what keeps them free of a window.
//
// No dependencies. SQLite comes from the SDK as a system module, SwiftUI draws
// the bar item, and Foundation does the rest.

import PackageDescription

let package = Package(
    name: "otransit",
    // The floor is the current release, and the tools version above is what
    // lets this line name it: `.v27` needs PackageDescription 6.4.
    //
    // It started at 14, which cost the Liquid Glass design on every Mac
    // including this one: macOS gives an app that design only when it is linked
    // against 26 or newer and runs anything older in compatibility mode, and
    // SwiftPM stamps the SDK it builds against from this line. 26 bought the
    // appearance; 27 is here because this app has one user, that user is on 27,
    // and a floor below the machine it runs on buys nothing it cannot spend.
    platforms: [.macOS(.v27)],
    targets: [
        .target(name: "OTransitKit"),
        .executableTarget(name: "OTransit", dependencies: ["OTransitKit"]),
        .testTarget(name: "OTransitKitTests", dependencies: ["OTransitKit"]),
    ]
)
