import SwiftUI

/// Paired shell. Native `TabView` adopts the Liquid Glass tab bar on iOS 26.
struct HostTabView: View {
    var body: some View {
        TabView {
            Tab("Conversations", systemImage: "bubble.left.and.text.bubble.right") {
                ThreadsView()
            }
            .accessibilityIdentifier("tab.threads")
            Tab("Host", systemImage: "desktopcomputer") {
                NavigationStack { OverviewView() }
            }
            .accessibilityIdentifier("tab.host")
            Tab("Diagnostics", systemImage: "stethoscope") {
                NavigationStack { DiagnosticsView() }
            }
            .accessibilityIdentifier("tab.diagnostics")
            Tab("Settings", systemImage: "gearshape") {
                NavigationStack { SettingsView() }
            }
            .accessibilityIdentifier("tab.settings")
        }
    }
}
