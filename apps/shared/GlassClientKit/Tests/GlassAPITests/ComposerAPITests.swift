import Foundation
import GlassAPI
import GlassAuth
import GlassModels
import GlassTestSupport
import GlassTransport
import Testing

@Suite struct ComposerAPITests {
    let endpoint = try! GlassEndpoint(origin: "https://mac.tail1.ts.net", mode: .remote)

    func client(_ transport: StubTransport, secret: Bool = false, endpoint: GlassEndpoint? = nil) throws -> GlassClient {
        let endpoint = endpoint ?? self.endpoint
        let store = InMemoryCapabilityStore()
        try store.save(.ownerCapability(String(repeating: "e", count: 64)), for: endpoint)
        return GlassClient(endpoint: endpoint, store: store, transport: transport,
                           secretCapability: secret ? String(repeating: "f", count: 64) : nil)
    }

    func body(_ request: URLRequest?) throws -> [String: JSONValue] {
        try JSONDecoder().decode([String: JSONValue].self, from: try #require(request?.httpBody))
    }

    @Test func sendWrapsTheMessageWithThreadAndOrigin() async throws {
        let transport = StubTransport(json: #"{"sent":true}"#)
        try await client(transport).send(.prompt(text: "hello", requestID: "r1"), thread: "a1", sourceRef: "/d/a1.jsonl")
        let request = try #require(transport.requests.first)
        #expect(request.url?.path == "/api/send")
        #expect(request.value(forHTTPHeaderField: "Origin") == "https://mac.tail1.ts.net")
        #expect(try body(request) == ["id": "a1", "sourceRef": "/d/a1.jsonl",
                                      "message": .object(["t": "prompt", "text": "hello", "request_id": "r1"])])
    }

    @Test func readOnlyRejectionSurfacesTheGatewayMessage() async throws {
        let transport = StubTransport(status: 409, json: #"{"error":"Read-only observer: claim control before sending input"}"#)
        await #expect(throws: GlassError.gateway(status: 409, message: "Read-only observer: claim control before sending input")) {
            try await client(transport).send(.cancel, thread: "a1")
        }
    }

    @Test func secretIsSentOnceWithTheSecretCapabilityAndThenCleared() async throws {
        let transport = StubTransport(json: #"{"submitted":true}"#)
        var secret = Array(#"pa"ss\word"#.utf8)
        try await client(transport, secret: true).submitSecret(&secret, thread: "a1", interactionID: "i1")
        #expect(secret.allSatisfy { $0 == 0 })
        let request = try #require(transport.requests.first)
        #expect(request.url?.path == "/api/secret")
        #expect(request.value(forHTTPHeaderField: "X-Glass-Secret-Capability") == String(repeating: "f", count: 64))
        #expect(try body(request) == ["id": "a1", "interactionId": "i1", "secret": .string(#"pa"ss\word"#)])
    }

    @Test func secretIsClearedEvenWhenTheHostRejectsIt() async throws {
        let transport = StubTransport(status: 400, json: #"{"error":"No matching secret request is pending"}"#)
        var secret = Array("hunter2".utf8)
        await #expect(throws: GlassError.self) {
            try await client(transport, secret: true).submitSecret(&secret, thread: "a1", interactionID: "i1")
        }
        #expect(secret.allSatisfy { $0 == 0 })
    }

    @Test func secretIsRefusedOverPlainHTTP() async throws {
        let lan = try GlassEndpoint(origin: "http://192.168.1.20:8787", mode: .remote)
        let transport = StubTransport(json: "{}")
        var secret = Array("hunter2".utf8)
        await #expect(throws: GlassError.insecureTransport) {
            try await client(transport, secret: true, endpoint: lan).submitSecret(&secret, thread: "a1", interactionID: "i1")
        }
        #expect(transport.requests.isEmpty)
        #expect(secret.allSatisfy { $0 == 0 })
    }

    @Test func uploadSendsRawBytesAndReportsProgress() async throws {
        let transport = StubTransport(json: #"{"token":"tok","name":"a.txt","bytes":5,"kind":"text"}"#)
        let seen = Locked<[Int64]>([])
        let receipt = try await client(transport).upload(Data("notes".utf8), name: "a.txt", thread: "a1") { sent, _ in seen.mutate { $0.append(sent) } }
        let request = try #require(transport.requests.first)
        let items = URLComponents(url: try #require(request.url), resolvingAgainstBaseURL: false)?.queryItems ?? []
        #expect(request.url?.path == "/api/upload")
        #expect(items.contains(URLQueryItem(name: "name", value: "a.txt")))
        #expect(items.contains(URLQueryItem(name: "id", value: "a1")))
        #expect(request.value(forHTTPHeaderField: "Content-Type") == "application/octet-stream")
        #expect(request.httpBody == Data("notes".utf8))
        #expect(receipt.token == "tok")
        #expect(seen.value == [5])
    }

    @Test func uploadOverEightMiBNeverLeavesTheDevice() async throws {
        let transport = StubTransport(json: "{}")
        await #expect(throws: GlassError.bodyTooLarge(limit: 8 * 1024 * 1024, actual: 8 * 1024 * 1024 + 1)) {
            _ = try await client(transport).upload(Data(count: 8 * 1024 * 1024 + 1), name: "big.png", thread: "a1")
        }
        #expect(transport.requests.isEmpty)
    }

    @Test func contextAttachmentAndInspectHitTheirRoutes() async throws {
        let transport = StubTransport { request in
            switch request.url?.path {
            case "/api/context": (200, Data(#"{"attachments":[{"path":"/u/1","source":"Uploaded a.txt · 5 bytes","bytes":5,"mediaType":"text/plain"}]}"#.utf8))
            case "/api/attachment": (200, Data([1, 2, 3]))
            default: (200, Data(#"{"body":{"tasks":[]}}"#.utf8))
            }
        }
        let client = try client(transport)
        #expect(try await client.context("a1").map(\.label) == ["a.txt"])
        #expect(try await client.attachment(String(repeating: "a", count: 64), thread: "a1") == Data([1, 2, 3]))
        #expect(try await client.inspect("todo", thread: "a1") == .object(["body": .object(["tasks": .array([])])]))
        let attachment = transport.requests[1]
        #expect(URLComponents(url: attachment.url!, resolvingAgainstBaseURL: false)?.queryItems?
            .contains(URLQueryItem(name: "attachment", value: String(repeating: "a", count: 64))) == true)
        #expect(try body(transport.requests[2]) == ["id": "a1", "name": "todo", "values": .object([:])])
    }
}

final class Locked<Value: Sendable>: @unchecked Sendable {
    private let lock = NSLock()
    private var stored: Value
    init(_ value: Value) { stored = value }
    var value: Value { lock.withLock { stored } }
    func mutate(_ change: (inout Value) -> Void) { lock.withLock { change(&stored) } }
}
