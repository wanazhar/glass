import SwiftUI

@main
struct GlassApp: App {
    @State private var model: AppModel = {
        let arguments = ProcessInfo.processInfo.arguments
        #if DEBUG
        if arguments.contains("-GlassDemo") {
            return .demo(threadCount: arguments.contains("-GlassDrawerPerformance") ? 1_000 : 336)
        }
        #endif
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
    #if DEBUG
    @Environment(\.scenePhase) private var scenePhase
    #endif
    @AppStorage(ThemePreference.key) private var theme: ThemePreference = .system
    @State private var showsSplash = !ProcessInfo.processInfo.arguments.contains("-GlassResetState")
        && !ProcessInfo.processInfo.arguments.contains("-GlassSkipSplash")
    private var demoLaunchFrame: String? {
        #if DEBUG
        guard ProcessInfo.processInfo.arguments.contains("-GlassDemo") else { return nil }
        if LensPreview.current == .splash { return "ripple" }
        return UserDefaults.standard.string(forKey: "GlassLaunchFrame")
        #else
        return nil
        #endif
    }

    var body: some View {
        Group {
            if model.endpoint == nil || model.phase == .unpaired {
                PairingView()
            } else {
                ThreadsView()
            }
        }
        .accessibilityHidden(showsSplash && !reduceMotion)
        .overlay {
            if showsSplash && !reduceMotion {
                LaunchView(sample: launchSample, skip: finishLaunch)
                    .transition(.opacity)
            }
        }
        .preferredColorScheme(theme.colorScheme)
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.2), value: model.phase == .unpaired)
        .task(id: model.endpoint) {
            if model.phase == .connecting { await model.refresh() }
        }
        #if DEBUG
        .task(id: scenePhase) {
            if scenePhase == .active, ProcessInfo.processInfo.arguments.contains("-GlassLiveActivityPreview") {
                KeepAlive.shared.previewActivity()
            }
        }
        #endif
        .task(id: reduceMotion) {
            if reduceMotion {
                showsSplash = false
                return
            }
            guard showsSplash, demoLaunchFrame == nil else { return }
            try? await Task.sleep(for: .seconds(LaunchView.duration))
            if !Task.isCancelled { finishLaunch() }
        }
    }

    private var launchSample: Double? {
        switch demoLaunchFrame {
        case "start": 0
        case "ripple": 0.23
        case "handoff": LaunchView.duration
        default: nil
        }
    }

    private func finishLaunch() {
        withAnimation(reduceMotion ? nil : .easeOut(duration: 0.18)) { showsSplash = false }
    }
}
