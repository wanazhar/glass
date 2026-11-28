import ActivityKit
import CryptoKit
import Foundation
import OSLog

@MainActor
final class MarbleWorkActivity {
    typealias State = MarbleWorkActivityAttributes.ContentState
    private typealias LiveActivity = Activity<MarbleWorkActivityAttributes>

    private static let log = Logger(subsystem: "com.wanazhar.glassmarble", category: "work-activity")
    private var activity: LiveActivity?
    private var lastContent: ActivityContent<State>?
    private var updateTask: Task<Void, Never>?
    private var dismissedTaskID: String?

    var taskID: String? { lastContent?.state.taskID ?? activity?.content.state.taskID }
    var hasActivity: Bool {
        guard let activity else { return false }
        return activity.activityState == .active || activity.activityState == .stale
    }

    static func hostID(_ url: URL) -> String {
        SHA256.hash(data: Data(url.absoluteString.utf8)).map { String(format: "%02x", $0) }.joined()
    }

    func present(_ state: State?, hostID: String, canStart: Bool, streamIsLive: Bool) {
        if let activity, activity.attributes.hostID != hostID {
            end()
        }
        if activity == nil {
            activity = LiveActivity.activities.first {
                $0.attributes.hostID == hostID && ($0.activityState == .active || $0.activityState == .stale)
            }
            lastContent = activity?.content
        }
        guard let state else {
            end()
            return
        }
        if let activity, activity.activityState == .dismissed || activity.activityState == .ended {
            dismissedTaskID = taskID
            self.activity = nil
            lastContent = nil
        }
        let now = Date.now
        let content = ActivityContent(state: state, staleDate: now.addingTimeInterval(45))
        if let activity {
            if let lastContent, lastContent.state == state {
                guard streamIsLive, (lastContent.staleDate ?? now).timeIntervalSince(now) <= 25 else { return }
            }
            lastContent = content
            let previous = updateTask
            updateTask = Task {
                await previous?.value
                await activity.update(content)
            }
        } else if canStart, dismissedTaskID != state.taskID, ActivityAuthorizationInfo().areActivitiesEnabled {
            do {
                activity = try LiveActivity.request(attributes: .init(hostID: hostID), content: content, pushType: nil)
                lastContent = content
            } catch {
                Self.log.debug("Could not start work Live Activity: \(error.localizedDescription, privacy: .public)")
            }
        }
    }

    func end() {
        guard let activity else { return }
        self.activity = nil
        lastContent = nil
        let previous = updateTask
        updateTask = Task {
            await previous?.value
            await activity.end(nil, dismissalPolicy: .immediate)
        }
    }

    func waitForChanges() async { await updateTask?.value }
}
