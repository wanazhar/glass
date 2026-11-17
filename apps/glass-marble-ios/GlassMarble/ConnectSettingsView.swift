import SwiftUI

struct ConnectSettingsView: View {
    @EnvironmentObject private var store: MarbleStore
    @Environment(\.dismiss) private var dismiss
    @State private var address = ""
    @State private var token = ""
    @State private var note: String?

    var body: some View {
        NavigationStack {
            Form {
                Section("Host") {
                    TextField("http://100.x.y.z:4783", text: $address)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)
                    SecureField("Access token", text: $token)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                }

                Section {
                    Button("Connect") { connect() }
                    if let note {
                        Text(note)
                            .font(.footnote)
                            .foregroundStyle(MarblePalette.muted)
                    }
                } footer: {
                    Text("On the host run `glass-marble serve --host <tailscale-ip> --token-file ~/.marble-token`, then use the address printed at startup. The token is stored in the iOS keychain.")
                }

                Section("This device") {
                    LabeledContent("Status", value: store.statusLabel)
                    LabeledContent("State revision", value: String(store.state.revision))
                    LabeledContent("Pending approvals", value: String(store.state.approvals.count))
                }
            }
            .scrollContentBackground(.hidden)
            .background(MarbleBackdrop())
            .navigationTitle("Host")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .onAppear {
            address = store.endpoint.baseURL.absoluteString
            token = store.endpoint.token ?? ""
            note = nil
        }
    }

    private func connect() {
        guard
            let url = URL(string: address.trimmingCharacters(in: .whitespacesAndNewlines)),
            url.scheme == "http" || url.scheme == "https",
            url.host != nil
        else {
            note = "Enter a full http or https URL, for example http://100.64.0.5:4783."
            return
        }
        let trimmedToken = token.trimmingCharacters(in: .whitespacesAndNewlines)
        store.apply(
            MarbleEndpoint(baseURL: url, token: trimmedToken.isEmpty ? nil : trimmedToken)
        )
        note = "Connecting to \(url.absoluteString)…"
    }
}
