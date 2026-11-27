import SwiftUI
import UIKit
import UserNotifications

/// Read the deep-link target out of a local notification.
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
        MarbleBackgroundConnection.endActivitiesFromPreviousLaunch()
        return true
    }

    func applicationWillTerminate(_ application: UIApplication) {
        MarbleBackgroundConnection.endActivitiesBeforeTermination()
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
        let marble = userInfo["marble"] as? [String: Any]
        let threadId = marble?["threadId"] as? String
        let mirroredInEveryone = marble?["mirroredInEveryone"] as? String == "true"
        return await MainActor.run {
            let shouldSuppress = UIApplication.shared.connectedScenes.contains { scene in
                guard scene.activationState == .foregroundActive || scene.activationState == .foregroundInactive,
                      let delegate = scene.delegate as? MarbleSceneDelegate else { return false }
                return !delegate.shouldPresentNotification(
                    target: target, threadId: threadId, mirroredInEveryone: mirroredInEveryone
                )
            }
            return shouldSuppress ? [] : [.banner, .sound, .list]
        }
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
    private static var hasPresentedSplash = false
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

    func shouldPresentNotification(target: String?, threadId: String?, mirroredInEveryone: Bool) -> Bool {
        guard store.onboarded, !store.needsToken,
              window?.rootViewController?.presentedViewController == nil else { return true }
        let visibleChat = navigation.state.visibleChat { id in
            store.prism.marbles.first { $0.id == id }?.conversationId
        }
        return MarbleNotificationCursor.shouldPresent(
            target: target,
            visibleChat: visibleChat,
            threadId: threadId,
            visibleThreadId: navigation.state.visibleThreadId,
            mirroredInEveryone: mirroredInEveryone
        )
    }

    func scene(
        _ scene: UIScene,
        willConnectTo session: UISceneSession,
        options connectionOptions: UIScene.ConnectionOptions
    ) {
        guard let windowScene = scene as? UIWindowScene else { return }
        MarbleStore.liveActivityStore = store
        store.shouldPresentReply = { [weak self] target, threadId, mirroredInEveryone in
            self?.shouldPresentNotification(
                target: target, threadId: threadId, mirroredInEveryone: mirroredInEveryone
            ) ?? false
        }
        // A notification can launch the app cold; honour its target too.
        if let response = connectionOptions.notificationResponse,
           let id = marbleConversationId(from: response.notification.request.content.userInfo) {
            navigation.queuePendingOpen(id)
        }
        let showsSplash = !Self.hasPresentedSplash && connectionOptions.notificationResponse == nil
            && !ProcessInfo.processInfo.arguments.contains("-MarbleUITestFixture")
        Self.hasPresentedSplash = true
        let root = MarbleRootView(showsSplash: showsSplash)
            .environmentObject(store)
            .environmentObject(navigation)
        let controller = UIHostingController(rootView: root)
        controller.view.backgroundColor = UIColor(named: "MarblePaper")

        let window = UIWindow(windowScene: windowScene)
        window.rootViewController = controller
        window.backgroundColor = UIColor(named: "MarblePaper")
        window.tintColor = UIColor(named: "AccentColor")
        window.makeKeyAndVisible()
        self.window = window
    }

    func sceneWillResignActive(_ scene: UIScene) {
        store.appWillResignActive()
    }

    func sceneDidEnterBackground(_ scene: UIScene) {
        store.appDidEnterBackground()
    }

    func sceneDidBecomeActive(_ scene: UIScene) {
        store.appBecameActive()
    }

    func sceneDidDisconnect(_ scene: UIScene) {
        store.sceneDisconnected()
    }
}

extension Notification.Name {
    /// Posted with a conversation id when a notification asks to open one.
    static let marbleOpenConversation = Notification.Name("marble.open.conversation")
}
