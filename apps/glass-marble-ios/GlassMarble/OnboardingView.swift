import SwiftUI

/// Full-screen connect flow shown whenever there is no saved host (or the saved
/// token is rejected). There is no default host: Marble stays out of the
/// product UI until a real host answers and validates.
struct MarbleOnboardingView: View {
    @EnvironmentObject private var store: MarbleStore
    @State private var address = ""
    @State private var token = ""
    @State private var checking = false
    @State private var verdict: MarbleOnboarding?
    /// Set when the saved token was rejected so the form highlights the field.
    var expiredToken = false

    var body: some View {
        NavigationStack {
            Form {
                Section(header: Text("Host")) {
                    TextField("http://100.x.y.z:4783", text: $address)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)
                }

                Section {
                    SecureField(
                        expiredToken ? "Access token (the last one was rejected)" : "Access token",
                        text: $token
                    )
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                } footer: {
                    Text("The address of the machine running glass-marble — its Tailscale address like http://100.x.y.z:4783, or https://<name>.ts.net:4783 when the host has a certificate. The token lives in the token file on the host.")
                }

                Section {
                    Button {
                        Task { await connect() }
                    } label: {
                        HStack {
                            Text(checking ? "Checking…" : "Connect")
                            if checking { ProgressView() }
                        }
                    }
                    .disabled(
                        checking
                            || address.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                    )
                    if let verdict, !verdict.isReady {
                        Text(verdict.message)
                            .font(.footnote)
                            .foregroundStyle(.red)
                            .fixedSize(horizontal: false, vertical: true)
                        if let httpURL = verdict.httpHint {
                            Button("Use \(httpURL)") {
                                address = httpURL
                            }
                        }
                    }
                }
            }
            .navigationTitle("Connect to Marble")
            .navigationBarTitleDisplayMode(.inline)
        }
        .onAppear {
            // The host is prefilled; the token never is. A saved token that
            // was already rejected must not sit in the field and corrupt the
            // next paste.
            if let saved = store.endpoint, address.isEmpty {
                address = saved.baseURL.absoluteString
            }
        }
    }

    /// healthz → semver → lean state, all before anything is saved.
    private func connect() async {
        let rawAddress = address
        let rawToken = MarbleToken.normalize(token)
        checking = true
        defer { checking = false }
        let (verdict, endpoint) = await MarbleProbe.check(host: rawAddress, token: rawToken)
        self.verdict = verdict
        if verdict.isReady, let endpoint {
            store.apply(endpoint)
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
