import Foundation

/// Identity and version of the running build, read from the bundle.
struct AppInfo: Equatable, Sendable {
    var displayName: String
    var bundleIdentifier: String
    var version: String
    var build: String

    static var current: AppInfo { AppInfo(bundle: .main) }

    init(displayName: String, bundleIdentifier: String, version: String, build: String) {
        self.displayName = displayName
        self.bundleIdentifier = bundleIdentifier
        self.version = version
        self.build = build
    }

    init(bundle: Bundle) {
        let info = bundle.infoDictionary ?? [:]
        displayName = info["CFBundleDisplayName"] as? String ?? info["CFBundleName"] as? String ?? "Glass"
        bundleIdentifier = bundle.bundleIdentifier ?? ""
        version = info["CFBundleShortVersionString"] as? String ?? "0"
        build = info["CFBundleVersion"] as? String ?? "0"
    }

    /// "0.1.0 (1)", the form shown in Settings › About.
    var versionLabel: String { "\(version) (\(build))" }
}
