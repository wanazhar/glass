import UIKit
import UserNotifications

/// Opt-in alerts while Glass is in the background but still connected, like the web client's notifications.
@MainActor
enum Attention {
    static let key = "glass.notifications.v1"

    static var isOn: Bool { UserDefaults.standard.bool(forKey: key) }

    /// Asks for permission when turning on; returns the resulting state.
    static func set(_ on: Bool) async -> Bool {
        var granted = false
        if on {
            granted = (try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound])) ?? false
        }
        UserDefaults.standard.set(granted, forKey: key)
        return granted
    }

    static func notify(_ title: String, conversation: String, id: String) {
        guard isOn, UIApplication.shared.applicationState != .active else { return }
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = conversation
        content.sound = .default
        content.threadIdentifier = "glass-" + id
        UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: "glass-" + id, content: content, trigger: nil))
    }
}
