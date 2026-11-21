// swift-tools-version: 6.0
import PackageDescription

let package = Package(
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
