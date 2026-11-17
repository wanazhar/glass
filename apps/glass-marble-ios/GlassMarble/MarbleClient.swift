import Foundation
import Security

/// One self-hosted Marble host, usually reached over Tailscale.
struct MarbleEndpoint: Equatable {
    var baseURL: URL
    var token: String?

    static let fallback = MarbleEndpoint(
        baseURL: URL(string: "http://127.0.0.1:4783")!,
        token: nil
    )

    var displayHost: String { baseURL.absoluteString }
}

enum MarbleClientError: LocalizedError, Equatable {
    case invalidURL
    case unauthorized
    case http(Int, String)
    case offline(String)

    var errorDescription: String? {
        switch self {
        case .invalidURL:
            return "That host address is not a valid URL."
        case .unauthorized:
            return "This host needs its access token."
        case .http(let status, let body):
            return "Host rejected the request (\(status)): \(body)"
        case .offline(let message):
            return message.isEmpty ? "Cannot reach the host." : message
        }
    }
}

/// HTTP and server-sent-event access to a Marble host.
final class MarbleClient: @unchecked Sendable {
    let endpoint: MarbleEndpoint
    private let session: URLSession
    private let decoder = JSONDecoder()

    init(endpoint: MarbleEndpoint) {
        self.endpoint = endpoint
        let configuration = URLSessionConfiguration.default
        configuration.waitsForConnectivity = true
        configuration.timeoutIntervalForRequest = 60
        configuration.timeoutIntervalForResource = 60 * 60 * 4
        configuration.httpShouldSetCookies = true
        configuration.httpCookieAcceptPolicy = .always
        configuration.requestCachePolicy = .reloadIgnoringLocalCacheData
        session = URLSession(configuration: configuration)
    }

    // MARK: - Requests

    private func makeRequest(
        path: String,
        query: [URLQueryItem] = [],
        method: String = "GET",
        body: Data? = nil,
        acceptsEventStream: Bool = false
    ) throws -> URLRequest {
        guard var components = URLComponents(
            url: endpoint.baseURL,
            resolvingAgainstBaseURL: false
        ) else {
            throw MarbleClientError.invalidURL
        }
        components.path = path
        if !query.isEmpty { components.queryItems = query }
        guard let url = components.url else { throw MarbleClientError.invalidURL }
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.setValue(
            acceptsEventStream ? "text/event-stream" : "application/json",
            forHTTPHeaderField: "Accept"
        )
        if let token = endpoint.token, !token.isEmpty {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        if let body {
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
            request.httpBody = body
        }
        return request
    }

    private func validate(_ response: URLResponse, data: Data) throws {
        guard let http = response as? HTTPURLResponse else {
            throw MarbleClientError.offline("The host did not answer over HTTP.")
        }
        if http.statusCode == 401 { throw MarbleClientError.unauthorized }
        guard (200..<300).contains(http.statusCode) else {
            let body = String(data: data, encoding: .utf8) ?? ""
            throw MarbleClientError.http(http.statusCode, body)
        }
    }

    private func fetch<T: Decodable>(_ request: URLRequest) async throws -> T {
        do {
            let (data, response) = try await session.data(for: request)
            try validate(response, data: data)
            return try decoder.decode(T.self, from: data)
        } catch let error as MarbleClientError {
            throw error
        } catch let error as DecodingError {
            throw MarbleClientError.offline("Unexpected host response: \(error)")
        } catch {
            throw MarbleClientError.offline(error.localizedDescription)
        }
    }

    private func post(_ path: String, body: Encodable? = nil) async throws {
        let data = try body.map { try JSONEncoder().encode($0) }
        let request = try makeRequest(path: path, method: "POST", body: data)
        do {
            let (responseData, response) = try await session.data(for: request)
            try validate(response, data: responseData)
        } catch let error as MarbleClientError {
            throw error
        } catch {
            throw MarbleClientError.offline(error.localizedDescription)
        }
    }

    // MARK: - Product API

    func health() async throws -> MarbleHealth {
        try await fetch(try makeRequest(path: "/healthz"))
    }

    func state() async throws -> MarbleState {
        try await fetch(try makeRequest(path: "/api/state"))
    }

    func send(text: String, conversationId: String?) async throws -> MarbleAccepted {
        struct Body: Encodable {
            var text: String
            var conversationId: String?
        }
        let data = try JSONEncoder().encode(
            Body(text: text, conversationId: conversationId)
        )
        return try await fetch(
            try makeRequest(path: "/api/turns", method: "POST", body: data)
        )
    }

    func cancel(taskId: String) async throws {
        try await post("/api/tasks/\(taskId)/cancel")
    }

    func decide(requestId: String, allow: Bool) async throws {
        let encoded = requestId.addingPercentEncoding(
            withAllowedCharacters: .urlPathAllowed
        ) ?? requestId
        try await post("/api/approvals/\(encoded)/\(allow ? "allow" : "deny")")
    }

    func login(token: String) async throws {
        struct Body: Encodable { var token: String }
        try await post("/api/login", body: Body(token: token))
    }

    // MARK: - Events

    /// Long-lived server-sent events starting after `since`. The caller owns
    /// reconnection; the stream ends when the host closes it.
    func events(since: UInt64) -> AsyncThrowingStream<MarbleEvent, Error> {
        AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    let request = try makeRequest(
                        path: "/api/events",
                        query: [URLQueryItem(name: "since", value: String(since))],
                        acceptsEventStream: true
                    )
                    let (bytes, response) = try await session.bytes(for: request)
                    guard let http = response as? HTTPURLResponse else {
                        throw MarbleClientError.offline("The host closed the stream.")
                    }
                    if http.statusCode == 401 { throw MarbleClientError.unauthorized }
                    guard http.statusCode == 200 else {
                        throw MarbleClientError.http(http.statusCode, "stream rejected")
                    }
                    for try await line in bytes.lines {
                        guard line.hasPrefix("data:") else { continue }
                        let payload = line.dropFirst("data:".count)
                            .trimmingCharacters(in: .whitespaces)
                        guard
                            let data = payload.data(using: .utf8),
                            let event = try? decoder.decode(MarbleEvent.self, from: data)
                        else { continue }
                        continuation.yield(event)
                    }
                    continuation.finish()
                } catch {
                    if Task.isCancelled {
                        continuation.finish()
                    } else {
                        continuation.finish(throwing: error)
                    }
                }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }
}

