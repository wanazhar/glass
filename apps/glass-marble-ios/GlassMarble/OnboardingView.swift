import SwiftUI

/// Full-screen connect flow shown whenever there is no saved host (or the saved
/// token is rejected). There is no default host: Marble stays out of the
/// product UI until a real host answers and validates.
struct MarbleOnboardingView: View {
    private enum Field { case host, token }

    @EnvironmentObject private var store: MarbleStore
    @FocusState private var focusedField: Field?
    @State private var address = ""
    @State private var token = ""
    @State private var checkTask: Task<Void, Never>?
    @State private var verdict: MarbleOnboarding?
    @State private var showsForm = false
    /// Set when the saved token was rejected so the form highlights the field.
    var expiredToken = false
    var onConnected: () -> Void = {}

    private var canConnect: Bool {
        !address.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !MarbleToken.normalize(token).isEmpty
    }

    var body: some View {
        NavigationStack {
            if !showsForm && !expiredToken {
                ScrollView {
                    VStack(spacing: 16) {
                        PocketPair().frame(width: 86, height: 86)
                        Text("Not connected").font(.largeTitle.weight(.semibold))
                        Text("Connect to your Marble host to see your conversations and memories.")
                            .foregroundStyle(MarbleTheme.muted)
                        Button("Connect to host") { showsForm = true }
                            .buttonStyle(.glassProminent)
                    }
                    .multilineTextAlignment(.center)
                    .padding(28)
                    .frame(maxWidth: 500)
                    .frame(maxWidth: .infinity)
                }
                .background(MarbleTheme.paper)
                .navigationTitle("Marble")
                .navigationBarTitleDisplayMode(.inline)
            } else {
                pairingForm
            }
        }
        .onDisappear { checkTask?.cancel() }
        .onAppear {
            // The host is prefilled; the token never is. A saved token that
            // was already rejected must not sit in the field and corrupt the
            // next paste.
            if let saved = store.endpoint, address.isEmpty {
                address = saved.baseURL.absoluteString
            }
        }
    }

    private var pairingForm: some View {
        Form {
            Section {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Host").font(.headline)
                    TextField("Host URL", text: $address, prompt: Text(verbatim: "http://100.x.y.z:4783"))
                        .focused($focusedField, equals: .host)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)
                    pairingError(for: .host)
                }
                .padding(.vertical, 6)
                VStack(alignment: .leading, spacing: 8) {
                    Text("Access token").font(.headline)
                    SecureField("Paste your token", text: $token)
                        .accessibilityLabel("Access token")
                        .focused($focusedField, equals: .token)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    pairingError(for: .token)
                    if expiredToken, verdict == nil, MarbleToken.normalize(token).isEmpty {
                        Label("Access token expired", systemImage: "key.slash")
                            .font(.footnote)
                            .foregroundStyle(MarbleTheme.problem)
                        Text("Your host is still saved. Enter a new token to reconnect.")
                            .font(.footnote)
                            .foregroundStyle(MarbleTheme.muted)
                    }
                }
                .padding(.vertical, 6)
            } footer: {
                Text("Use your host’s Tailscale URL and the access token from that host.")
                    .foregroundStyle(MarbleTheme.muted)
            }
            .listRowBackground(MarbleTheme.card)
            .disabled(checkTask != nil)

            Section {
                if checkTask != nil {
                    VStack(spacing: 12) {
                        ProgressView("Connecting to your host…")
                        Button("Cancel") {
                            checkTask?.cancel()
                            checkTask = nil
                        }
                    }
                    .frame(maxWidth: .infinity)
                } else {
                    Button {
                        focusedField = nil
                        verdict = nil
                        checkTask = Task { await connect() }
                    } label: {
                        Text("Connect")
                            .font(.headline)
                            .foregroundStyle(canConnect ? MarbleTheme.paper : MarbleTheme.muted)
                            .frame(maxWidth: .infinity, minHeight: 44)
                    }
                    .buttonStyle(.glassProminent)
                    .disabled(!canConnect)
                }
            }
            .listRowBackground(Color.clear)
            .listRowInsets(EdgeInsets())
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .background(MarbleTheme.paper)
        .navigationTitle(expiredToken ? "Replace access token" : "Pair your host")
        .navigationBarTitleDisplayMode(.large)
        .onChange(of: address) { verdict = nil }
        .onChange(of: token) {
            if errorField == .token { verdict = nil }
        }
    }

    private var errorField: Field? {
        switch verdict {
        case nil, .ready: nil
        case .badToken, .tokenRequired: .token
        default: .host
        }
    }

    @ViewBuilder
    private func pairingError(for field: Field) -> some View {
        if errorField == field, let verdict {
            Label(verdict.message, systemImage: "exclamationmark.circle")
                .font(.footnote)
                .foregroundStyle(MarbleTheme.problem)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityIdentifier(field == .host ? "pairing.hostError" : "pairing.tokenError")
            if field == .host, let httpURL = verdict.httpHint {
                Button("Use \(httpURL)") { address = httpURL }
                    .buttonStyle(.borderless)
            }
        }
    }

    /// healthz → semver → lean state, all before anything is saved.
    private func connect() async {
        let rawAddress = address
        let rawToken = MarbleToken.normalize(token)
        let (verdict, endpoint) = await MarbleProbe.check(host: rawAddress, token: rawToken)
        guard !Task.isCancelled else { return }
        checkTask = nil
        self.verdict = verdict
        if verdict.isReady, let endpoint {
            store.apply(endpoint)
            onConnected()
        }
    }
}

/// Standalone health/token probe used by onboarding and "Change host".
enum MarbleProbe {
    /// The verdict to show, and the endpoint to save when the verdict is ready.
    static func check(host: String, token: String) async -> (MarbleOnboarding, MarbleEndpoint?) {
        switch MarbleEndpoint.parse(host) {
        case .failure(let reason):
            let hint = switch reason {
            case .empty: "Enter the host address first."
            case .badScheme: "Only http or https addresses work — for example http://100.64.0.5:4783."
            case .missingHost, .malformed:
                "That does not look like a host address — for example http://100.64.0.5:4783."
            }
            return (.unreachable(hint), nil)
        case .success(var endpoint):
            let normalized = MarbleToken.normalize(token)
            endpoint.token = normalized.isEmpty ? nil : normalized
            // https pairs the certificate against the token before any other
            // traffic: a self-signed host earns its pin without a CA.
            if endpoint.baseURL.scheme == "https" {
                do {
                    _ = try await MarblePinning.pin(endpoint)
                } catch MarblePinningError.verificationFailed {
                    return (
                        .hostAnswered(
                            "The certificate at this address could not be verified with your token."
                        ), nil
                    )
                } catch {
                    // A failed proof is not fatal — the certificate may still
                    // be system-trusted (e.g. a real ts.net cert), so fall
                    // through to the ordinary probe.
                }
            }
            // Same plumbing the Linux live tests exercise. The closures take
            // the endpoint because the probe may retry over http://.
            return await MarbleOnboardingEvaluator.probe(
                endpoint: endpoint,
                health: { try await MarbleClient(endpoint: $0).health() },
                state: { try await MarbleClient(endpoint: $0).state() }
            )
        }
    }
}
