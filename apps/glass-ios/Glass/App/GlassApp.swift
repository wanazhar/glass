import SwiftUI

@main
struct GlassApp: App {
    @State private var model: AppModel = {
        let model = AppModel()
        if ProcessInfo.processInfo.arguments.contains("-GlassResetState") { model.resetForTesting() }
        return model
    }()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
        }
    }
}

struct RootView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        Group {
            if model.endpoint == nil || model.phase == .unpaired {
                PairingView()
                    .transition(.opacity.combined(with: .scale(scale: 0.98)))
            } else {
                HostTabView()
                    .transition(.opacity)
            }
        }
        .animation(.smooth(duration: 0.35), value: model.phase == .unpaired)
        .task(id: model.endpoint) {
            if model.phase == .connecting { await model.refresh() }
        }
    }
}