/// Product preferences: host address in defaults, token in the keychain.
enum MarblePreferences {
    private static let baseURLKey = "marble.baseURL"
    private static let tokenAccount = "marble.token"
    private static let keychainService = "com.wanazhar.glassmarble"

    static var baseURLString: String {
        get { UserDefaults.standard.string(forKey: baseURLKey) ?? MarbleEndpoint.fallback.baseURL.absoluteString }
        set { UserDefaults.standard.set(newValue, forKey: baseURLKey) }
    }

    static var baseURL: URL {
        URL(string: baseURLString) ?? MarbleEndpoint.fallback.baseURL
    }

    static var token: String? {
        get { readKeychain(account: tokenAccount) }
        set {
            if let newValue, !newValue.isEmpty {
                writeKeychain(newValue, account: tokenAccount)
            } else {
                deleteKeychain(account: tokenAccount)
            }
        }
    }

    static func save(_ endpoint: MarbleEndpoint) {
        baseURLString = endpoint.baseURL.absoluteString
        token = endpoint.token
    }

    private static func readKeychain(account: String) -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: keychainService,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var item: CFTypeRef?
        guard
            SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess,
            let data = item as? Data
        else { return nil }
        return String(data: data, encoding: .utf8)
    }

    private static func writeKeychain(_ value: String, account: String) {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: keychainService,
            kSecAttrAccount as String: account,
        ]
        SecItemDelete(query as CFDictionary)
        var attributes = query
        attributes[kSecValueData as String] = Data(value.utf8)
        attributes[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlock
        SecItemAdd(attributes as CFDictionary, nil)
    }

    private static func deleteKeychain(account: String) {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: keychainService,
            kSecAttrAccount as String: account,
        ]
        SecItemDelete(query as CFDictionary)
    }
}
