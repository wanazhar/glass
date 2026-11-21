import Foundation
import GlassTransport

/// Records requests and answers them from a handler; no sockets.
public final class StubTransport: GlassHTTPTransport, @unchecked Sendable {
    public typealias Handler = @Sendable (URLRequest) throws -> (Int, Data)

    private let lock = NSLock()
    private var handler: Handler
    private var recorded: [URLRequest] = []
    private var streams: [(Int, [Data])] = []

    /// Queues one scripted SSE response; each `stream` call consumes the next one.
    public func enqueueStream(status: Int = 200, chunks: [String]) {
        lock.withLock { streams.append((status, chunks.map { Data($0.utf8) })) }
    }

    public init(handler: @escaping Handler) { self.handler = handler }

    public convenience init(status: Int = 200, json: String) {
        self.init { _ in (status, Data(json.utf8)) }
    }

    public var requests: [URLRequest] { lock.withLock { recorded } }

    public func respond(with handler: @escaping Handler) { lock.withLock { self.handler = handler } }

    public func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        let handler = lock.withLock { recorded.append(request); return self.handler }
        let (status, data) = try handler(request)
        let response = HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: "HTTP/1.1", headerFields: nil)!
        return (data, response)
    }

    public func stream(_ request: URLRequest) async throws -> (AsyncThrowingStream<Data, any Error>, HTTPURLResponse) {
        let next: (Int, [Data])? = lock.withLock {
            recorded.append(request)
            return streams.isEmpty ? nil : streams.removeFirst()
        }
        guard let (status, chunks) = next else { throw URLError(.cannotConnectToHost) }
        let headers = status == 200 ? ["Content-Type": "text/event-stream"] : [:]
        let response = HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: "HTTP/1.1", headerFields: headers)!
        return (AsyncThrowingStream { continuation in
            for chunk in chunks { continuation.yield(chunk) }
            continuation.finish()
        }, response)
    }
}
