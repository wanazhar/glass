// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "GlassClientKit",
    platforms: [.iOS(.v18), .macOS(.v15)],
    products: [
        .library(name: "GlassModels", targets: ["GlassModels"]),
        .library(name: "GlassTestSupport", targets: ["GlassTestSupport"]),
    ],
    targets: [
        .target(name: "GlassModels"),
        .target(
            name: "GlassTestSupport",
            dependencies: ["GlassModels"],
            resources: [.copy("Fixtures")]
        ),
        .testTarget(
            name: "GlassModelsTests",
            dependencies: ["GlassModels", "GlassTestSupport"]
        ),
    ],
    swiftLanguageModes: [.v6]
)
