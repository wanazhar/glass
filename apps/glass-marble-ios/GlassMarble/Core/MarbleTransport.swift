import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif

/// One self-hosted Marble host, usually reached over Tailscale. There is no
/// default host: the app asks for one on first launch.
struct MarbleEndpoint: Equatable {
    var baseURL: URL
    var token: String?

    var displayHost: String { baseURL.absoluteString }

    enum ParseError: Error, Equatable {
        case empty
        case badScheme
        case missingHost
        case malformed
    }

    /// Normalise what a person typed: trim, add `http://` when no scheme is
    /// given, require http(s) and a host, and drop any trailing slash so paths
    /// join cleanly.
    static func parse(_ input: String) -> Result<MarbleEndpoint, ParseError> {
        var text = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return .failure(.empty) }
        if !text.contains("://") {
            text = "http://" + text
        }
        guard var components = URLComponents(string: text) else {
            return .failure(.malformed)
        }
        guard let scheme = components.scheme?.lowercased(),
              scheme == "http" || scheme == "https" else {
            return .failure(.badScheme)
        }
        guard let host = components.host, !host.isEmpty else {
            return .failure(.missingHost)
        }
        while components.path.hasSuffix("/"), components.path.count > 1 {
            components.path.removeLast()
        }
        if components.path == "/" { components.path = "" }
        guard let url = components.url else { return .failure(.malformed) }
        return .success(MarbleEndpoint(baseURL: url, token: nil))
    }

    /// The same endpoint on another scheme (used to retry an https:// probe
    /// over http:// when the host turns out to serve plain HTTP).
    func with(scheme: String) -> MarbleEndpoint? {
        guard var components = URLComponents(
            url: baseURL,
            resolvingAgainstBaseURL: false
        ) else { return nil }
        components.scheme = scheme
        guard let url = components.url else { return nil }
        var copy = self
        copy.baseURL = url
        return copy
    }

    /// Build the URL for one API path and its query items, joining onto any
    /// path the base URL already carries and percent-encoding the pieces.
    /// A `?` embedded in `path` is split out and its items merged into the
    /// query, so a route string can never be encoded into `%3F`.
    func url(path: String, query: [URLQueryItem] = []) -> URL? {
        var path = path
        var query = query
        if let question = path.firstIndex(of: "?") {
            let embedded = String(path[path.index(after: question)...])
            path = String(path[..<question])
            let items = URLComponents(string: "/?\(embedded)")?.queryItems ?? []
            query = items + query
        }
        guard var components = URLComponents(
            url: baseURL,
            resolvingAgainstBaseURL: false
        ) else { return nil }
        // Callers percent-encode identifiers before building a route, so the
        // path is joined as already encoded; setting `path` would encode the
        // `%` again and the host would see `%253A` instead of `:`.
        var allowed = CharacterSet.urlPathAllowed
        allowed.insert("%")
        let leaf = path.hasPrefix("/") ? path : "/" + path
        let encodedLeaf = leaf.addingPercentEncoding(withAllowedCharacters: allowed) ?? leaf
        components.percentEncodedPath = components.percentEncodedPath + encodedLeaf
        if !query.isEmpty {
            components.queryItems = query
        }
        return components.url
    }

    /// One request with the product's headers: bearer auth when a token is
    /// set, JSON by default, `text/event-stream` for the events route.
    func request(
        path: String,
        query: [URLQueryItem] = [],
        method: String = "GET",
        body: Data? = nil,
        acceptsEventStream: Bool = false
    ) throws -> URLRequest {
        guard let url = url(path: path, query: query) else {
            throw MarbleClientError.invalidURL
        }
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.setValue(
            acceptsEventStream ? "text/event-stream" : "application/json",
            forHTTPHeaderField: "Accept"
        )
        if let token, !token.isEmpty {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        if let body {
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = body
        }
        return request
    }
}

/// Every GET route the app uses, defined once so the client and the live
/// tests can never disagree about a path or put a query string inside one.
struct MarbleRoute: Equatable {
    var path: String
    var query: [URLQueryItem] = []

