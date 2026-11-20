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

    /// Lean state: control, presence, and counts without every transcript.
    func state() async throws -> MarbleState {
        try await fetch(try makeRequest(path: "/api/state?lean=1"))
    }

    func transcript(conversationId: String, after: UInt64?) async throws -> MarbleTranscriptPage {
        var path = "/api/transcript?conversationId=\(encoded(conversationId))"
        if let after, after > 0 {
            path += "&after=\(after)"
        }
        return try await fetch(try makeRequest(path: path))
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

    /// Correct the running turn without stopping and restarting it.
    func steer(text: String, conversationId: String?, taskId: String?) async throws {
        struct Body: Encodable {
            var text: String
            var conversationId: String?
            var taskId: String?
        }
        try await post(
            "/api/turns/steer",
            body: Body(text: text, conversationId: conversationId, taskId: taskId)
        )
    }

    func renameConversation(id: String, title: String) async throws {
        struct Body: Encodable { var title: String }
        try await post("/api/conversations/\(encoded(id))/rename", body: Body(title: title))
    }

    func deleteConversation(id: String) async throws {
        try await post("/api/conversations/\(encoded(id))/delete")
    }

    func cancel(taskId: String) async throws {
        try await post("/api/tasks/\(taskId)/cancel")
    }

    func decide(requestId: String, allow: Bool, remember: Bool = false) async throws {
        let encoded = requestId.addingPercentEncoding(
            withAllowedCharacters: .urlPathAllowed
        ) ?? requestId
        if allow, remember {
            struct Body: Encodable { var remember: Bool }
            try await post("/api/approvals/\(encoded)/allow", body: Body(remember: true))
        } else {
            try await post("/api/approvals/\(encoded)/\(allow ? "allow" : "deny")")
        }
    }

    // MARK: - Control layer

    func setProfile(_ profile: String) async throws {
        struct Body: Encodable { var profile: String }
        try await post("/api/control/profile", body: Body(profile: profile))
    }

    func setPaused(_ paused: Bool) async throws {
        try await post(paused ? "/api/control/pause" : "/api/control/resume")
    }

    func stopEverything() async throws {
        try await post("/api/control/stop")
    }

    func revokeGrant(_ grantId: String) async throws {
        let encoded = grantId.addingPercentEncoding(
            withAllowedCharacters: .urlPathAllowed
        ) ?? grantId
        try await post("/api/control/grants/\(encoded)/revoke")
    }

    func login(token: String) async throws {
        struct Body: Encodable { var token: String }
        try await post("/api/login", body: Body(token: token))
    }

    // MARK: - Presence, memory, routines

    func presence() async throws -> MarblePresence {
        try await fetch(try makeRequest(path: "/api/presence"))
    }

    func memory() async throws -> MarbleMemory {
        try await fetch(try makeRequest(path: "/api/memory"))
    }

    private struct MemoryReply: Decodable { var memory: MarbleMemory }
    private struct RoutinesReply: Decodable { var routines: MarbleRoutines }
    private struct OkReply: Decodable { var ok: Bool }

    private func encoded(_ value: String) -> String {
        value.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? value
    }

    func remember(text: String) async throws -> MarbleMemory {
        struct Body: Encodable { var text: String }
        let reply: MemoryReply = try await postJSON("/api/memory", body: Body(text: text))
        return reply.memory
    }

    func updateMemory(id: String, text: String, kind: String? = nil) async throws -> MarbleMemory {
        struct Body: Encodable {
            var text: String
            var kind: String?
        }
        let reply: MemoryReply = try await postJSON(
            "/api/memory/\(encoded(id))/update",
            body: Body(text: text, kind: kind)
        )
        return reply.memory
    }

    func forgetMemory(id: String) async throws -> MarbleMemory {
        let reply: MemoryReply = try await postJSON(
            "/api/memory/\(encoded(id))/forget",
            body: Optional<String>.none
        )
        return reply.memory
    }

    func acceptSuggestion(id: String) async throws -> MarbleMemory {
        let reply: MemoryReply = try await postJSON(
            "/api/memory/\(encoded(id))/accept",
            body: Optional<String>.none
        )
        return reply.memory
    }

    func dismissSuggestion(id: String) async throws -> MarbleMemory {
        let reply: MemoryReply = try await postJSON(
            "/api/memory/\(encoded(id))/dismiss",
            body: Optional<String>.none
        )
        return reply.memory
    }

    func routines() async throws -> MarbleRoutines {
        try await fetch(try makeRequest(path: "/api/routines"))
    }

    func addRoutine(
        title: String,
        prompt: String,
        schedule: String,
        conversationId: String?
    ) async throws -> MarbleRoutines {
        struct Body: Encodable {
            var title: String
            var prompt: String
            var schedule: String
            var conversationId: String?
            var offsetMinutes: Int
        }
        let reply: RoutinesReply = try await postJSON(
            "/api/routines",
            body: Body(
                title: title,
                prompt: prompt,
                schedule: schedule,
                conversationId: conversationId,
                offsetMinutes: TimeZone.current.secondsFromGMT() / 60
            )
        )
        return reply.routines
    }

    /// Tell the host which timezone `daily:HH:MM` means to this phone.
    func setRoutineOffset(minutes: Int) async throws {
        struct Body: Encodable { var offsetMinutes: Int }
        try await post("/api/routines/offset", body: Body(offsetMinutes: minutes))
    }

    func runRoutine(id: String) async throws {
        try await post("/api/routines/\(encoded(id))/run")
    }

    func setRoutine(id: String, enabled: Bool) async throws {
        try await post("/api/routines/\(encoded(id))/\(enabled ? "enable" : "disable")")
    }

    func removeRoutine(id: String) async throws {
        try await post("/api/routines/\(encoded(id))/remove")
    }

    // MARK: - Prism and tooling

    func prism() async throws -> MarblePrism {
        try await fetch(try makeRequest(path: "/api/prism"))
    }

    struct MarbleDraft: Encodable {
        var name: String
        var role: String
        var glyph: String
        var hue: Int
        var workspace: String
    }

    private struct PrismReply: Decodable { var prism: MarblePrism }

    private struct DispatchReply: Decodable {
        var dispatched: [String]
        var marbles: [String]
        var swarm: Bool
    }

    @discardableResult
    func addMarble(_ draft: MarbleDraft) async throws -> MarblePrism {
        let reply: PrismReply = try await postJSON("/api/prism/marbles", body: draft)
        return reply.prism
    }

    func removeMarble(id: String) async throws {
        let _: OkReply = try await postJSON(
            "/api/prism/marbles/\(encoded(id))/remove",
            body: Optional<String>.none
        )
    }

    /// Hand one prompt to several marbles at once. Empty `marbles` means all.
    func dispatchSwarm(text: String, marbles: [String]) async throws -> (names: [String], swarm: Bool) {
        struct Body: Encodable {
            var text: String
            var marbles: [String]?
        }
        let reply: DispatchReply = try await postJSON(
            "/api/prism/dispatch",
            body: Body(text: text, marbles: marbles.isEmpty ? nil : marbles)
        )
        return (reply.marbles, reply.swarm)
    }

    func tools() async throws -> MarbleToolInventory {
        try await fetch(try makeRequest(path: "/api/tools"))
    }

    /// The workspace crate map, read when the sheet that shows it opens.
    func crates() async throws -> MarbleCrateMap {
        try await fetch(try makeRequest(path: "/api/crates"))
    }

    // MARK: - Notifications and voice

    struct NotificationUpdate: Encodable {
        var webhookUrl: String
        var webhookToken: String
        var events: [String]
        var apns: MarbleApns?
    }

    func notificationSettings() async throws -> MarbleNotificationSettings {
        try await fetch(try makeRequest(path: "/api/notifications"))
    }

    func saveNotifications(_ update: NotificationUpdate) async throws -> MarbleNotificationSettings {
        try await postJSON("/api/notifications", body: update)
    }

    func testNotification() async throws -> Bool {
        let reply: OkReply = try await postJSON(
            "/api/notifications/test",
            body: Optional<String>.none
        )
        return reply.ok
    }

    func registerDevice(token: String, topic: String) async throws {
        struct Body: Encodable {
            var token: String
            var platform: String
            var topic: String
        }
        let _: OkReply = try await postJSON(
            "/api/devices",
            body: Body(token: token, platform: "ios", topic: topic)
        )
    }

    func voice() async throws -> MarbleVoice {
        try await fetch(try makeRequest(path: "/api/voice"))
    }

    func setVoice(enabled: Bool) async throws -> MarbleVoice {
        struct Body: Encodable { var enabled: Bool }
        return try await postJSON("/api/voice", body: Body(enabled: enabled))
    }

    func speak(text: String) async throws -> String {
        struct Body: Encodable { var text: String }
        struct Reply: Decodable { var spoken: String }
        let reply: Reply = try await postJSON("/api/voice/speak", body: Body(text: text))
        return reply.spoken
    }

    /// POST one JSON body and decode the reply.
    private func postJSON<T: Decodable>(_ path: String, body: Encodable?) async throws -> T {
        let data = try body.map { try JSONEncoder().encode($0) }
        return try await fetch(try makeRequest(path: path, method: "POST", body: data))
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
