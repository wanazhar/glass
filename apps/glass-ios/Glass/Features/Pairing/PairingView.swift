import SwiftUI

/// First run and re-pair: paste the private Connect link or scan its QR code.
struct PairingView: View {
    @Environment(AppModel.self) private var model
    @State private var link = ""
    @State private var showingScanner = false
    @FocusState private var linkFocused: Bool

    var body: some View {
        @Bindable var model = model
        NavigationStack {
            Form {
                if let reason = model.repairReason {
                    Section {
                        Label {
                            VStack(alignment: .leading, spacing: 2) {
                                Text("Pair again")
                                Text(reason).font(.footnote).foregroundStyle(.secondary)
                            }
                        } icon: {
                            Image(systemName: "key.slash").foregroundStyle(.orange)
                        }
                        .accessibilityElement(children: .combine)
                        .accessibilityIdentifier("pair.repair")
                    }
                }

                Section {
                    TextField("https://host.tailnet.ts.net#owner=…", text: $link, axis: .vertical)
                        .font(.system(.body, design: .monospaced))
                        .lineLimit(1...4)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)
                        .submitLabel(.go)
                        .focused($linkFocused)
                        .onSubmit { connect() }
                        .accessibilityIdentifier("pair.link")
                    if let error = model.pairingError {
                        Label(error, systemImage: "exclamationmark.triangle.fill")
                            .font(.footnote)
                            .foregroundStyle(.red)
                            .accessibilityElement(children: .combine)
                            .accessibilityIdentifier("pair.error")
                    }
                } header: {
                    Text("Private link")
                } footer: {
                    Text("On your host, run gc connect qr. Scan the code it shows, or paste its private link.")
                        .accessibilityIdentifier("pair.title")
                }

                Section {
                    Button {
                        linkFocused = false
                        showingScanner = true
                    } label: {
                        Label("Scan QR Code", systemImage: "qrcode.viewfinder")
                    }
                    .accessibilityIdentifier("pair.scan")
                    PasteButton(payloadType: String.self) { strings in
                        guard let first = strings.first else { return }
                        Task { @MainActor in
                            link = first
                            await model.pair(with: first)
                        }
                    }
                    .buttonStyle(.borderless)
                }

                Section {
                    Toggle("Allow HTTP on local network", isOn: $model.allowLANHTTP)
                        .accessibilityIdentifier("pair.allowHTTP")
                } footer: {
                    Text("Off by default. Use for hosts started with gc connect --http. Glass never sends secrets over HTTP.")
                }
            }
            .scrollDismissesKeyboard(.interactively)
            .navigationTitle("Connect to Host")
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    if model.isPairing {
                        ProgressView().accessibilityIdentifier("pair.progress")
                    } else {
                        Button("Connect", action: connect)
                            .disabled(link.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                            .accessibilityIdentifier("pair.connect")
                    }
                }
            }
        }
        .sheet(isPresented: $showingScanner) {
            QRScannerSheet { scanned in
                showingScanner = false
                link = scanned
                Task { await model.pair(with: scanned) }
            }
        }
    }

    private func connect() {
        linkFocused = false
        let text = link
        Task { await model.pair(with: text) }
    }
}

#Preview {
    PairingView().environment(AppModel())
}
