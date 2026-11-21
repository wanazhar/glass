import SwiftUI

struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @State private var confirmingForget = false

    var body: some View {
        @Bindable var model = model
        Form {
            if let endpoint = model.endpoint {
                Section {
                    LabeledContent("Host", value: endpoint.origin)
                    LabeledContent("Transport", value: endpoint.transportLabel)
                    Button("Forget This Host", role: .destructive) { confirmingForget = true }
                        .accessibilityIdentifier("settings.forget")
                } header: {
                    Text("Paired host")
                } footer: {
                    Text("The pairing is stored in this iPhone’s Keychain. Forgetting it does not affect threads on the host.")
                }
            }
            Section {
                Toggle("Allow HTTP on local network", isOn: $model.allowLANHTTP)
                    .accessibilityIdentifier("settings.allowHTTP")
            } footer: {
                Text("Lets you pair with hosts started with gc connect --http. Secret entry always requires HTTPS.")
            }
            Section("About") {
                LabeledContent("Version", value: model.info.versionLabel)
                LabeledContent("Bundle", value: model.info.bundleIdentifier)
            }
        }
        .navigationTitle("Settings")
        .confirmationDialog("Forget this host?", isPresented: $confirmingForget, titleVisibility: .visible) {
            Button("Forget Host", role: .destructive) { model.forget() }
                .accessibilityIdentifier("settings.forget.confirm")
        } message: {
            Text("You’ll need a new private link from gc connect to pair again.")
        }
    }
}
