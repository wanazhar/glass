import Foundation
import GlassModels
import GlassTransport

extension GlassClient {
    /// `GET /api/browse`: a page of a directory inside the host's permitted workspace.
    public func browse(_ path: String = "", offset: Int = 0) async throws(GlassError) -> BrowsePage {
        var query = [URLQueryItem(name: "path", value: path)]
        if offset > 0 { query.append(URLQueryItem(name: "offset", value: String(offset))) }
        return try await decode(BrowsePage.self, from: .get("/api/browse", query: query))
    }

    /// `GET /api/usage`: allocated size of a directory.
    public func diskUsage(_ path: String) async throws(GlassError) -> DiskUsage {
        try await decode(DiskUsage.self, from: .get("/api/usage", query: [URLQueryItem(name: "path", value: path)]))
    }

    /// `GET /api/file`: a file from the thread workspace, or from a browsed workspace directory.
    public func file(_ path: String, thread id: String? = nil, sourceRef: String? = nil, workspace: String? = nil) async throws(GlassError) -> FileContent {
        var query = workspace.map { [URLQueryItem(name: "workspace", value: $0)] } ?? Self.threadQuery(id ?? "", sourceRef)
        query.append(URLQueryItem(name: "path", value: path))
        var request = GlassRequest.get("/api/file", query: query)
        request.timeout = 60
        return try await response(for: request)
    }

    /// `GET /api/edit`: a text file opened for editing.
    public func editableFile(_ path: String, thread id: String, sourceRef: String? = nil) async throws(GlassError) -> EditableFile {
        var query = Self.threadQuery(id, sourceRef)
        query.append(URLQueryItem(name: "path", value: path))
        return try await decode(EditableFile.self, from: .get("/api/edit", query: query))
    }

    /// `GET /api/changes`: Git changes in the thread workspace.
    public func changes(thread id: String, sourceRef: String? = nil) async throws(GlassError) -> WorkspaceChanges {
        try await decode(WorkspaceChanges.self, from: .get("/api/changes", query: Self.threadQuery(id, sourceRef)))
    }

    /// `GET /api/agents`: the thread's teammates.
    public func teammates(thread id: String, sourceRef: String? = nil) async throws(GlassError) -> [Teammate] {
        Teammate.decodeList(try await decode(JSONValue.self, from: .get("/api/agents", query: Self.threadQuery(id, sourceRef))))
    }

    /// `GET /api/agent`: a teammate's transcript, newest window first; pass `before` to page back.
    public func teammateTranscript(_ agent: String, thread id: String, sourceRef: String? = nil, before: Int? = nil) async throws(GlassError) -> TeammateTranscript {
        var query = Self.threadQuery(id, sourceRef)
        query.append(URLQueryItem(name: "agent", value: agent))
        if let before { query.append(URLQueryItem(name: "before", value: String(before))) }
        return try await decode(TeammateTranscript.self, from: .get("/api/agent", query: query))
    }

    public func agentHistory(_ agent: String, thread id: String, sourceRef: String? = nil, before: Int? = nil) async throws(GlassError) -> HistoryPage {
        var query = Self.threadQuery(id, sourceRef)
        query.append(URLQueryItem(name: "agent", value: agent))
        query.append(URLQueryItem(name: "view", value: "chat"))
        if let before { query.append(URLQueryItem(name: "before", value: String(before))) }
        return try await decode(HistoryPage.self, from: .get("/api/agent", query: query))
    }

    public func stopAgent(_ agent: String? = nil, thread id: String, sourceRef: String? = nil) async throws(GlassError) {
        var body = Self.threadBody(id, sourceRef)
        if let agent { body["agent"] = .string(agent) }
        _ = try await data(for: Self.post("/api/agent/stop", body))
    }

    public func sendToAgent(_ agent: String, text: String, thread id: String, sourceRef: String? = nil) async throws(GlassError) {
        var body = Self.threadBody(id, sourceRef)
        body["agent"] = .string(agent)
        body["text"] = .string(text)
        _ = try await data(for: Self.post("/api/agent/send", body))
    }

