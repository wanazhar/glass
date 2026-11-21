import Foundation
import GlassAuth
import GlassModels
import GlassTransport

/// One paired gateway. A 403 clears the stored credential so the app can ask to re-pair.
public actor GlassClient {
    public nonisolated let endpoint: GlassEndpoint
    private let transport: any GlassHTTPTransport
    private let store: any CapabilityStore
    private var secretCapability: String?

    public init(endpoint: GlassEndpoint, store: any CapabilityStore, transport: any GlassHTTPTransport, secretCapability: String? = nil) {
        self.endpoint = endpoint
        self.store = store
        self.transport = transport
        self.secretCapability = endpoint.isSecure ? secretCapability : nil
    }

    public var secretEntryUnlocked: Bool { secretCapability != nil }

    public func setSecretCapability(_ value: String?) {
        secretCapability = endpoint.isSecure ? value : nil
    }

    public func data(for request: GlassRequest) async throws(GlassError) -> Data {
        let credential: GlassCredential
        do { credential = try store.credential(for: endpoint) } catch {
            throw .unauthorized("Couldn’t read this host’s pairing from the Keychain")
        }
        guard credential != .none else { throw .unauthorized("Pair this iPhone with your host to continue") }
        let urlRequest = try GlassRequestBuilder.urlRequest(
            for: request, endpoint: endpoint, credential: credential, secretCapability: secretCapability
        )
        do {
            let (data, response) = try await transport.send(urlRequest)
            return try GlassResponse.validate(data, response)
        } catch {
            let mapped = GlassError.from(error, endpoint: endpoint)
            if mapped.isUnauthorized { forgetCredentials() }
            throw mapped
        }
    }

    public func decode<T: Decodable & Sendable>(_ type: T.Type, from request: GlassRequest) async throws(GlassError) -> T {
        let data = try await data(for: request)
        do { return try JSONDecoder().decode(T.self, from: data) } catch {
            throw .decoding(String(describing: T.self))
        }
    }

    public func status() async throws(GlassError) -> GatewayStatus {
        try await decode(GatewayStatus.self, from: .get("/api/status"))
    }

    /// `GET /api/diagnostics`: the read-only `doctor` report.
    public func diagnostics() async throws(GlassError) -> JSONValue {
        try await decode(JSONValue.self, from: .get("/api/diagnostics"))
    }

    public func forgetCredentials() {
        secretCapability = nil
        try? store.remove(for: endpoint)
    }
}
