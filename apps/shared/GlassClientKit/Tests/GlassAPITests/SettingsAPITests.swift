import Foundation
import GlassAPI
import GlassAuth
import GlassModels
import GlassTestSupport
import GlassTransport
import Testing

@Suite struct SettingsAPITests {
    static let snapshot = #"{"revision":"r1","defaults":{"provider":"openai","model":"m","max_tokens":1000}}"#

    func client(_ transport: StubTransport, origin: String = "https://mac.tail1.ts.net", mode: GlassEndpoint.Mode = .remote) throws -> GlassClient {
        let endpoint = try GlassEndpoint(origin: origin, mode: mode)
        let store = InMemoryCapabilityStore()
        try store.save(.ownerCapability(String(repeating: "e", count: 64)), for: endpoint)
        return GlassClient(endpoint: endpoint, store: store, transport: transport)
    }

    @Test func commandsLoadsTheCatalogue() async throws {
        let transport = StubTransport(json: #"{"commands":[{"name":"usage","readonly":true,"portable":true,"group":"work"}]}"#)
        let commands = try await client(transport).commands()
        #expect(commands.map(\.name) == ["usage"])
        #expect(transport.requests.first?.url?.path == "/api/commands")
    }

    @Test func settingsSendsTheRevisionAndOrigin() async throws {
        let transport = StubTransport(json: Self.snapshot)
        let snapshot = try await client(transport).updateSettings(.preference(key: "image_input", value: true), revision: "r0")
        #expect(snapshot.revision == "r1")
        let request = try #require(transport.requests.first)
        #expect(request.url?.path == "/api/settings" && request.httpMethod == "POST")
        #expect(request.value(forHTTPHeaderField: "Origin") == "https://mac.tail1.ts.net")
        let body = try JSONDecoder().decode(JSONValue.self, from: try #require(request.httpBody))
        #expect(body == .object(["revision": "r0", "action": "preference", "key": "image_input", "value": true]))
    }

    @Test func credentialChangesAreRefusedOverHTTP() async throws {
        let transport = StubTransport(json: Self.snapshot)
        let http = try client(transport, origin: "http://192.168.1.4:8080", mode: .remote)
        await #expect(throws: GlassError.insecureTransport) {
            try await http.updateSettings(.addConnection(label: "", provider: "openai", endpoint: "https://e", model: "m", secret: "k"), revision: "r")
        }
        #expect(transport.requests.isEmpty)
        _ = try await http.updateSettings(.useConnection(id: "c1"), revision: "r")
        #expect(transport.requests.count == 1)
    }

    @Test func conflictSurfacesTheHostMessage() async throws {
        let transport = StubTransport(status: 409, json: #"{"error":"Settings changed elsewhere. Refresh before saving your changes."}"#)
        do {
            _ = try await client(transport).updateSettings(.useConnection(id: "c1"), revision: "old")
            Issue.record("expected conflict")
        } catch {
            #expect(SettingsChange.isConflict(error.localizedDescription))
        }
    }
}