    static let health = MarbleRoute(path: "/healthz")
    static let state = MarbleRoute(
        path: "/api/state",
        query: [URLQueryItem(name: "lean", value: "1")]
    )
    static let presence = MarbleRoute(path: "/api/presence")
    static let memory = MarbleRoute(path: "/api/memory")
    static let routines = MarbleRoute(path: "/api/routines")
    static let prism = MarbleRoute(path: "/api/prism")

    /// The shared chat's timeline, paged by `after` like a transcript page.
    static func everyone(after: UInt64?, main: Bool = false) -> MarbleRoute {
        var query: [URLQueryItem] = []
        if let after { query.append(URLQueryItem(name: "after", value: "\(after)")) }
        if main { query.append(URLQueryItem(name: "main", value: "1")) }
        return MarbleRoute(path: "/api/everyone", query: query)
    }
    static let tools = MarbleRoute(path: "/api/tools")
    static let crates = MarbleRoute(path: "/api/crates")
    static let voice = MarbleRoute(path: "/api/voice")
    static let control = MarbleRoute(path: "/api/control")

    static func transcript(
        conversationId: String, after: UInt64? = nil, main: Bool = false, around: String? = nil
    ) -> MarbleRoute {
        var query = [URLQueryItem(name: "conversationId", value: conversationId)]
        if let around {
            query.append(URLQueryItem(name: "around", value: around))
        } else if let after, after > 0 {
            query.append(URLQueryItem(name: "after", value: String(after)))
        }
        if main { query.append(URLQueryItem(name: "main", value: "1")) }
        return MarbleRoute(path: "/api/transcript", query: query)
    }

    static func threads(conversationId: String) -> MarbleRoute {
        MarbleRoute(path: "/api/threads", query: [.init(name: "conversation_id", value: conversationId)])
    }

    static func messageSearch(_ query: String, threadId: String?, limit: Int) -> MarbleRoute {
        var items = [URLQueryItem(name: "q", value: query), URLQueryItem(name: "limit", value: String(limit))]
        if let threadId { items.append(.init(name: "thread_id", value: threadId)) }
        return MarbleRoute(path: "/api/search", query: items)
    }

    static func search(_ q: String, limit: Int = 30) -> MarbleRoute {
        MarbleRoute(
            path: "/api/transcript/search",
            query: [
                URLQueryItem(name: "q", value: q),
                URLQueryItem(name: "limit", value: String(limit)),
            ]
        )
    }

    static func export(conversationId: String? = nil) -> MarbleRoute {
        var query: [URLQueryItem] = []
        if let conversationId, !conversationId.isEmpty {
            query.append(URLQueryItem(name: "conversationId", value: conversationId))
        }
        return MarbleRoute(path: "/api/export", query: query)
    }

    static func tlsProof(nonce: String) -> MarbleRoute {
        MarbleRoute(path: "/api/tls-proof", query: [
            URLQueryItem(name: "nonce", value: nonce),
        ])
    }

    static func events(since: UInt64) -> MarbleRoute {
        MarbleRoute(
            path: "/api/events",
            query: [URLQueryItem(name: "since", value: String(since))]
        )
    }
}

/// What a person pastes is not always what the token file contains: strip
/// surrounding quotes and a leading `Bearer `, then remove every whitespace
/// character anywhere so a copied multi-line value still works.
enum MarbleToken {
    static func normalize(_ input: String) -> String {
        var text = input.trimmingCharacters(in: .whitespacesAndNewlines)
        while text.hasPrefix("\"") || text.hasPrefix("'") {
            text = String(text.dropFirst())
                .trimmingCharacters(in: .whitespacesAndNewlines)
        }
        while text.hasSuffix("\"") || text.hasSuffix("'") {
            text = String(text.dropLast())
                .trimmingCharacters(in: .whitespacesAndNewlines)
        }
        if text.prefix(6).lowercased() == "bearer",
           text.count == 6 || text.dropFirst(6).first?.isWhitespace == true {
            text = String(text.dropFirst(6))
        }
        text.removeAll { $0.isWhitespace }
        return text
    }
}

