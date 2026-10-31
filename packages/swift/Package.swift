// swift-tools-version:5.9
//
// The Swift package for inillucent, for iOS and macOS.
//
// `InillucentFFI` is the C library, built by `build-xcframework.sh` for the iPhone, the iOS
// simulator and the Mac as a static library in one XCFramework, with the header and a module map so
// Swift can import it. `Inillucent` is the Swift API over it. The XCFramework is a build product and
// is not checked in: run `sh packages/swift/build-xcframework.sh` on a Mac before `swift build`.

import PackageDescription

let package = Package(
    name: "Inillucent",
    platforms: [.iOS(.v13), .macOS(.v11)],
    products: [
        .library(name: "Inillucent", targets: ["Inillucent"]),
    ],
    targets: [
        .binaryTarget(name: "InillucentFFI", path: "InillucentFFI.xcframework"),
        .target(
            name: "Inillucent",
            dependencies: ["InillucentFFI"],
            resources: [.copy("PrivacyInfo.xcprivacy")],
            // The Rust standard library's random numbers on Apple platforms come from the Security
            // framework, and nothing else the static library uses is outside libSystem.
            linkerSettings: [.linkedFramework("Security")]
        ),
        .testTarget(
            name: "InillucentTests",
            dependencies: ["Inillucent"],
            // A copy of drivers/conformance/suite.json, made by run-tests.sh, because a package
            // resource has to be inside the target's folder.
            resources: [.copy("Resources/suite.json")]
        ),
    ]
)
