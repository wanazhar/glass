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
            Tab("Diagnostics", systemImage: "stethoscope", value: .diagnostics) {
                NavigationStack { DiagnosticsView() }
            }
            .accessibilityIdentifier("tab.diagnostics")
            Tab("Settings", systemImage: "gearshape", value: .settings) {
                NavigationStack { SettingsView() }
            }
            .accessibilityIdentifier("tab.settings")
        }
    }
}
