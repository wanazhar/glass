import Foundation
import GlassAuth
import GlassModels
import GlassTestSupport
import GlassTransport
import Testing

let hex = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
let compact = "AbCdEfGhIjKlMnOpQr_-12"

@Suite struct PairingLinkTests {
    @Test func parsesCompactAndFullOwnerLinks() throws {
        let a = try PairingLink.parse("  https://mac.tail1.ts.net#owner=\(compact)\n")
        #expect(a.kind == .owner && a.token == compact && a.endpoint.origin == "https://mac.tail1.ts.net" && a.endpoint.mode == .remote)
        let b = try PairingLink.parse("https://mac.tail1.ts.net:8443/#owner=\(hex)")
        #expect(b.kind == .owner && b.token == hex && b.endpoint.origin == "https://mac.tail1.ts.net:8443")
    }

    @Test func parsesSecretAndLocalLinks() throws {
        #expect(try PairingLink.parse("https://mac.tail1.ts.net#secret=\(hex)").kind == .secret)
        let local = try PairingLink.parse("http://127.0.0.1:51718/#\(hex)")
        #expect(local.kind == .local && local.endpoint.mode == .local)
    }

    @Test func rejectsMalformedTokens() {
        for bad in ["https://h.ts.net#owner=\(hex.uppercased())", "https://h.ts.net#owner=short",
                    "https://h.ts.net#secret=\(compact)", "https://h.ts.net", "https://h.ts.net#\(hex)",
                    "http://192.168.1.2/#\(hex)"] {
            #expect(throws: PairingLink.ParseError.self, "\(bad)") { try PairingLink.parse(bad, policy: .init(allowInsecureHTTP: true)) }
        }
        #expect(throws: PairingLink.ParseError.empty) { try PairingLink.parse("   ") }
        #expect(throws: PairingLink.ParseError.notALink) { try PairingLink.parse("hello") }
    }

    @Test func plainHTTPRequiresTheLANToggleAndNeverCarriesSecrets() throws {
        let link = "http://192.168.1.10:8080#owner=\(compact)"
        #expect(throws: PairingLink.ParseError.insecureHTTPDisabled) { try PairingLink.parse(link) }
        #expect(try PairingLink.parse(link, policy: .init(allowInsecureHTTP: true)).endpoint.isSecure == false)
        #expect(throws: PairingLink.ParseError.secretRequiresHTTPS) {
            try PairingLink.parse("http://192.168.1.10:8080#secret=\(hex)", policy: .init(allowInsecureHTTP: true))
        }
    }

    @Test func bareCodeNeedsAKnownRemoteHost() throws {
        #expect(throws: PairingLink.ParseError.needsHost) { try PairingLink.parse(compact) }
        let known = try GlassEndpoint(origin: "https://mac.tail1.ts.net", mode: .remote)
        let link = try PairingLink.parse(compact, knownEndpoint: known)
        #expect(link.endpoint == known && link.kind == .owner)
    }

    @Test func descriptionRedactsTheToken() throws {
        #expect(!String(describing: try PairingLink.parse("https://mac.tail1.ts.net#owner=\(compact)")).contains(compact))
    }
}

@Suite struct CapabilityStoreTests {
    @Test func credentialsAreIsolatedPerOriginAndMode() throws {
        let store = InMemoryCapabilityStore()
        let a = try GlassEndpoint(origin: "https://a.ts.net", mode: .remote)
        let b = try GlassEndpoint(origin: "https://a.ts.net:8443", mode: .remote)
        let c = try GlassEndpoint(origin: "http://127.0.0.1:5000", mode: .local)
        try store.save(.ownerCapability(hex), for: a)
        try store.save(.bearer(hex), for: c)
        #expect(try store.credential(for: a) == .ownerCapability(hex))
        #expect(try store.credential(for: b) == .none)
        #expect(try store.credential(for: c) == .bearer(hex))
        try store.remove(for: a)
        #expect(try store.credential(for: a) == .none)
    }
}

@Suite struct PairingServiceTests {
    let statusJSON = #"{"workspace":"/w","remote":true,"name":"Glass","protocolVersion":1,"capabilities":["events-resume"]}"#

    @Test func ownerLinkExchangesTokenThenVerifiesStatus() async throws {
        let owner = String(repeating: "c", count: 64)
        let status = statusJSON
        let transport = StubTransport { request in
            switch request.url?.path {
            case "/api/local": return (200, Data(#"{"ownerCapability":"\#(owner)","secretCapability":null}"#.utf8))
            default: return (200, Data(status.utf8))
            }
        }
        let store = InMemoryCapabilityStore()
        let link = try PairingLink.parse("https://mac.tail1.ts.net#owner=\(compact)")
        let result = try await PairingService(transport: transport, store: store).pair(link)
        #expect(result.credential == .ownerCapability(owner))
        #expect(result.secretCapability == nil)
        #expect(result.status.compatibility == .compatible)
        #expect(try store.credential(for: link.endpoint) == .ownerCapability(owner))
        let requests = transport.requests
        #expect(requests.count == 2)
        #expect(requests[0].httpMethod == "POST")
        #expect(requests[0].value(forHTTPHeaderField: "Origin") == "https://mac.tail1.ts.net")
        let body = try JSONSerialization.jsonObject(with: requests[0].httpBody!) as? [String: String]
        #expect(body == ["token": compact])
        #expect(requests[1].value(forHTTPHeaderField: "X-Glass-Owner-Capability") == owner)
    }

    @Test func secretLinkKeepsSecretCapabilityOutOfTheStore() async throws {
        let owner = String(repeating: "c", count: 64), secret = String(repeating: "d", count: 64)
        let status = statusJSON
        let transport = StubTransport { request in
            request.url?.path == "/api/local"
                ? (200, Data(#"{"ownerCapability":"\#(owner)","secretCapability":"\#(secret)"}"#.utf8))
                : (200, Data(status.utf8))
        }
        let store = InMemoryCapabilityStore()
        let result = try await PairingService(transport: transport, store: store)
            .pair(try PairingLink.parse("https://mac.tail1.ts.net#secret=\(hex)"))
        #expect(result.secretCapability == secret)
        #expect(try store.credential(for: result.endpoint) == .ownerCapability(owner))
    }

    @Test func rejectedLinkStoresNothing() async throws {
        let transport = StubTransport(status: 403, json: #"{"error":"Invalid owner capability"}"#)
        let store = InMemoryCapabilityStore()
        let link = try PairingLink.parse("https://mac.tail1.ts.net#owner=\(compact)")
        await #expect(throws: GlassError.unauthorized("Invalid owner capability")) {
            try await PairingService(transport: transport, store: store).pair(link)
        }
        #expect(try store.credential(for: link.endpoint) == .none)
    }

    @Test func malformedCapabilityIsRejected() async throws {
        let transport = StubTransport(json: #"{"ownerCapability":"not-hex"}"#)
        let link = try PairingLink.parse("https://mac.tail1.ts.net#owner=\(compact)")
        await #expect(throws: GlassError.self) {
            try await PairingService(transport: transport, store: InMemoryCapabilityStore()).pair(link)
        }
    }

    @Test func localLinkUsesBearerAndSkipsUnlock() async throws {
        let transport = StubTransport(json: #"{"workspace":"/w","remote":false}"#)
        let result = try await PairingService(transport: transport, store: InMemoryCapabilityStore())
            .pair(try PairingLink.parse("http://127.0.0.1:5000/#\(hex)"))
        #expect(result.credential == .bearer(hex))
        #expect(result.status.compatibility == .legacyGateway)
        #expect(transport.requests.map(\.url?.path) == ["/api/status"])
    }
}
