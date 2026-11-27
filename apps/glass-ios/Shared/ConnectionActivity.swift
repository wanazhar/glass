import ActivityKit
import AppIntents
import Foundation

/// The Live Activity Glass shows while it keeps a host connection open in the background.
/// Updated on the device only; there is no push token.
struct ConnectionActivityAttributes: ActivityAttributes {
    struct ContentState: Codable, Hashable {
        var status: String
        var pulse: Bool? = nil
    }

    var host: String
}

extension Notification.Name {
    static let glassDisconnect = Notification.Name("com.wanazhar.glass.disconnect")
}

/// The Live Activity's Disconnect button. It runs in the app, which closes its connections and ends the activity.
struct DisconnectIntent: LiveActivityIntent {
    static let title: LocalizedStringResource = "Disconnect"
    static let description = IntentDescription("Closes Glass's background connection to your host.")

    init() {}

    @MainActor
    func perform() async throws -> some IntentResult {
        NotificationCenter.default.post(name: .glassDisconnect, object: nil)
        return .result()
    }
}
