import Foundation
import Security

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
        let delegate: URLSessionDelegate? =
            endpoint.baseURL.scheme == "https"
                ? MarblePinnedSessionDelegate() : nil
        session = URLSession(
            configuration: configuration,
            delegate: delegate,
            delegateQueue: nil
        )
    }

    // MARK: - Requests

    private func makeRequest(
        path: String,
        query: [URLQueryItem] = [],
        method: String = "GET",
        body: Data? = nil,
        acceptsEventStream: Bool = false
    ) throws -> URLRequest {
        try endpoint.request(
            path: path,
            query: query,
            method: method,
            body: body,
            acceptsEventStream: acceptsEventStream
        )
    }

    private func makeRequest(
        route: MarbleRoute,
        method: String = "GET",
        body: Data? = nil,
        acceptsEventStream: Bool = false
    ) throws -> URLRequest {
        try makeRequest(
            path: route.path,
            query: route.query,
            method: method,
            body: body,
            acceptsEventStream: acceptsEventStream
        )
    }

    /// A renewed certificate fails the pin check once; re-pair against the
    /// token and retry the request before giving up.
    private func withRepin<T>(_ work: () async throws -> T) async throws -> T {
        do {
            return try await work()
        } catch let error as MarbleClientError {
            guard case .tls = error, endpoint.baseURL.scheme == "https",
                  endpoint.token != nil
            else { throw error }
            _ = try await MarblePinning.pin(endpoint)
            return try await work()
        }
    }

    private func fetch<T: Decodable>(_ request: URLRequest) async throws -> T {
        try await withRepin {
            do {
                let (data, response) = try await session.data(for: request)
                try marbleValidate(response, data: data)
                return try decoder.decode(T.self, from: data)
            } catch let error as MarbleClientError {
                throw error
            } catch {
                throw MarbleClientError.from(error)
            }
        }
    }

    /// Raw bytes for endpoints that return a whole document (export).
    func fetchData(_ request: URLRequest) async throws -> Data {
        try await withRepin {
            do {
                let (data, response) = try await session.data(for: request)
                try marbleValidate(response, data: data)
                return data
            } catch let error as MarbleClientError {
                throw error
            } catch {
                throw MarbleClientError.from(error)
            }
        }
    }

    private func post(_ path: String, body: Encodable? = nil) async throws {
        let data = try body.map { try JSONEncoder().encode($0) }
        let request = try makeRequest(path: path, method: "POST", body: data)
        do {
            let (responseData, response) = try await session.data(for: request)
            try marbleValidate(response, data: responseData)
        } catch let error as MarbleClientError {
            throw error
        } catch {
            throw MarbleClientError.from(error)
        }
    }

    // MARK: - Product API

    func health() async throws -> MarbleHealth {
        // healthz answers 503 with the same JSON body when the engine is down;
        // onboarding needs that body to say so instead of "unreachable". Every
        // failure maps through MarbleClientError so the evaluator can tell a
        // refused request from a silent one.
        do {
            let (data, response) = try await session.data(
                for: try makeRequest(route: .health)
            )
            let status = (response as? HTTPURLResponse)?.statusCode ?? 0
            if status == 503 {
                return try decoder.decode(MarbleHealth.self, from: data)
            }
            try marbleValidate(response, data: data)
            return try decoder.decode(MarbleHealth.self, from: data)
        } catch let error as MarbleClientError {
            throw error
        } catch {
            throw MarbleClientError.from(error)
        }
    }

    /// Lean state: control, presence, and counts without every transcript.
    func state() async throws -> MarbleState {
        try await fetch(try makeRequest(route: .state))
    }

    func transcript(conversationId: String, after: UInt64?) async throws -> MarbleTranscriptPage {
        try await fetch(
            try makeRequest(route: .transcript(conversationId: conversationId, after: after))
        )
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
        try await post("/api/tasks/\(encoded(taskId))/cancel")
    }

    /// Repeat the last failed, cancelled, or interrupted turn.
    func retry(taskId: String) async throws {
        struct Body: Encodable { var taskId: String }
        try await post("/api/turns/retry", body: Body(taskId: taskId))
    }

    /// Newest transcript matches across all history.
    func search(query: String, limit: Int = 30) async throws -> [MarbleSearchHit] {
        struct Reply: Decodable { var hits: [MarbleSearchHit] }
        let reply: Reply = try await fetch(
            try makeRequest(route: .search(query, limit: limit))
        )
        return reply.hits
    }

    /// `GET /api/export` — the whole document, or one conversation's slice.
    func export(conversationId: String? = nil) async throws -> Data {
        try await fetchData(
            try makeRequest(route: .export(conversationId: conversationId))
        )
    }

    /// Choose the model new conversations use; empty means the engine default.
    func setModel(_ model: String) async throws {
        struct Body: Encodable { var model: String }
        try await post("/api/model", body: Body(model: model))
    }

    /// Routines quiet-hours window, in host minutes past midnight.
    func setRoutineQuiet(_ quiet: MarbleQuietHours) async throws {
        try await post("/api/routines/quiet", body: quiet)
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
        try await fetch(try makeRequest(route: .presence))
    }

    func memory() async throws -> MarbleMemory {
        try await fetch(try makeRequest(route: .memory))
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
        try await fetch(try makeRequest(route: .routines))
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
        try await fetch(try makeRequest(route: .prism))
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
        try await fetch(try makeRequest(route: .tools))
    }

    /// The workspace crate map, read when the sheet that shows it opens.
    func crates() async throws -> MarbleCrateMap {
        try await fetch(try makeRequest(route: .crates))
    }

    // MARK: - Notifications and voice

    struct NotificationUpdate: Encodable {
        var webhookUrl: String
        var webhookToken: String
        var events: [String]
        var apns: MarbleApns?
        var quietHours: MarbleQuietHours?
        var reAlertSeconds: Int?
        var retries: Int?
        var dedupeSeconds: Int?
    }

    func notificationSettings() async throws -> MarbleNotificationSettings {
        try await fetch(try makeRequest(route: .notifications))
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

    /// Registered push devices (the host only ever shows token prefixes).
    func devices() async throws -> [MarbleDevice] {
        struct Reply: Decodable { var devices: [MarbleDevice] }
        do {
            let reply: Reply = try await fetch(try makeRequest(route: .devices))
            return reply.devices
        } catch {
            // Some hosts answer with a bare array instead of a wrapper.
            let devices: [MarbleDevice] = try await fetch(try makeRequest(route: .devices))
            return devices
        }
    }

    func removeDevice(tokenPrefix: String) async throws {
        try await post("/api/devices/\(encoded(tokenPrefix))/remove")
    }

    func voice() async throws -> MarbleVoice {
        try await fetch(try makeRequest(route: .voice))
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
                        route: .events(since: since),
                        acceptsEventStream: true
                    )
                    let (bytes, response) = try await session.bytes(for: request)
                    try marbleValidateStatus(response)
                    var parser = MarbleSSEParser()
                    for try await line in bytes.lines {
                        guard let frame = parser.feed(line),
                              let data = frame.data.data(using: .utf8),
                              let event = try? decoder.decode(MarbleEvent.self, from: data)
                        else { continue }
                        continuation.yield(event)
                    }
                    if let frame = parser.finish(),
                       let data = frame.data.data(using: .utf8),
                       let event = try? decoder.decode(MarbleEvent.self, from: data) {
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

    /// Nothing is saved until onboarding succeeds; there is no default host.
    static var baseURLString: String? {
        get { UserDefaults.standard.string(forKey: baseURLKey) }
        set { UserDefaults.standard.set(newValue, forKey: baseURLKey) }
    }

    static var baseURL: URL? {
        baseURLString.flatMap { URL(string: $0) }
    }

    static var hasHost: Bool { baseURL != nil }

    static func signOut() {
        if let baseURL {
            MarblePinStore.clear(
                for: MarblePinStore.authority(
                    for: MarbleEndpoint(baseURL: baseURL, token: nil)
                )
            )
        }
        baseURLString = nil
        token = nil
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

    static func readKeychain(account: String) -> String? {
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

    static func writeKeychain(_ value: String, account: String) {
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

    static func deleteKeychain(account: String) {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: keychainService,
            kSecAttrAccount as String: account,
        ]
        SecItemDelete(query as CFDictionary)
    }
}
