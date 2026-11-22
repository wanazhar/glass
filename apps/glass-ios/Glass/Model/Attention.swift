import UIKit
import UserNotifications

/// Opt-in local alerts for replies that arrive while Glass is running and you aren't looking at that
/// conversation. There is no remote push: nothing is sent once iOS suspends the app.
@MainActor
enum Attention {
    static let key = "glass.notifications.v1"
    static let notifiedKey = "glass.notified.v1"

    static var isOn: Bool { UserDefaults.standard.bool(forKey: key) }

    /// The conversation on screen, if any. Set by the conversation view as it appears and disappears.
    static var visibleThread: String?

    private static let presenter = ForegroundPresenter()

    /// Shows alerts as banners even while Glass is open, for a reply in another conversation.
    static func install() {
        UNUserNotificationCenter.current().delegate = presenter
    }

    /// Asks for permission when turning on; returns the resulting state.
    static func set(_ on: Bool) async -> Bool {
        var granted = false
        if on {
            granted = (try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound])) ?? false
        }
        UserDefaults.standard.set(granted, forKey: key)
        return granted
    }

    /// Alerts only when they're on and you aren't looking at that conversation in the open app.
    nonisolated static func shouldNotify(enabled: Bool, appState: UIApplication.State, looking: Bool) -> Bool {
        enabled && !(appState == .active && looking)
    }

    /// Posts at most one alert per event id and thread. `replayed` events are catch-up after a launch or
    /// reconnect, which never alert.
    static func notify(_ title: String, conversation: String, thread: String, event: String, replayed: Bool) {
        let ledger = NotifiedLedger(store: UserDefaults.standard, key: notifiedKey)
        guard !replayed, ledger.record(event, for: thread) else { return }
        let looking = visibleThread == thread
        guard shouldNotify(enabled: isOn, appState: UIApplication.shared.applicationState, looking: looking) else { return }
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = conversation
        content.sound = .default
        content.threadIdentifier = "glass-" + thread
        UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: "glass-\(thread)-\(event)", content: content, trigger: nil))
    }
}

/// The last event id that raised an alert, per conversation, kept across launches so nothing fires twice.
struct NotifiedLedger {
    let store: UserDefaults
    let key: String

    func last(for thread: String) -> String? {
        (store.dictionary(forKey: key) as? [String: String])?[thread]
    }

    /// Records `event` and returns true when it is newer than the last one alerted for `thread`.
    /// Event ids are `epoch:cursor`; a new epoch (host restart) is always new.
    func record(_ event: String, for thread: String) -> Bool {
        var all = store.dictionary(forKey: key) as? [String: String] ?? [:]
        if let last = all[thread], !Self.isNewer(event, than: last) { return false }
        all[thread] = event
        store.set(all, forKey: key)
        return true
    }

    static func isNewer(_ event: String, than last: String) -> Bool {
        let parse = { (s: String) -> (UInt64, UInt64)? in
            let parts = s.split(separator: ":")
            guard parts.count == 2, let e = UInt64(parts[0]), let c = UInt64(parts[1]) else { return nil }
            return (e, c)
        }
        guard let new = parse(event), let old = parse(last) else { return event != last }
        return new.0 != old.0 || new.1 > old.1
    }
}

private final class ForegroundPresenter: NSObject, UNUserNotificationCenterDelegate {
    func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification) async -> UNNotificationPresentationOptions {
        [.banner, .list, .sound]
    }
}
