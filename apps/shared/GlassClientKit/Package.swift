// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "GlassClientKit",
    platforms: [.iOS(.v18), .macOS(.v15)],
    products: [
        .library(name: "GlassModels", targets: ["GlassModels"]),
        .library(name: "GlassTransport", targets: ["GlassTransport"]),
        .library(name: "GlassAuth", targets: ["GlassAuth"]),
        .library(name: "GlassAPI", targets: ["GlassAPI"]),
        .library(name: "GlassTestSupport", targets: ["GlassTestSupport"]),
    ],
    targets: [
        .target(name: "GlassModels"),
        .target(name: "GlassTransport", dependencies: ["GlassModels"]),
        .target(name: "GlassAuth", dependencies: ["GlassModels", "GlassTransport"]),
        .target(name: "GlassAPI", dependencies: ["GlassModels", "GlassTransport", "GlassAuth"]),
        .target(
            name: "GlassTestSupport",
            dependencies: ["GlassModels", "GlassTransport"],
            resources: [.copy("Fixtures")]
        ),
        .testTarget(name: "GlassModelsTests", dependencies: ["GlassModels", "GlassTestSupport"]),
        .testTarget(name: "GlassTransportTests", dependencies: ["GlassTransport", "GlassTestSupport"]),
        .testTarget(name: "GlassAuthTests", dependencies: ["GlassAuth", "GlassTestSupport"]),
        .testTarget(name: "GlassAPITests", dependencies: ["GlassAPI", "GlassAuth", "GlassTestSupport"]),
    ],
    swiftLanguageModes: [.v6]
)
