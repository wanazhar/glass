import SwiftUI

@main
struct GlassApp: App {
    @State private var model: AppModel = {
        if ProcessInfo.processInfo.arguments.contains("-GlassDemo") { return .demo() }
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
    @State private var showsSplash = !ProcessInfo.processInfo.arguments.contains("-GlassResetState")
        && !ProcessInfo.processInfo.arguments.contains("-GlassSkipSplash")
    private var demoLaunchFrame: String? {
        guard ProcessInfo.processInfo.arguments.contains("-GlassDemo") else { return nil }
        return UserDefaults.standard.string(forKey: "GlassLaunchFrame")
    }

    var body: some View {
        Group {
            if model.endpoint == nil || model.phase == .unpaired {
                PairingView()
            } else {
                ThreadsView()
            }
        }
        .preferredColorScheme(theme.colorScheme)
        .overlay {
            if showsSplash {
                LaunchView(sample: demoLaunchFrame == "ripple" ? 0.22 : demoLaunchFrame == nil ? nil : 0) { finishLaunch() }
                    .scaleEffect(demoLaunchFrame == "handoff" ? 1.05 : 1)
                    .opacity(demoLaunchFrame == "handoff" ? 0.48 : 1)
                    .transition(reduceMotion ? .opacity : .scale(scale: 1.08).combined(with: .opacity))
                    .zIndex(1)
            }
        }
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.2), value: model.phase == .unpaired)
        .task(id: model.endpoint) {
            if model.phase == .connecting { await model.refresh() }
        }
        .task {
            guard showsSplash, demoLaunchFrame == nil else { return }
            try? await Task.sleep(for: .milliseconds(1_450))
            if !Task.isCancelled { finishLaunch() }
        }
    }

    private func finishLaunch() {
        withAnimation(reduceMotion ? .easeOut(duration: 0.16) : .spring(duration: 0.36, bounce: 0)) {
            showsSplash = false
        }
    }
}
