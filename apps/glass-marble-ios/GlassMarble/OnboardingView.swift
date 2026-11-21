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
                Section("Host") {
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
                    Text("The address of the machine running glass-marble — usually its Tailscale address. The token lives in the token file on the host.")
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
                    }
                }
            }
            .navigationTitle("Connect to Marble")
            .navigationBarTitleDisplayMode(.inline)
        }
        .onAppear {
            if let saved = store.endpoint {
                if address.isEmpty { address = saved.baseURL.absoluteString }
                if token.isEmpty, let savedToken = saved.token { token = savedToken }
            }
        }
    }

    /// healthz → semver → lean state, all before anything is saved.
    private func connect() async {
        let rawAddress = address
        let rawToken = token.trimmingCharacters(in: .whitespacesAndNewlines)
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
            endpoint.token = token.isEmpty ? nil : token
            let client = MarbleClient(endpoint: endpoint)
            let healthResult: Swift.Result<MarbleHealth, MarbleClientError>
            var stateOutcome = MarbleOnboardingEvaluator.StateOutcome.failed("state check failed")
            do {
                healthResult = .success(try await client.health())
                do {
                    _ = try await client.state()
                    stateOutcome = .ok
                } catch MarbleClientError.unauthorized {
                    stateOutcome = .unauthorized
                } catch {
                    stateOutcome = .failed(error.localizedDescription)
                }
            } catch let error as MarbleClientError {
                healthResult = .failure(error)
            } catch {
                healthResult = .failure(.from(error))
            }
            let verdict = MarbleOnboardingEvaluator.evaluate(
                urlTried: endpoint.baseURL.absoluteString,
                health: healthResult,
                state: stateOutcome,
                hadToken: endpoint.token != nil
            )
            return (verdict, endpoint)
        }
    }
}
