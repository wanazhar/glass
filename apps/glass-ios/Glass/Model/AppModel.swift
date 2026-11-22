import Foundation
import GlassAPI
import GlassAuth
import GlassModels
import GlassTransport
import Observation

/// App-wide connection state for the single paired host.
@MainActor @Observable
final class AppModel {
    enum Phase: Equatable {
        case unpaired
        case connecting
        case connected(GatewayStatus)
        case unreachable(String)
    }

    static let keychainService = "com.wanazhar.glass.connect"

    private(set) var endpoint: GlassEndpoint?
    private(set) var phase: Phase = .unpaired
    private(set) var isPairing = false
    private(set) var lastRoundTrip: Duration?
    private(set) var lastRefreshed: Date?
    var pairingError: String?
    /// Shown on the pairing screen after a 403 cleared the stored capability.
    private(set) var repairReason: String?
    private(set) var secretEntryUnlocked = false

    enum Tab: Hashable { case threads, desktop, settings }
    /// The selected root tab, so commands can move between them.
    var selectedTab: Tab = .threads
    /// Submenus open under Settings, so commands can land on Diagnostics or Appearance.
    var settingsPath: [SettingsRoute] = []

    /// Opens Settings, optionally at one of its submenus.
    func show(_ route: SettingsRoute?) {
        settingsPath = route.map { [$0] } ?? []
        selectedTab = .settings
    }
    /// The host's command catalogue, loaded once per pairing.
    private(set) var commands: [CommandDescriptor] = []
    private(set) var commandsError: String?

    var allowLANHTTP: Bool {
        didSet { defaults.set(allowLANHTTP, forKey: Keys.allowLANHTTP) }
    }

    let info: AppInfo
    private let defaults: UserDefaults
    private let store: any CapabilityStore
    private let transport: any GlassHTTPTransport
    private var client: GlassClient?

    private enum Keys {
        static let endpoint = "glass.host.v1"
        static let allowLANHTTP = "glass.allowLANHTTP"
    }

    init(
        info: AppInfo = .current,
        defaults: UserDefaults = .standard,
        store: any CapabilityStore = KeychainCapabilityStore(service: AppModel.keychainService),
        transport: any GlassHTTPTransport = URLSessionTransport()
    ) {
        self.info = info
        self.defaults = defaults
        self.store = store
        self.transport = transport
        self.allowLANHTTP = defaults.bool(forKey: Keys.allowLANHTTP)
        if let data = defaults.data(forKey: Keys.endpoint),
           let saved = try? JSONDecoder().decode(GlassEndpoint.self, from: data),
           (try? store.credential(for: saved)) ?? .none != .none {
            endpoint = saved
            client = GlassClient(endpoint: saved, store: store, transport: transport)
            phase = .connecting
        }
    }

    /// The paired host's client; nil until paired.
    var api: GlassClient? { client }

    var status: GatewayStatus? { if case .connected(let status) = phase { status } else { nil } }

    func pair(with text: String) async {
        guard !isPairing else { return }
        pairingError = nil
        let link: PairingLink
        do {
            link = try PairingLink.parse(text, policy: .init(allowInsecureHTTP: allowLANHTTP), knownEndpoint: endpoint)
        } catch {
            pairingError = error.localizedDescription
            return
        }
        isPairing = true
        defer { isPairing = false }
        do {
            let start = ContinuousClock.now
            let result = try await PairingService(transport: transport, store: store).pair(link)
            lastRoundTrip = ContinuousClock.now - start
            if let previous = endpoint, previous != result.endpoint { try? store.remove(for: previous) }
            endpoint = result.endpoint
            defaults.set(try? JSONEncoder().encode(result.endpoint), forKey: Keys.endpoint)
            client = GlassClient(endpoint: result.endpoint, store: store, transport: transport, secretCapability: result.secretCapability)
            secretEntryUnlocked = result.secretCapability != nil
            repairReason = nil
            lastRefreshed = .now
            phase = .connected(result.status)
        } catch {
            pairingError = error.localizedDescription
        }
    }

    func refresh() async {
        guard let client else { return }
        if status == nil { phase = .connecting }
        do {
            let start = ContinuousClock.now
            let status = try await client.status()
            lastRoundTrip = ContinuousClock.now - start
            lastRefreshed = .now
            secretEntryUnlocked = await client.secretEntryUnlocked
            phase = .connected(status)
            KeepAlive.shared.hostName = status.workspaceName
        } catch {
            handle(error)
        }
    }

    func diagnostics() async throws -> JSONValue {
        guard let client else { throw GlassError.unauthorized("Not paired") }
        do { return try await client.diagnostics() } catch {
            handle(error)
            throw error
        }
    }

    func loadCommands(force: Bool = false) async {
        guard force || commands.isEmpty, let api = client else { return }
        commandsError = nil
        do { commands = try await api.commands() } catch {
            if error.isUnauthorized { handle(error) }
            commandsError = error.localizedDescription
        }
    }

    /// Removes the Keychain credential and returns to pairing; the host keeps its threads.
    func forget() {
        commands = []
        selectedTab = .threads
        if let endpoint { try? store.remove(for: endpoint) }
        defaults.removeObject(forKey: Keys.endpoint)
        client = nil
        secretEntryUnlocked = false
        repairReason = nil
        pairingError = nil
        phase = .unpaired
    }

    func handle(_ error: GlassError) {
        switch error {
        case .unauthorized(let message):
            client = nil
            secretEntryUnlocked = false
            repairReason = message
            phase = .unpaired
        case .cancelled:
            break
        default:
            phase = .unreachable(error.localizedDescription)
        }
    }

    /// Clears all persisted state; used by UI tests via `-GlassResetState`.
    func resetForTesting() {
        forget()
        allowLANHTTP = false
        Drafts.shared.removeAll()
    }
}

extension GlassEndpoint {
    var transportLabel: String {
        switch (mode, isSecure) {
        case (.local, _): "Local"
        case (.remote, true): looksLikeTailnet ? "Tailscale HTTPS" : "HTTPS"
        case (.remote, false): looksLikeTailnet ? "Tailnet HTTP" : "Local network HTTP"
        }
    }
}
