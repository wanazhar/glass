import SwiftUI

/// Paired shell. Native `TabView` adopts the Liquid Glass tab bar on iOS 26.
struct HostTabView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model
        TabView(selection: $model.selectedTab) {
            Tab("Conversations", systemImage: "bubble.left.and.text.bubble.right", value: .threads) {
                ThreadsView()
            }
            .accessibilityIdentifier("tab.threads")
            Tab("Host", systemImage: "desktopcomputer", value: .host) {
                NavigationStack { OverviewView() }
            }
            .accessibilityIdentifier("tab.host")
            Tab("Desktop", systemImage: "display", value: .desktop) {
                NavigationStack { DesktopView() }
            }
            .accessibilityIdentifier("tab.desktop")
            Tab("Diagnostics", systemImage: "stethoscope", value: .diagnostics) {
                NavigationStack { DiagnosticsView() }
            }
            .accessibilityIdentifier("tab.diagnostics")
            Tab("Settings", systemImage: "gearshape", value: .settings) {
                NavigationStack { SettingsView() }
            }
            .accessibilityIdentifier("tab.settings")
        }
        .background {
            // No visible label: it only carries ⌘, and must not show up as on-screen text.
            Button { model.selectedTab = .settings } label: { EmptyView() }
                .keyCommand(.settings)
                .allowsHitTesting(false)
                .accessibilityHidden(true)
        }
    }
}
