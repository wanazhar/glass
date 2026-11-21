import SwiftUI

struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @State private var confirmingForget = false
    @State private var hostSettings: HostSettingsModel?
    @AppStorage(ThemePreference.key) private var theme: ThemePreference = .system
    @AppStorage(ActivityAnimation.key) private var animation: ActivityAnimation = .dots
    @AppStorage(ToolDensity.key) private var density: ToolDensity = .compact
    @AppStorage(Attention.key) private var notifications = false

    var body: some View {
        @Bindable var model = model
        Form {
            if let endpoint = model.endpoint {
                Section {
                    LabeledContent("Host", value: endpoint.origin)
                    LabeledContent("Connection", value: endpoint.transportLabel)
                    Button("Forget This Host", role: .destructive) { confirmingForget = true }
                        .accessibilityIdentifier("settings.forget")
                } header: {
                    Text("Paired host")
                } footer: {
                    Text("The pairing is stored in this iPhone’s Keychain. Forgetting it does not affect threads on the host.")
                }
            }
            if let hostSettings {
                Section {
                    ForEach(HostSettingsSection.allCases) { section in
                        NavigationLink {
                            HostSettingsView(section: section, settings: hostSettings)
                        } label: {
                            Label(section.title, systemImage: section.systemImage)
                        }
                        .accessibilityIdentifier("settings.section.\(section.rawValue)")
                    }
                } header: {
                    Text("Host defaults")
                } footer: {
                    Text("Saved on the host and shared by every client. New conversations use them.")
                }
            }
            Section {
                Picker("Theme", selection: $theme) {
                    ForEach(ThemePreference.allCases) { Text($0.title).tag($0) }
                }
                .accessibilityIdentifier("settings.theme")
                Picker("Activity animation", selection: $animation) {
                    ForEach(ActivityAnimation.allCases) { Text($0.title).tag($0) }
                }
                .accessibilityIdentifier("settings.animation")
                Picker("Tool cards", selection: $density) {
                    ForEach(ToolDensity.allCases) { Text($0.title).tag($0) }
                }
                .accessibilityIdentifier("settings.tools")
            } header: {
                Text("Appearance")
            } footer: {
                Text("Saved on this iPhone. Reduce Motion always keeps the activity indicator still.")
            }
            Section {
                Toggle("Notifications", isOn: Binding(get: { notifications }, set: { on in
                    Task { notifications = await Attention.set(on) }
                }))
                .accessibilityIdentifier("settings.notifications")
            } footer: {
                Text("Alerts you when a reply is ready or Glass needs an answer while the app is in the background. iPhone pauses the connection soon after you leave the app, so alerts stop then.")
            }
            Section {
                Toggle("Allow HTTP on local network", isOn: $model.allowLANHTTP)
                    .accessibilityIdentifier("settings.allowHTTP")
            } footer: {
                Text("Lets you pair with hosts started with gc connect --http. Secret entry always requires HTTPS.")
            }
            Section("About") {
                LabeledContent("Version", value: model.info.versionLabel)
            }
        }
        .navigationTitle("Settings")
        .onAppear { if hostSettings == nil, model.api != nil { hostSettings = HostSettingsModel(app: model) } }
        .confirmationDialog("Forget this host?", isPresented: $confirmingForget, titleVisibility: .visible) {
            Button("Forget Host", role: .destructive) { model.forget() }
                .accessibilityIdentifier("settings.forget.confirm")
        } message: {
            Text("You’ll need a new private link from gc connect to pair again.")
        }
    }
}