    /// `GET /api/activity-text`: a page of a large saved activity record.
    public func activityText(record: Int, offset: Int = 0, expected: String? = nil, thread id: String, sourceRef: String? = nil) async throws(GlassError) -> ActivityTextPage {
        var query = Self.threadQuery(id, sourceRef)
        query.append(URLQueryItem(name: "recordOffset", value: String(record)))
        query.append(URLQueryItem(name: "offset", value: String(offset)))
        if let expected { query.append(URLQueryItem(name: "expected", value: expected)) }
        return try await decode(ActivityTextPage.self, from: .get("/api/activity-text", query: query))
    }

    /// `GET /api/activity-location`: the record and turn behind a saved message.
    public func activityLocation(record: Int, thread id: String, sourceRef: String? = nil) async throws(GlassError) -> ActivityLocation {
        var query = Self.threadQuery(id, sourceRef)
        query.append(URLQueryItem(name: "recordOffset", value: String(record)))
        return try await decode(ActivityLocation.self, from: .get("/api/activity-location", query: query))
    }

    /// `GET /api/export`: the conversation as Markdown.
    public func export(thread id: String, sourceRef: String? = nil) async throws(GlassError) -> Data {
        var request = GlassRequest.get("/api/export", query: Self.threadQuery(id, sourceRef))
        request.timeout = 120
        return try await data(for: request)
    }

    /// `POST /api/artifact`: registers HTML for an isolated interactive preview.
    public func createArtifact(_ html: Data, thread id: String, sourceRef: String? = nil) async throws(GlassError) -> ArtifactReceipt {
        guard !html.isEmpty, html.count <= GlassBodyLimit.artifact else {
            throw .invalidResponse("Interactive HTML must be between 1 byte and 512 KiB")
        }
        let request = GlassRequest(method: .post, path: "/api/artifact", query: Self.threadQuery(id, sourceRef), body: html,
                                   bodyLimit: GlassBodyLimit.artifact, contentType: "text/html; charset=utf-8")
        let data = try await data(for: request)
        do { return try JSONDecoder().decode(ArtifactReceipt.self, from: data) } catch { throw .decoding("ArtifactReceipt") }
    }

    /// `GET /api/artifact?token=`: the isolated document, for a `WKURLSchemeHandler`.
    public func artifactDocument(token: String) async throws(GlassError) -> FileContent {
        try await response(for: .get("/api/artifact", query: [URLQueryItem(name: "token", value: token)]))
    }

    /// `POST /api/upgrade`: restarts the thread's runtime, keeping its history.
    public func restartRuntime(thread id: String, sourceRef: String? = nil) async throws(GlassError) -> JSONValue {
        var body = Self.threadBody(id, sourceRef)
        body["confirmed"] = .bool(true)
        var request = try Self.post("/api/upgrade", body)
        request.timeout = 30
        return try await decode(JSONValue.self, from: request)
    }

    /// `GET /api/agent-events`: roster revisions that tell the app when to refresh teammates.
    public func teammateSignals(thread id: String, sourceRef: String? = nil) async throws(GlassError) -> AsyncThrowingStream<TeammateSignal, any Error> {
        let credential = try credential()
        var request = GlassRequest.get("/api/agent-events", query: Self.threadQuery(id, sourceRef))
        request.timeout = 60
        var urlRequest = try GlassRequestBuilder.urlRequest(for: request, endpoint: endpoint, credential: credential, secretCapability: nil)
        urlRequest.setValue("text/event-stream", forHTTPHeaderField: "Accept")
        let chunks: AsyncThrowingStream<Data, any Error>
        do {
            let (stream, response) = try await transport.stream(urlRequest)
            guard response.statusCode == 200 else {
                _ = try GlassResponse.validate(Data(), response)
                throw GlassError.invalidResponse("Teammate updates are unavailable")
            }
            chunks = stream
        } catch {
            throw GlassError.from(error, endpoint: endpoint)
        }
        return AsyncThrowingStream { continuation in
            let task = Task {
                var parser = SSEParser(lastEventID: nil)
                do {
                    for try await chunk in chunks {
                        for frame in try parser.feed(chunk) {
                            guard case .object(let value)? = try? JSONDecoder().decode(JSONValue.self, from: Data(frame.data.utf8)) else { continue }
                            if value["offline"] == .bool(true) { continuation.yield(.offline); continuation.finish(); return }
                            if case .number(let revision)? = value["revision"] { continuation.yield(.revision(Int(revision))) }
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
}
