import Foundation
import GlassModels
import GlassTransport

extension GlassClient {
    /// `GET /api/desktop/status`.
    public func desktopStatus() async throws(GlassError) -> DesktopStatus {
        try await decode(DesktopStatus.self, from: .get("/api/desktop/status"))
    }

    /// `POST /api/desktop/session` with `start`; nil starts the host's default desktop.
    public func startDesktop(_ option: DesktopOption? = nil) async throws(GlassError) {
        var body: [String: JSONValue] = ["action": .string("start")]
        if let option {
            body["environment"] = .string(option.environment)
            body["lifecycle"] = .string(option.lifecycle)
        }
        var request = try GlassRequest.post("/api/desktop/session", json: JSONValue.object(body))
        request.timeout = 75
        _ = try await data(for: request)
    }

    /// `POST /api/desktop/session` with `stop`.
    public func stopDesktop() async throws(GlassError) {
        var request = try GlassRequest.post("/api/desktop/session", json: JSONValue.object(["action": .string("stop")]))
        request.timeout = 30
        _ = try await data(for: request)
    }

    /// `POST /api/desktop/profile`: deletes the saved reusable desktop.
    public func deleteDesktopProfile() async throws(GlassError) {
        _ = try await data(for: try GlassRequest.post("/api/desktop/profile", json: JSONValue.object([
            "action": .string("delete"), "profileId": .string("default"),
        ])))
    }

    /// `POST /api/desktop/lease`: takes or returns mouse and keyboard control.
    public func setDesktopLease(_ action: DesktopLeaseAction, session: String?) async throws(GlassError) -> String? {
        var body: [String: JSONValue] = ["action": .string(action.rawValue)]
        if let session { body["sessionId"] = .string(session) }
        let result = try await decode(JSONValue.self, from: try GlassRequest.post("/api/desktop/lease", json: JSONValue.object(body)))
        return result["lease"]?.stringValue
    }

    /// `POST /api/desktop/input`.
    public func sendDesktopInput(_ input: DesktopInput) async throws(GlassError) {
        var request = try GlassRequest.post("/api/desktop/input", json: input.body)
        request.timeout = 10
        _ = try await data(for: request)
    }

    /// `GET /api/desktop/frame`: one JPEG, for when the live stream can't connect. Nil while no frame is ready.
    public func desktopFrame() async throws(GlassError) -> DesktopFrame? {
        let content = try await response(for: .get("/api/desktop/frame"))
        guard content.contentType.hasPrefix("image/"), !content.data.isEmpty else { return nil }
        return DesktopFrame(jpeg: content.data, sequence: 0)
    }

    /// `GET /api/desktop/stream`: live JPEG frames, scaled by the host to `width` (320…1280).
    public func desktopStream(width: Int = 1280) async throws(GlassError) -> AsyncThrowingStream<DesktopFrame, any Error> {
        let credential = try credential()
        var request = GlassRequest.get("/api/desktop/stream", query: [URLQueryItem(name: "width", value: String(min(max(width, 320), 1280)))])
        request.timeout = 30
        let urlRequest = try GlassRequestBuilder.urlRequest(for: request, endpoint: endpoint, credential: credential, secretCapability: nil)
        let chunks: AsyncThrowingStream<Data, any Error>
        do {
            let (stream, response) = try await transport.stream(urlRequest)
            guard response.statusCode == 200 else {
                _ = try GlassResponse.validate(Data(), response)
                throw GlassError.invalidResponse("The live desktop view is unavailable")
            }
            chunks = stream
        } catch {
            throw GlassError.from(error, endpoint: endpoint)
        }
        return AsyncThrowingStream { continuation in
            let task = Task {
                var parser = DesktopStreamParser()
                do {
                    for try await chunk in chunks {
                        for frame in try parser.feed(chunk) { continuation.yield(frame) }
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }

    /// `POST /api/desktop/updates`: waits up to about 10 s; nil when nothing changed.
    public func desktopUpdates(session: String, after: Int) async throws(GlassError) -> DesktopUpdate? {
        var request = try GlassRequest.post("/api/desktop/updates", json: JSONValue.object([
            "sessionId": .string(session), "after": .number(Double(after)),
        ]))
        request.timeout = 15
        let body = try await data(for: request)
        guard !body.isEmpty else { return nil }
        do {
            return try JSONDecoder().decode(DesktopUpdate.self, from: body)
        } catch {
            throw GlassError.invalidResponse("The desktop sent an update Glass can’t read")
        }
    }
}
