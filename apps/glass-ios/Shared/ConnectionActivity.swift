import ActivityKit
import AppIntents
import Foundation

/// The Live Activity Glass shows while it keeps a host connection open in the background.
/// Updated on the device only; there is no push token. Its state is a new type rather than a change
/// to the earlier `ConnectionActivityAttributes`, so a widget from an older install never has to decode it.
struct LensConnectionAttributes: ActivityAttributes {
    struct ContentState: Codable, Hashable {
        var status: String
        /// Flips on every update, so the Lens can pulse gently.
        var pulse: Bool
        var workingAgents: Int? = nil
        var longestAgentStart: Date? = nil
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
