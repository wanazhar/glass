import ActivityKit
import AppIntents
import Foundation

struct MarbleWorkActivityAttributes: ActivityAttributes {
    struct ContentState: Codable, Hashable {
        let taskID: String
        let agentName: String
        let glyph: String
        let hue: Int
        let isEveryone: Bool
        let phase: MarbleWorkPhase
        let startedAt: Date
    }

    let hostID: String
}

struct MarbleStopTurnIntent: LiveActivityIntent {
    static let title: LocalizedStringResource = "Stop Marble’s turn"
    static let isDiscoverable = false

    @Parameter(title: "Turn") var taskID: String
    @Parameter(title: "Host") var hostID: String

    init() {}

    init(taskID: String, hostID: String) {
        self.taskID = taskID
        self.hostID = hostID
    }

    @MainActor
    func perform() async throws -> some IntentResult {
        #if !MARBLE_WIDGET_EXTENSION
        try await MarbleStore.stopTurnFromLiveActivity(taskID: taskID, hostID: hostID)
        #endif
        return .result()
    }
}
