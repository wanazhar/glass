import Foundation
import GlassAuth
import GlassModels
import GlassTransport
import Testing
@testable import Glass

private final class ScriptedTransport: GlassHTTPTransport, @unchecked Sendable {
    let lock = NSLock()
    var handler: @Sendable (URLRequest) -> (Int, String)
    init(_ handler: @escaping @Sendable (URLRequest) -> (Int, String)) { self.handler = handler }
    func set(_ handler: @escaping @Sendable (URLRequest) -> (Int, String)) { lock.withLock { self.handler = handler } }
    func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        let (status, body) = lock.withLock { handler }(request)
        return (Data(body.utf8), HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: nil, headerFields: nil)!)
    }
}

@MainActor @Suite struct AppModelTests {
    let defaults: UserDefaults = {
        let name = "glass.tests.\(UUID().uuidString)"
        return UserDefaults(suiteName: name)!
    }()
    let store = InMemoryCapabilityStore()
    let owner = String(repeating: "c", count: 64)
    nonisolated static let link = "https://mac.tail1.ts.net#owner=AbCdEfGhIjKlMnOpQr_-12"
    nonisolated static let status = #"{"workspace":"/Users/o/glass","remote":true,"protocolVersion":1,"capabilities":["events-resume"]}"#

    private func happyTransport() -> ScriptedTransport {
        let owner = owner
        return ScriptedTransport { request in
            request.url?.path == "/api/local" ? (200, #"{"ownerCapability":"\#(owner)"}"#) : (200, Self.status)
        }
    }

    @Test func pairingPersistsHostAndConnects() async throws {
        let model = AppModel(defaults: defaults, store: store, transport: happyTransport())
        await model.pair(with: Self.link)
        #expect(model.pairingError == nil)
        #expect(model.status?.workspaceName == "glass")
        #expect(model.endpoint?.origin == "https://mac.tail1.ts.net")
        #expect(try store.credential(for: model.endpoint!) == .ownerCapability(owner))

        let relaunched = AppModel(defaults: defaults, store: store, transport: happyTransport())
        #expect(relaunched.endpoint == model.endpoint)
        #expect(relaunched.phase == .connecting)
    }

    @Test func revokedCapabilityReturnsToPairingWithReason() async throws {
        let transport = happyTransport()
        let model = AppModel(defaults: defaults, store: store, transport: transport)
        await model.pair(with: Self.link)
        transport.set { _ in (403, #"{"error":"Open the private Connect owner link","ownerAuthRequired":true}"#) }
        await model.refresh()
        #expect(model.phase == .unpaired)
        #expect(model.repairReason == "Open the private Connect owner link")
        #expect(try store.credential(for: model.endpoint!) == .none)
    }

    @Test func forgettingOrRevokingTheHostClosesHeldConnections() async throws {
        let transport = happyTransport()
        let model = AppModel(defaults: defaults, store: store, transport: transport)
        await model.pair(with: Self.link)
        var stopped: [String] = []
        KeepAlive.shared.hold("conversation:test") { stopped.append("conversation") }
        model.forget()
        #expect(stopped == ["conversation"])
        #expect(!KeepAlive.shared.isConnected)

        await model.pair(with: Self.link)
        KeepAlive.shared.hold("desktop") { stopped.append("desktop") }
        transport.set { _ in (403, #"{"error":"Open the private Connect owner link","ownerAuthRequired":true}"#) }
        await model.refresh()
        #expect(stopped == ["conversation", "desktop"])
        #expect(!KeepAlive.shared.isConnected)
    }

    @Test func unreachableHostKeepsThePairing() async throws {
        let transport = happyTransport()
        let model = AppModel(defaults: defaults, store: store, transport: transport)
        await model.pair(with: Self.link)
        transport.set { _ in (502, #"{"error":"bad gateway"}"#) }
        await model.refresh()
        #expect(model.phase == .unreachable("bad gateway"))
        #expect(try store.credential(for: model.endpoint!) != .none)
    }

    @Test func lanToggleGatesHTTPLinksAndPersists() async {
        let model = AppModel(defaults: defaults, store: store, transport: happyTransport())
        await model.pair(with: "http://192.168.1.10:8080#owner=AbCdEfGhIjKlMnOpQr_-12")
        #expect(model.pairingError?.contains("HTTP") == true)
        model.allowLANHTTP = true
        await model.pair(with: "http://192.168.1.10:8080#owner=AbCdEfGhIjKlMnOpQr_-12")
        #expect(model.pairingError == nil)
        #expect(model.endpoint?.isSecure == false)
        #expect(AppModel(defaults: defaults, store: store, transport: happyTransport()).allowLANHTTP)
    }

    @Test func forgetClearsKeychainAndHost() async throws {
        let model = AppModel(defaults: defaults, store: store, transport: happyTransport())
        await model.pair(with: Self.link)
        let endpoint = try #require(model.endpoint)
        model.forget()
        #expect(model.phase == .unpaired)
        #expect(try store.credential(for: endpoint) == .none)
        #expect(AppModel(defaults: defaults, store: store, transport: happyTransport()).endpoint == nil)
    }

    @Test func titleCasingForDiagnosticsKeys() {
        #expect("bridgeCount".titleCased == "Bridge Count")
        #expect("schema_version".titleCased == "Schema Version")
        #expect(JSONValue.number(12_222_717_952).displayString == "12222717952")
    }
}
