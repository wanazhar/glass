import Foundation
import GlassAPI
import GlassAuth
import GlassModels
import GlassTestSupport
import GlassTransport
import Testing

@Suite struct ThreadAPITests {
    let endpoint = try! GlassEndpoint(origin: "https://mac.tail1.ts.net", mode: .remote)

    func client(_ transport: StubTransport) throws -> GlassClient {
        let store = InMemoryCapabilityStore()
        try store.save(.ownerCapability(String(repeating: "e", count: 64)), for: endpoint)
        return GlassClient(endpoint: endpoint, store: store, transport: transport)
    }

    @Test func threadsSendSearchAndProjectQuery() async throws {
        let transport = StubTransport(json: #"{"threads":[],"offset":512}"#)
        _ = try await client(transport).threads(offset: 512, query: "fix bug", project: "/w/glass")
        let url = try #require(transport.requests.first?.url)
        let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems ?? []
        #expect(url.path == "/api/threads")
        #expect(items.contains(URLQueryItem(name: "q", value: "fix bug")))
        #expect(items.contains(URLQueryItem(name: "project", value: "/w/glass")))
        #expect(items.contains(URLQueryItem(name: "offset", value: "512")))
    }

    @Test func openPostsIdAndSourceRefWithOrigin() async throws {
        let transport = StubTransport(json: #"{"connected":true,"session":"a1","sourceRef":"/d/a1.jsonl","originLabel":"Created in GC"}"#)
        let reply = try await client(transport).open("a1", sourceRef: "/d/a1.jsonl")
        let request = try #require(transport.requests.first)
        #expect(reply.originLabel == "Created in GC")
        #expect(request.httpMethod == "POST")
        #expect(request.value(forHTTPHeaderField: "Origin") == "https://mac.tail1.ts.net")
        let body = try JSONDecoder().decode([String: String].self, from: try #require(request.httpBody))
        #expect(body == ["id": "a1", "sourceRef": "/d/a1.jsonl"])
    }

    @Test func historyRequestsCompactWindowsBeforeAnOffset() async throws {
        let transport = StubTransport(json: #"{"records":[],"before":0}"#)
        _ = try await client(transport).history("a1", before: 4096)
        let query = try #require(transport.requests.first?.url?.query)
        #expect(query.contains("before=4096") && query.contains("compact=1") && query.contains("id=a1"))
    }

    @Test func liveEventsResumeWithLastEventIDAfterReconnect() async throws {
        let transport = StubTransport(json: "{}")
        transport.enqueueStream(chunks: ["id: 5:1\ndata: {\"reset\":true,\"cursor\":1}\n\n", "id: 5:2\ndata: {\"cursor\":2,\"events\":[{\"t\":\"done\"}]}\n\n"])
        transport.enqueueStream(chunks: ["id: 5:3\ndata: {\"cursor\":3,\"events\":[]}\n\n"])
        transport.enqueueStream(status: 403, chunks: [])
        let live = LiveEvents(client: try client(transport), thread: "a1", delay: { _ in .zero })
        var updates: [LiveUpdate] = []
        for await update in live.updates(resume: EventCursor(epoch: 5, cursor: 0)) { updates.append(update) }
        let headers = transport.requests.map { $0.value(forHTTPHeaderField: "Last-Event-ID") }
        #expect(headers == ["5:0", "5:2", "5:3"])
        #expect(updates.filter { if case .frame = $0 { true } else { false } }.count == 3)
        #expect(updates.last.map { if case .unauthorized = $0 { true } else { false } } == true)
    }

    @Test func closeSurfacesActiveElsewhereErrors() async throws {
        let transport = StubTransport(status: 400, json: #"{"error":"session_active_elsewhere: busy"}"#)
        await #expect(throws: GlassError.gateway(status: 400, message: "session_active_elsewhere: busy")) {
            try await client(transport).close("a1")
        }
    }
}
