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
