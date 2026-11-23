import Foundation
import GlassModels
import GlassTransport

public struct PairingResult: Sendable, Hashable {
    public let endpoint: GlassEndpoint
    public let credential: GlassCredential
    /// Held in memory only, like the web UI.
    public let secretCapability: String?
    public let status: GatewayStatus
}

/// Exchanges a Connect link for a stored credential, then proves it with `GET /api/status`.
public struct PairingService: Sendable {
    let transport: any GlassHTTPTransport
    let store: any CapabilityStore

    public init(transport: any GlassHTTPTransport, store: any CapabilityStore) {
        self.transport = transport
        self.store = store
    }

    struct UnlockRequest: Encodable, Sendable { let token: String }
    struct UnlockResponse: Decodable, Sendable {
        let ownerCapability: String?
        let secretCapability: String?
    }

    public func pair(_ link: PairingLink) async throws(GlassError) -> PairingResult {
        let endpoint = link.endpoint
        let credential: GlassCredential
        var secret: String?
        switch link.kind {
        case .local:
            credential = .bearer(link.token)
        case .owner, .secret:
            let unlock = try await send(
                .post("/api/local", json: UnlockRequest(token: link.token)), endpoint: endpoint, credential: .none
            )
            let body: UnlockResponse
            do { body = try JSONDecoder().decode(UnlockResponse.self, from: unlock) } catch {
                throw .decoding("pairing response")
            }
            guard let owner = body.ownerCapability, PairingLink.isHexToken(owner) else {
                throw .invalidResponse("the host did not return an owner capability")
            }
            credential = .ownerCapability(owner)
            if let candidate = body.secretCapability, endpoint.isSecure, PairingLink.isHexToken(candidate) {
                secret = candidate
            }
        }
        let statusData = try await send(.get("/api/status"), endpoint: endpoint, credential: credential)
        let status: GatewayStatus
        do { status = try JSONDecoder().decode(GatewayStatus.self, from: statusData) } catch {
            throw .decoding("status")
        }
        do { try store.save(credential, for: endpoint) } catch {
            throw .invalidResponse("could not save to the Keychain: \(error.localizedDescription)")
        }
        return PairingResult(endpoint: endpoint, credential: credential, secretCapability: secret, status: status)
    }

    private func send(_ request: GlassRequest, endpoint: GlassEndpoint, credential: GlassCredential) async throws(GlassError) -> Data {
        let urlRequest = try GlassRequestBuilder.urlRequest(for: request, endpoint: endpoint, credential: credential)
        do {
            let (data, response) = try await transport.send(urlRequest)
            return try GlassResponse.validate(data, response)
        } catch {
            throw GlassError.from(error, endpoint: endpoint)
        }
    }
}
