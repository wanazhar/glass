import SwiftUI

@main
struct GlassApp: App {
    @State private var model: AppModel = {
        let model = AppModel()
        if ProcessInfo.processInfo.arguments.contains("-GlassResetState") { model.resetForTesting() }
        return model
    }()

    init() {
        Attention.install()
        KeepAlive.install()
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
        }
    }
}

struct RootView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @AppStorage(ThemePreference.key) private var theme: ThemePreference = .system

    var body: some View {
        Group {
            if model.endpoint == nil || model.phase == .unpaired {
                PairingView()
            } else {
                ThreadsView()
            }
        }
        .preferredColorScheme(theme.colorScheme)
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.2), value: model.phase == .unpaired)
        .task(id: model.endpoint) {
            if model.phase == .connecting { await model.refresh() }
        }
    }
}
