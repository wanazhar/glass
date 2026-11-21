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

    // MARK: Threads and history

    public func threads(offset: Int = 0, query: String = "", project: String = "") async throws(GlassError) -> ThreadPage {
        var items = [URLQueryItem(name: "offset", value: String(offset))]
        if !query.isEmpty { items.append(URLQueryItem(name: "q", value: query)) }
        if !project.isEmpty { items.append(URLQueryItem(name: "project", value: project)) }
        return try await decode(ThreadPage.self, from: .get("/api/threads", query: items))
    }

    public func newThread(workspace: String = "") async throws(GlassError) -> NewThread {
        try await decode(NewThread.self, from: try Self.post("/api/new", ["workspace": .string(workspace)]))
    }

    public func open(_ id: String, sourceRef: String? = nil) async throws(GlassError) -> ThreadConnection {
        try await decode(ThreadConnection.self, from: try Self.post("/api/open", Self.threadBody(id, sourceRef)))
    }

    public func close(_ id: String, sourceRef: String? = nil) async throws(GlassError) -> ThreadConnection {
        try await decode(ThreadConnection.self, from: try Self.post("/api/close", Self.threadBody(id, sourceRef)))
    }

    /// One 512 KiB history window ending at `before` (nil = newest).
    public func history(_ id: String, sourceRef: String? = nil, before: Int? = nil, compact: Bool = true) async throws(GlassError) -> HistoryPage {
        var items = Self.threadQuery(id, sourceRef)
        if let before { items.append(URLQueryItem(name: "before", value: String(before))) }
        if compact { items.append(URLQueryItem(name: "compact", value: "1")) }
        return try await decode(HistoryPage.self, from: .get("/api/history", query: items))
    }

    /// Opens one `/api/events` connection, sending `Last-Event-ID` when resuming.
    public func eventConnection(_ id: String, sourceRef: String? = nil, resume: EventCursor?) async throws(GlassError) -> AsyncThrowingStream<(EventCursor?, EventFrame), any Error> {
        let credential: GlassCredential
        do { credential = try store.credential(for: endpoint) } catch {
            throw .unauthorized("Couldn’t read this host’s pairing from the Keychain")
        }
        guard credential != .none else { throw .unauthorized("Pair this iPhone with your host to continue") }
        var request = GlassRequest.get("/api/events", query: Self.threadQuery(id, sourceRef))
        request.timeout = 60
        var urlRequest = try GlassRequestBuilder.urlRequest(for: request, endpoint: endpoint, credential: credential, secretCapability: nil)
        urlRequest.setValue("text/event-stream", forHTTPHeaderField: "Accept")
        if let resume { urlRequest.setValue(resume.description, forHTTPHeaderField: "Last-Event-ID") }
        let chunks: AsyncThrowingStream<Data, any Error>
        do {
            let (stream, response) = try await transport.stream(urlRequest)
            guard response.statusCode == 200,
                  response.value(forHTTPHeaderField: "Content-Type")?.hasPrefix("text/event-stream") == true else {
                _ = try GlassResponse.validate(Data(), response)
                throw GlassError.invalidResponse("Workspace event connection unavailable")
            }
            chunks = stream
        } catch {
            let mapped = GlassError.from(error, endpoint: endpoint)
            if mapped.isUnauthorized { forgetCredentials() }
            throw mapped
        }
        return AsyncThrowingStream { continuation in
            let task = Task {
                var parser = SSEParser(lastEventID: resume?.description)
                do {
                    for try await chunk in chunks {
                        for frame in try parser.feed(chunk) {
                            let decoded = try JSONDecoder().decode(EventFrame.self, from: Data(frame.data.utf8))
                            continuation.yield((frame.id.flatMap(EventCursor.init), decoded))
                        }
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    private static func threadQuery(_ id: String, _ sourceRef: String?) -> [URLQueryItem] {
        var items = [URLQueryItem(name: "id", value: id)]
        if let sourceRef, !sourceRef.isEmpty { items.append(URLQueryItem(name: "sourceRef", value: sourceRef)) }
        return items
    }

    private static func threadBody(_ id: String, _ sourceRef: String?) -> [String: JSONValue] {
        var body: [String: JSONValue] = ["id": .string(id)]
        if let sourceRef, !sourceRef.isEmpty { body["sourceRef"] = .string(sourceRef) }
        return body
    }

    private static func post(_ path: String, _ body: [String: JSONValue]) throws(GlassError) -> GlassRequest {
        try GlassRequest.post(path, json: JSONValue.object(body))
    }

    public func forgetCredentials() {
        secretCapability = nil
        try? store.remove(for: endpoint)
    }
}
