import SwiftUI

/// First run and re-pair: paste the private Connect link or scan its QR code.
struct PairingView: View {
    @Environment(AppModel.self) private var model
    @State private var link = ""
    @State private var showingScanner = false
    @FocusState private var linkFocused: Bool

    var body: some View {
        @Bindable var model = model
        ZStack {
            Backdrop()
            ScrollView {
                VStack(spacing: 28) {
                    header
                    if let reason = model.repairReason { repairBanner(reason) }
                    GlassGroup(spacing: 16) {
                        VStack(spacing: 14) {
                            linkCard
                            actions
                        }
                    }
                    lanToggle(isOn: $model.allowLANHTTP)
                }
                .padding(.horizontal, 24)
                .padding(.vertical, 40)
                .frame(maxWidth: 520)
                .frame(maxWidth: .infinity)
            }
            .scrollDismissesKeyboard(.interactively)
        }
        .sheet(isPresented: $showingScanner) {
            QRScannerSheet { scanned in
                showingScanner = false
                link = scanned
                Task { await model.pair(with: scanned) }
            }
        }
    }

    private var header: some View {
        VStack(spacing: 18) {
            Image("AppGlyph")
                .resizable()
                .scaledToFit()
                .frame(width: 88, height: 88)
                .clipShape(.rect(cornerRadius: 22, style: .continuous))
                .shadow(color: .black.opacity(0.18), radius: 16, y: 8)
                .accessibilityHidden(true)
            VStack(spacing: 8) {
                Text("Connect to your host")
                    .font(.system(.title, design: .rounded, weight: .bold))
                    .accessibilityIdentifier("pair.title")
                Text("On your Mac, run **gc connect qr**, then scan the code or paste the private link.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
        }
    }

    private func repairBanner(_ reason: String) -> some View {
        Label {
            VStack(alignment: .leading, spacing: 4) {
                Text("Pair again").font(.subheadline.weight(.semibold))
                Text(reason).font(.footnote).foregroundStyle(.secondary)
            }
        } icon: {
            Image(systemName: "key.slash").foregroundStyle(.orange)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .glassSurface(in: .rect(cornerRadius: 18, style: .continuous), tint: .orange)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("pair.repair")
    }

    private var linkCard: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text("Private link").font(.footnote.weight(.semibold)).foregroundStyle(.secondary)
                Spacer()
                PasteButton(payloadType: String.self) { strings in
                    guard let first = strings.first else { return }
                    Task { @MainActor in
                        link = first
                        await model.pair(with: first)
                    }
                }
                .labelStyle(.iconOnly)
                .buttonBorderShape(.capsule)
                .controlSize(.small)
            }
            TextField("https://your-mac.tailnet.ts.net#owner=…", text: $link, axis: .vertical)
                .font(.system(.footnote, design: .monospaced))
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
                    .accessibilityIdentifier("pair.error")
            }
        }
        .padding(16)
        .glassSurface(in: .rect(cornerRadius: 22, style: .continuous))
    }

    private var actions: some View {
        HStack(spacing: 12) {
            Button {
                linkFocused = false
                showingScanner = true
            } label: {
                Label("Scan QR", systemImage: "qrcode.viewfinder").frame(maxWidth: .infinity)
            }
            .glassButtonStyle()
            .accessibilityIdentifier("pair.scan")

            Button(action: connect) {
                Group {
                    if model.isPairing {
                        ProgressView()
                    } else {
                        Label("Connect", systemImage: "arrow.right.circle.fill")
                    }
                }
                .frame(maxWidth: .infinity)
            }
            .glassButtonStyle(prominent: true)
            .disabled(link.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || model.isPairing)
            .accessibilityIdentifier("pair.connect")
        }
        .controlSize(.large)
    }

    private func lanToggle(isOn: Binding<Bool>) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Toggle("Allow HTTP on local network", isOn: isOn)
                .font(.subheadline.weight(.medium))
                .accessibilityIdentifier("pair.allowHTTP")
            Text("Off by default. Use for hosts started with gc connect --http. Secrets are never sent over HTTP.")
                .font(.footnote)
                .foregroundStyle(.secondary)
        }
        .padding(16)
        .glassSurface(in: .rect(cornerRadius: 22, style: .continuous))
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
