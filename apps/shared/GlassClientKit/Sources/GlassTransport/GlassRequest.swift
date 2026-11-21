import Foundation

/// Gateway request-body ceilings; checked before anything leaves the device.
public enum GlassBodyLimit {
    public static let json = 64 * 1024
    public static let upload = 8 * 1024 * 1024
    public static let artifact = 512 * 1024
}

public enum GlassCredential: Sendable, Hashable, Codable {
    case none
    /// Origin-bound 64-hex capability from `POST /api/local` (remote mode).
    case ownerCapability(String)
    /// Raw Connect token (local loopback mode).
    case bearer(String)

    public var isEmpty: Bool { self == .none }
}

public struct GlassRequest: Sendable, Hashable {
    public enum Method: String, Sendable { case get = "GET", post = "POST" }

    public var method: Method
    public var path: String
    public var query: [URLQueryItem]
    public var body: Data?
    public var bodyLimit: Int
    /// Sends the in-memory secret capability; refused on plain HTTP.
    public var carriesSecret: Bool
    public var timeout: TimeInterval

    public init(
        method: Method, path: String, query: [URLQueryItem] = [], body: Data? = nil,
        bodyLimit: Int = GlassBodyLimit.json, carriesSecret: Bool = false, timeout: TimeInterval = 30
    ) {
        self.method = method
        self.path = path
        self.query = query
        self.body = body
        self.bodyLimit = bodyLimit
        self.carriesSecret = carriesSecret
        self.timeout = timeout
    }

    public static func get(_ path: String, query: [URLQueryItem] = []) -> GlassRequest {
        GlassRequest(method: .get, path: path, query: query)
    }

    public static func post(
        _ path: String, json: some Encodable & Sendable, limit: Int = GlassBodyLimit.json, carriesSecret: Bool = false
    ) throws(GlassError) -> GlassRequest {
        let data: Data
        do { data = try JSONEncoder().encode(json) } catch { throw .decoding(error.localizedDescription) }
        return GlassRequest(method: .post, path: path, body: data, bodyLimit: limit, carriesSecret: carriesSecret)
    }
}

public enum GlassHeader {
    public static let ownerCapability = "X-Glass-Owner-Capability"
    public static let secretCapability = "X-Glass-Secret-Capability"
}

public enum GlassRequestBuilder {
    /// Builds the `URLRequest` the gateway's origin, content-type, and capability checks expect.
    public static func urlRequest(
        for request: GlassRequest, endpoint: GlassEndpoint, credential: GlassCredential, secretCapability: String? = nil
    ) throws(GlassError) -> URLRequest {
        guard request.path.hasPrefix("/"), !request.path.hasPrefix("//") else {
            throw .invalidEndpoint("Request path must be absolute")
        }
        let size = request.body?.count ?? 0
        guard size <= request.bodyLimit else { throw .bodyTooLarge(limit: request.bodyLimit, actual: size) }
        if request.carriesSecret {
            guard endpoint.isSecure else { throw .insecureTransport }
            guard secretCapability != nil else {
                throw .unauthorized("Open the private secret-entry link from the host terminal to unlock secret entry")
            }
        }
        var components = URLComponents(string: endpoint.origin + request.path)
        if !request.query.isEmpty { components?.queryItems = request.query }
        guard let url = components?.url else { throw .invalidEndpoint("Invalid request path") }

        var urlRequest = URLRequest(url: url, cachePolicy: .reloadIgnoringLocalAndRemoteCacheData, timeoutInterval: request.timeout)
        urlRequest.httpMethod = request.method.rawValue
        urlRequest.httpShouldHandleCookies = false
        urlRequest.setValue("application/json", forHTTPHeaderField: "Accept")
        if request.method == .post {
            urlRequest.setValue(endpoint.origin, forHTTPHeaderField: "Origin")
            urlRequest.setValue("application/json", forHTTPHeaderField: "Content-Type")
            urlRequest.httpBody = request.body ?? Data("{}".utf8)
        }
        switch credential {
        case .none: break
        case .ownerCapability(let value): urlRequest.setValue(value, forHTTPHeaderField: GlassHeader.ownerCapability)
        case .bearer(let token): urlRequest.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        if request.carriesSecret, let secretCapability {
            urlRequest.setValue(secretCapability, forHTTPHeaderField: GlassHeader.secretCapability)
        }
        return urlRequest
    }
}
