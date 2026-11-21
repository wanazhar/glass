import Foundation
import GlassTransport

/// Records requests and answers them from a handler; no sockets.
public final class StubTransport: GlassHTTPTransport, @unchecked Sendable {
    public typealias Handler = @Sendable (URLRequest) throws -> (Int, Data)

    private let lock = NSLock()
    private var handler: Handler
    private var recorded: [URLRequest] = []

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
}
