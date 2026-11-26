import SwiftUI

@main
struct GlassApp: App {
    @State private var model: AppModel = {
        let arguments = ProcessInfo.processInfo.arguments
        if arguments.contains("-GlassDemo") {
            return .demo(threadCount: arguments.contains("-GlassDrawerPerformance") ? 1_000 : 336)
        }
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
    @State private var handingOff = false
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
                LaunchView(sample: demoLaunchFrame == "ripple" ? 0.34 : demoLaunchFrame == nil ? nil : 0,
                           handingOff: handingOff || demoLaunchFrame == "handoff", skip: finishLaunch)
                    .transition(.opacity)
                    .zIndex(1)
            }
        }
        .animation(reduceMotion ? nil : .easeInOut(duration: 0.2), value: model.phase == .unpaired)
        .task(id: model.endpoint) {
            if model.phase == .connecting { await model.refresh() }
        }
        .task {
            guard showsSplash, demoLaunchFrame == nil else { return }
            try? await Task.sleep(for: .milliseconds(1_180))
            if !Task.isCancelled { finishLaunch() }
        }
    }

    private func finishLaunch() {
        guard !handingOff else { return }
        withAnimation(.easeOut(duration: 0.2)) { handingOff = true }
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(200))
            withAnimation(reduceMotion ? .easeOut(duration: 0.16) : .spring(duration: 0.28, bounce: 0)) {
                showsSplash = false
            }
        }
    }
}