enum MarbleClientError: LocalizedError, Equatable {
    case invalidURL
    case unreachable(String)
    case timeout
    case offline(String)
    case unauthorized
    case forbidden
    case notFound
    case conflict(String)
    case server(Int, String)
    case decoding(String)
    case http(Int, String)
    /// iOS refused the load before any bytes went out (-1022).
    case appTransportSecurity
    /// The connection died in the TLS handshake — often a plain-HTTP host
    /// probed with an https:// address.
    case tls(String)

    var errorDescription: String? {
        switch self {
        case .invalidURL:
            return "That host address is not a valid URL."
        case .unreachable(let detail):
            return detail.isEmpty ? "Cannot reach the host." : detail
        case .timeout:
            return "The host took too long to answer."
        case .offline(let message):
            return message.isEmpty ? "Cannot reach the host." : message
        case .unauthorized:
            return "This host needs its access token."
        case .forbidden:
            return "The host refused that request (403)."
        case .notFound:
            return "The host does not know that route."
        case .conflict(let code):
            return "The host could not do that now (\(code))."
        case .server(let status, _):
            return "The host failed with an internal error (\(status))."
        case .decoding(let detail):
            return "Unexpected host response: \(detail)"
        case .http(let status, let body):
            return "Host rejected the request (\(status)): \(body)"
        case .appTransportSecurity:
            return "iOS blocked plain HTTP to this host (App Transport Security)."
        case .tls(let detail):
            return detail.isEmpty ? "The TLS handshake failed." : detail
        }
    }

    /// Map a URL error to a reachable case so surfaces say "cannot reach"
    /// instead of echoing a transport code.
    static func from(_ error: Error) -> MarbleClientError {
        if let clientError = error as? MarbleClientError { return clientError }
        if let urlError = error as? URLError {
            switch urlError.code {
            case .appTransportSecurityRequiresSecureConnection:
                return .appTransportSecurity
            case .secureConnectionFailed, .serverCertificateHasBadDate,
                 .serverCertificateUntrusted, .serverCertificateHasUnknownRoot,
                 .serverCertificateNotYetValid, .clientCertificateRejected,
                 .clientCertificateRequired:
                return .tls(urlError.localizedDescription)
            case .timedOut:
                return .timeout
            case .cannotFindHost, .cannotConnectToHost, .networkConnectionLost,
                 .dnsLookupFailed, .notConnectedToInternet, .dataNotAllowed,
                 .internationalRoamingOff:
                return .unreachable(urlError.localizedDescription)
            default:
                return .offline(urlError.localizedDescription)
            }
        }
        if let error = error as? DecodingError {
            return .decoding(String(describing: error))
        }
        return .offline(error.localizedDescription)
    }

    /// Map a host status code and body to the case a surface should show.
    static func status(_ statusCode: Int, body: String) -> MarbleClientError {
        switch statusCode {
        case 401: return .unauthorized
        case 403: return .forbidden
        case 404: return .notFound
        case 409: return .conflict(Self.errorCode(from: body))
        case 500..<600: return .server(statusCode, body)
        default: return .http(statusCode, body)
        }
    }

    /// Pull the host's `error` code out of a JSON body when it has one.
    static func errorCode(from body: String) -> String {
        guard
            let data = body.data(using: .utf8),
            let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let code = object["error"] as? String
        else { return "conflict" }
        return code
    }
}

/// Validate a completed HTTP response; throws the mapped error.
func marbleValidate(_ response: URLResponse, data: Data) throws {
    guard let http = response as? HTTPURLResponse else {
        throw MarbleClientError.offline("The host did not answer over HTTP.")
    }
    guard (200..<300).contains(http.statusCode) else {
        let body = String(data: data, encoding: .utf8) ?? ""
        throw MarbleClientError.status(http.statusCode, body: body)
    }
}

/// Validate only the status line, for streaming responses that have no body yet.
func marbleValidateStatus(_ response: URLResponse) throws {
    guard let http = response as? HTTPURLResponse else {
        throw MarbleClientError.offline("The host did not answer over HTTP.")
    }
    guard (200..<300).contains(http.statusCode) else {
        throw MarbleClientError.status(http.statusCode, body: "")
    }
}
