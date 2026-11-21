import Foundation

public protocol GlassHTTPTransport: Sendable {
    func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse)
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
