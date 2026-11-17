import SwiftUI
import UIKit

@main
final class MarbleAppDelegate: UIResponder, UIApplicationDelegate {
    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        true
    }

    /// The device token is handed to the product store, which registers it with
    /// the self-hosted host so Marble can push a notification when it needs you.
    func application(
        _ application: UIApplication,
        didRegisterForRemoteNotificationsWithDeviceToken deviceToken: Data
    ) {
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
    private let store = MarbleStore()

    func scene(
        _ scene: UIScene,
        willConnectTo session: UISceneSession,
        options connectionOptions: UIScene.ConnectionOptions
    ) {
        guard let windowScene = scene as? UIWindowScene else { return }
        let root = MarbleRootView()
            .environmentObject(store)
            .preferredColorScheme(.dark)
        let controller = UIHostingController(rootView: root)
        controller.view.backgroundColor = .clear

        let window = UIWindow(windowScene: windowScene)
        window.rootViewController = controller
        window.tintColor = UIColor(red: 0.43, green: 0.66, blue: 0.95, alpha: 1)
        window.makeKeyAndVisible()
        self.window = window
    }
}

extension Notification.Name {
    /// Posted with the APNs device token as a hex string.
    static let marblePushToken = Notification.Name("marble.push.token")
    static let marblePushRegistrationFailed = Notification.Name("marble.push.failed")
}
