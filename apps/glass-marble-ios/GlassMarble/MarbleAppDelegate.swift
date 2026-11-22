import SwiftUI
import UIKit
import UserNotifications

/// Read the deep-link target out of an APNs payload Marble sent itself.
func marbleConversationId(from userInfo: [AnyHashable: Any]) -> String? {
    guard let marble = userInfo["marble"] as? [String: Any] else { return nil }
    let id = marble["conversationId"] as? String
    return (id?.isEmpty == false) ? id : nil
}

@main
final class MarbleAppDelegate: UIResponder, UIApplicationDelegate, UNUserNotificationCenterDelegate {
    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        UNUserNotificationCenter.current().delegate = self
        return true
    }

    /// A tapped notification opens the conversation that needed attention, not
    /// whatever happened to be selected last time.
    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        if let id = marbleConversationId(from: response.notification.request.content.userInfo) {
            NotificationCenter.default.post(name: .marbleOpenConversation, object: id)
        }
        completionHandler()
    }

    /// The open chat already shows its own updates.
    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification
    ) async -> UNNotificationPresentationOptions {
        let userInfo = notification.request.content.userInfo
        let target = marbleConversationId(from: userInfo)
        let taskID = (userInfo["marble"] as? [String: Any])?["taskId"] as? String
        return await MainActor.run {
            let shouldSuppress = UIApplication.shared.connectedScenes.contains { scene in
                guard scene.activationState == .foregroundActive,
                      let delegate = scene.delegate as? MarbleSceneDelegate else { return false }
                return !delegate.shouldPresentNotification(target: target, taskID: taskID)
            }
            return shouldSuppress ? [] : [.banner, .sound, .list]
        }
    }

    /// The device token is handed to the product store, which registers it with
    /// the self-hosted host so Marble can push a notification when it needs you.
    func application(
        _ application: UIApplication,
        didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data
    ) {
        guard MarbleFeatures.remotePush else { return }
        let token = deviceToken.map { String(format: "%02x", $0) }.joined()
        NotificationCenter.default.post(name: .marblePushToken, object: token)
    }

    func application(
        _ application: UIApplication,
        didFailToRegisterForRemoteNotificationsWithError error: Error
    ) {
        NotificationCenter.default.post(
            name: .marblePushRegistrationFailed,
            object: error.localizedDescription
        )
    }

    func application(
        _ application: UIApplication,
        configurationForConnecting connectingSceneSession: UISceneSession,
        options: UIScene.ConnectionOptions
    ) -> UISceneConfiguration {
        let configuration = UISceneConfiguration(
            name: "Marble",
            sessionRole: connectingSceneSession.role
        )
        configuration.delegateClass = MarbleSceneDelegate.self
        return configuration
    }
}

final class MarbleSceneDelegate: UIResponder, UIWindowSceneDelegate {
    var window: UIWindow?
    #if DEBUG
    private let store: MarbleStore = {
        ProcessInfo.processInfo.arguments.contains("-MarbleUITestFixture")
            ? MarbleStore.uiTestFixture()
            : MarbleStore()
    }()
    #else
    private let store = MarbleStore()
    #endif
    private let navigation = MarbleNavigationModel()

    func shouldPresentNotification(target: String?, taskID: String?) -> Bool {
        guard store.onboarded, !store.needsToken,
              window?.rootViewController?.presentedViewController == nil else { return true }
        let visibleChat = navigation.state.visibleChat { id in
            store.prism.marbles.first { $0.id == id }?.conversationId
        }
        // The host can address a mirrored Everyone turn by its engine chat.
        let mirrored = taskID.map { id in
            !id.isEmpty && (store.everyoneTaskIds.contains(id) || store.everyoneEntries.contains { $0.taskId == id })
        } ?? false
        return MarbleNotificationCursor.shouldPresent(
            target: visibleChat == everyoneConversationId && mirrored ? everyoneConversationId : target,
            visibleChat: visibleChat
        )
    }

    func scene(
        _ scene: UIScene,
        willConnectTo session: UISceneSession,
        options connectionOptions: UIScene.ConnectionOptions
    ) {
        guard let windowScene = scene as? UIWindowScene else { return }
        // A notification can launch the app cold; honour its target too.
        if let response = connectionOptions.notificationResponse,
           let id = marbleConversationId(from: response.notification.request.content.userInfo) {
            navigation.queuePendingOpen(id)
        }
        let root = MarbleRootView()
            .environmentObject(store)
            .environmentObject(navigation)
        let controller = UIHostingController(rootView: root)
        controller.view.backgroundColor = .clear

        let window = UIWindow(windowScene: windowScene)
        window.rootViewController = controller
        window.tintColor = .systemBlue
        window.makeKeyAndVisible()
        self.window = window
    }
}

extension Notification.Name {
    /// Posted with the APNs device token as a hex string.
    static let marblePushToken = Notification.Name("marble.push.token")
    static let marblePushRegistrationFailed = Notification.Name("marble.push.failed")
    /// Posted with a conversation id when a notification asks to open one.
    static let marbleOpenConversation = Notification.Name("marble.open.conversation")
}
