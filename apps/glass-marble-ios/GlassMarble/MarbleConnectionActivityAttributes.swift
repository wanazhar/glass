import ActivityKit
import AppIntents
import Foundation

struct MarbleConnectionActivityAttributes: ActivityAttributes {
    struct ContentState: Codable, Hashable {
        let connectedAt: Date
    }

    let host: String
}

struct MarbleDisconnectIntent: LiveActivityIntent {
    static let title: LocalizedStringResource = "Disconnect from Marble"
    static let isDiscoverable = false

    @MainActor
    func perform() async throws -> some IntentResult {
        #if !MARBLE_WIDGET_EXTENSION
        MarbleStore.disconnectFromLiveActivity()
        // The app can be launched in the background solely to run this intent.
        for activity in Activity<MarbleConnectionActivityAttributes>.activities {
            await activity.end(nil, dismissalPolicy: .immediate)
        }
        #endif
        return .result()
    }
}
