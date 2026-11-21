import SwiftUI

/// Paired shell. Native `TabView` adopts the Liquid Glass tab bar on iOS 26. Tabs show icons only;
/// each keeps a spoken name for VoiceOver and UI tests.
struct HostTabView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model
        TabView(selection: $model.selectedTab) {
            Tab(value: .threads) {
                ThreadsView()
            } label: {
                TabIcon(title: "Conversations", systemImage: "bubble.left.and.text.bubble.right")
            }
            .accessibilityIdentifier("tab.threads")
            Tab(value: .desktop) {
                NavigationStack { DesktopView() }
            } label: {
                TabIcon(title: "Desktop", systemImage: "display")
            }
            .accessibilityIdentifier("tab.desktop")
            Tab(value: .settings) {
                NavigationStack(path: $model.settingsPath) {
                    SettingsView()
                        .navigationDestination(for: SettingsRoute.self) { $0.destination }
                }
            } label: {
                TabIcon(title: "Settings", systemImage: "gearshape")
            }
            .accessibilityIdentifier("tab.settings")
        }
        .background {
            // No visible label: it only carries ⌘, and must not show up as on-screen text.
            Button { model.show(nil) } label: { EmptyView() }
                .keyCommand(.settings)
                .allowsHitTesting(false)
                .accessibilityHidden(true)
        }
    }
}

private struct TabIcon: View {
    var title: String
    var systemImage: String

    var body: some View {
        Image(systemName: systemImage)
            .accessibilityLabel(Text(title))
    }
}
