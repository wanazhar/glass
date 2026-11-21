// swift-tools-version: 6.0
import PackageDescription

// MARBLE_HOST_TESTS=1 switches the manifest to a host-side MarbleCore library
// plus XCTest target so the transport/SSE/compatibility code is testable on
// Linux; otherwise it builds the single-app module exactly like Xcode.
let hostTests = Context.environment["MARBLE_HOST_TESTS"] == "1"

let package: Package
if hostTests {
    package = Package(
        name: "MarbleCore",
        targets: [
            .target(
                name: "MarbleCore",
                path: "GlassMarble/Core"
            ),
            .testTarget(
                name: "MarbleCoreTests",
                dependencies: ["MarbleCore"],
                path: "Tests/MarbleCoreTests",
                resources: [.copy("Fixtures")]
            ),
        ],
        swiftLanguageModes: [.v5]
    )
} else {
    package = Package(
        name: "GlassMarble",
        platforms: [.iOS("18.0")],
        products: [.executable(name: "GlassMarble", targets: ["GlassMarble"])],
        targets: [
            .executableTarget(
                name: "GlassMarble",
                path: "GlassMarble",
                exclude: ["Assets.xcassets", "Base.lproj"]
            )
        ],
        swiftLanguageModes: [.v5]
    )
}
