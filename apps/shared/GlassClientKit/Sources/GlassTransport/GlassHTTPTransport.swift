import Foundation

public protocol GlassHTTPTransport: Sendable {
    func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse)
    /// Opens a streaming response (SSE). Chunks arrive as the server flushes them.
    func stream(_ request: URLRequest) async throws -> (AsyncThrowingStream<Data, any Error>, HTTPURLResponse)
}

extension GlassHTTPTransport {
    /// Sends a request body, reporting bytes sent. Transports without progress report once at the end.
    public func upload(_ request: URLRequest, progress: @escaping @Sendable (Int64, Int64) -> Void) async throws -> (Data, HTTPURLResponse) {
        let result = try await send(request)
        let size = Int64(request.httpBody?.count ?? 0)
        progress(size, size)
        return result
    }

    /// Request/response transports that never stream (test doubles, simple tools).
    public func stream(_ request: URLRequest) async throws -> (AsyncThrowingStream<Data, any Error>, HTTPURLResponse) {
        throw GlassError.invalidResponse("streaming is not supported by this transport")
    }
}

/// Ephemeral, cookie-less, cache-less session that never follows redirects
/// (a redirect could carry capability headers to another origin).
public final class URLSessionTransport: NSObject, GlassHTTPTransport, URLSessionTaskDelegate {
    private let session: URLSession

    public init(configuration: URLSessionConfiguration = .ephemeral) {
        configuration.httpCookieStorage = nil
        configuration.httpShouldSetCookies = false
        configuration.urlCache = nil
        configuration.requestCachePolicy = .reloadIgnoringLocalAndRemoteCacheData
        configuration.waitsForConnectivity = false
        configuration.timeoutIntervalForRequest = 30
        session = URLSession(configuration: configuration)
        super.init()
    }

    public func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        let (data, response) = try await session.data(for: request, delegate: self)
        guard let http = response as? HTTPURLResponse else { throw GlassError.invalidResponse("not HTTP") }
        return (data, http)
    }

    public func stream(_ request: URLRequest) async throws -> (AsyncThrowingStream<Data, any Error>, HTTPURLResponse) {
        var request = request
        request.timeoutInterval = 60
        let (bytes, response) = try await session.bytes(for: request, delegate: self)
        guard let http = response as? HTTPURLResponse else { throw GlassError.invalidResponse("not HTTP") }
        let chunks = AsyncThrowingStream<Data, any Error> { continuation in
            let task = Task {
                var chunk = Data()
                do {
                    for try await byte in bytes {
                        chunk.append(byte)
                        // Flush every line: holding a lone frame-ending newline back would stall
                        // the frame until the host sends its next bytes.
                        if byte == 0x0A {
                            continuation.yield(chunk)
                            chunk.removeAll(keepingCapacity: true)
                        }
                    }
                    if !chunk.isEmpty { continuation.yield(chunk) }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
        return (chunks, http)
    }

    public func upload(_ request: URLRequest, progress: @escaping @Sendable (Int64, Int64) -> Void) async throws -> (Data, HTTPURLResponse) {
        var request = request
        let body = request.httpBody ?? Data()
        request.httpBody = nil
        let (data, response) = try await session.upload(for: request, from: body, delegate: UploadProgressDelegate(progress))
        guard let http = response as? HTTPURLResponse else { throw GlassError.invalidResponse("not HTTP") }
        return (data, http)
    }

    public func urlSession(
        _ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
        newRequest request: URLRequest
    ) async -> URLRequest? { nil }
}

public enum GlassResponse {
    /// Maps gateway status codes to `GlassError`; returns the body on 2xx.
    public static func validate(_ data: Data, _ response: HTTPURLResponse) throws(GlassError) -> Data {
        switch response.statusCode {
        case 200..<300: return data
        case 403: throw .unauthorized(message(in: data) ?? "This iPhone is no longer paired with the host")
        case 300..<400: throw .invalidResponse("redirects are not followed")
        default: throw .gateway(status: response.statusCode, message: message(in: data) ?? "")
        }
    }

    static func message(in data: Data) -> String? {
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return nil }
        return (object["error"] as? String).flatMap { $0.isEmpty ? nil : $0 }
    }
}

/// Per-upload delegate: reports progress and, like the session delegate, never follows redirects.
private final class UploadProgressDelegate: NSObject, URLSessionTaskDelegate, Sendable {
    let progress: @Sendable (Int64, Int64) -> Void

    init(_ progress: @escaping @Sendable (Int64, Int64) -> Void) { self.progress = progress }

    func urlSession(_ session: URLSession, task: URLSessionTask, didSendBodyData bytesSent: Int64,
                    totalBytesSent: Int64, totalBytesExpectedToSend: Int64) {
        progress(totalBytesSent, totalBytesExpectedToSend)
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, willPerformHTTPRedirection response: HTTPURLResponse,
                    newRequest request: URLRequest) async -> URLRequest? { nil }
}
