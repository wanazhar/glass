import Foundation
import GlassAPI
import GlassAuth
import GlassModels
import GlassTestSupport
import GlassTransport
import Testing

@Suite struct GlassClientTests {
    let endpoint = try! GlassEndpoint(origin: "https://mac.tail1.ts.net", mode: .remote)
    let owner = String(repeating: "e", count: 64)

    @Test func statusDecodesProtocolMetadata() async throws {
        let store = InMemoryCapabilityStore()
        try store.save(.ownerCapability(owner), for: endpoint)
        let transport = StubTransport(json: #"{"workspace":"/w/glass","remote":true,"protocolVersion":2,"capabilities":["desktop"]}"#)
        let status = try await GlassClient(endpoint: endpoint, store: store, transport: transport).status()
        #expect(status.workspaceName == "glass")
        #expect(status.supports("desktop"))
        #expect(status.compatibility == .appUpdateRequired(gatewayVersion: 2))
    }

    @Test func forbiddenClearsTheStoredCapability() async throws {
        let store = InMemoryCapabilityStore()
        try store.save(.ownerCapability(owner), for: endpoint)
        let transport = StubTransport(status: 403, json: #"{"error":"Open the private Connect owner link","ownerAuthRequired":true}"#)
        let client = GlassClient(endpoint: endpoint, store: store, transport: transport, secretCapability: owner)
        await #expect(throws: GlassError.unauthorized("Open the private Connect owner link")) { try await client.status() }
        #expect(try store.credential(for: endpoint) == .none)
        #expect(await client.secretEntryUnlocked == false)
    }

    @Test func serverErrorsKeepTheCapability() async throws {
        let store = InMemoryCapabilityStore()
        try store.save(.ownerCapability(owner), for: endpoint)
        let client = GlassClient(endpoint: endpoint, store: store, transport: StubTransport(status: 500, json: #"{"error":"boom"}"#))
        await #expect(throws: GlassError.gateway(status: 500, message: "boom")) { try await client.diagnostics() }
        #expect(try store.credential(for: endpoint) == .ownerCapability(owner))
    }

    @Test func unpairedClientNeverHitsTheNetwork() async throws {
        let transport = StubTransport(json: "{}")
        let client = GlassClient(endpoint: endpoint, store: InMemoryCapabilityStore(), transport: transport)
        await #expect(throws: GlassError.self) { try await client.status() }
        #expect(transport.requests.isEmpty)
    }

    @Test func secretCapabilityIsDroppedOnPlainHTTP() async throws {
        let lan = try GlassEndpoint(origin: "http://192.168.1.2:8080", mode: .remote)
        let client = GlassClient(endpoint: lan, store: InMemoryCapabilityStore(), transport: StubTransport(json: "{}"), secretCapability: owner)
        #expect(await client.secretEntryUnlocked == false)
    }
}
