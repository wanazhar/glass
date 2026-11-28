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
        var request = try endpoint.request(
            path: path,
            query: query,
            method: method,
            body: body,
            acceptsEventStream: acceptsEventStream
        )
        if method == "POST" && ["/api/turns", "/api/turns/retry", "/api/everyone", "/api/prism/dispatch"].contains(path) {
            request.setValue(TimeZone.autoupdatingCurrent.identifier, forHTTPHeaderField: "X-Marble-Time-Zone")
        }
        return request
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
                throw MarbleClientError.requestError(error)
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
                throw MarbleClientError.requestError(error)
            }
        }
    }

    private func post(_ path: String, body: Encodable? = nil, acceptsNotFound: Bool = false) async throws {
        let data = try body.map { try JSONEncoder().encode($0) }
        let request = try makeRequest(path: path, method: "POST", body: data)
        do {
            let (responseData, response) = try await session.data(for: request)
            try marbleValidate(response, data: responseData, acceptsNotFound: acceptsNotFound)
        } catch let error as MarbleClientError {
            throw error
        } catch {
            throw MarbleClientError.requestError(error)
        }
    }

    // MARK: - Product API

    func health(timeout: TimeInterval? = nil) async throws -> MarbleHealth {
        // healthz answers 503 with the same JSON body when the engine is down;
        // onboarding needs that body to say so instead of "unreachable". Every
        // failure maps through MarbleClientError so the evaluator can tell a
        // refused request from a silent one.
        try await withRepin {
            do {
                var request = try makeRequest(route: .health)
                if let timeout { request.timeoutInterval = timeout }
                let (data, response) = try await session.data(for: request)
                let status = (response as? HTTPURLResponse)?.statusCode ?? 0
                if status != 503 { try marbleValidate(response, data: data) }
                return try decoder.decode(MarbleHealth.self, from: data)
            } catch let error as MarbleClientError {
                throw error
            } catch {
                throw MarbleClientError.requestError(error)
            }
        }
    }

    /// Lean state: control, presence, and counts without every transcript.
    func state() async throws -> MarbleState {
        try await fetch(try makeRequest(route: .state))
    }

    func transcript(conversationId: String, around: String? = nil,
                    before: String? = nil) async throws -> MarbleTranscriptPage {
        let limit = around == nil ? MarbleHistoryWindow.pageSize : nil
        let route = MarbleRoute.transcript(conversationId: conversationId, main: true, around: around)
        let page: MarbleTranscriptPage = try await fetch(try makeRequest(route: route.history(before: before, limit: limit)))
        // Older hosts already honored limit without advertising a back cursor.
        if around == nil, before == nil, page.hasMore == nil, page.entries.count == limit {
            return try await fetch(try makeRequest(route: route))
        }
        return page
    }

    func send(text: String, conversationId: String?, threadId: String? = nil,
              clientMessageId: UUID? = nil) async throws -> MarbleAccepted {
        let data = try JSONEncoder().encode(
            MarbleTurnBody(text: text, conversationId: conversationId, threadId: threadId, clientMessageId: clientMessageId)
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

    func createConversation(marbleId: String? = nil) async throws -> MarbleConversation {
        struct Body: Encodable { let marbleId: String }
        struct Reply: Decodable { let conversation: MarbleConversation }
        let reply: Reply = try await postJSON("/api/conversations", body: marbleId.map { Body(marbleId: $0) })
        return reply.conversation
    }

    func renameConversation(id: String, title: String) async throws {
        struct Body: Encodable { var title: String }
        try await post("/api/conversations/\(encoded(id))/rename", body: Body(title: title))
    }

    func deleteConversation(id: String) async throws {
        try await post("/api/conversations/\(encoded(id))/delete", acceptsNotFound: true)
    }

    func archiveConversation(id: String, archived: Bool) async throws {
        try await post("/api/conversations/\(encoded(id))/\(archived ? "archive" : "unarchive")")
    }

    func restoreDeletedChat(id: String) async throws {
        try await post("/api/conversations/\(encoded(id))/restore")
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

    func messageSearch(query: String, threadId: String?, limit: Int = 30) async throws -> [MarbleSearchResult] {
        struct Reply: Decodable { var hits: [MarbleSearchResult] }
        do {
            let reply: Reply = try await fetch(
                try makeRequest(route: .messageSearch(query, threadId: threadId, limit: limit))
            )
            return reply.hits
        } catch MarbleClientError.notFound where threadId == nil {
            return try await search(query: query, limit: limit).map(MarbleSearchResult.init(legacy:))
        }
    }

    func threads(conversationId: String) async throws -> [MarbleThread] {
        struct Reply: Decodable {
            let threads: [MarbleThread]

            init(from decoder: Decoder) throws {
                if var list = try? decoder.unkeyedContainer() {
                    var threads: [MarbleThread] = []
                    while !list.isAtEnd { threads.append(try list.decode(MarbleThread.self)) }
                    self.threads = threads
                } else {
                    self.threads = try decoder.container(keyedBy: MarbleThreadKey.self).required("threads")
                }
            }
        }
        let reply: Reply = try await fetch(try makeRequest(route: .threads(conversationId: conversationId)))
        return reply.threads
    }

    func thread(id: String, before: String? = nil) async throws -> MarbleThreadPage {
        let route = MarbleRoute(path: "/api/threads/\(encoded(id))")
            .history(before: before, limit: MarbleHistoryWindow.pageSize)
        return try await fetch(try makeRequest(route: route))
    }

    func createThread(conversationId: String, parentEntryId: String? = nil, title: String? = nil,
                      group: MarbleGroupThread? = nil) async throws -> MarbleThread {
        struct Body: Encodable {
            var conversation_id: String
            var parent_entry_id: String?
            var title: String?
            var group: MarbleGroupThread?
        }
        struct Reply: Decodable {
            let thread: MarbleThread
            init(from decoder: Decoder) throws {
                let values = try decoder.container(keyedBy: MarbleThreadKey.self)
                thread = try values.optional("thread") ?? MarbleThread(from: decoder)
            }
        }
        let reply: Reply = try await postJSON(
            "/api/threads",
            body: Body(conversation_id: conversationId, parent_entry_id: parentEntryId, title: title, group: group)
        )
        return reply.thread
    }

    func updateEveryoneThread(_ id: String, action: String) async throws {
        struct Reply: Decodable { let ok: Bool }
        let _: Reply = try await postJSON("/api/threads/\(encoded(id))/\(action)", body: nil)
    }

    func updateGroupMembers(_ id: String, members: [String]) async throws {
        struct Reply: Decodable { let ok: Bool }
        let _: Reply = try await postJSON("/api/threads/\(encoded(id))/members", body: members)
    }

    func respondToGroupSuggestion(_ id: String, agentId: String, add: Bool) async throws {
        struct Body: Encodable { let agentId: String; let add: Bool }
        struct Reply: Decodable { let ok: Bool }
        let _: Reply = try await postJSON("/api/threads/\(encoded(id))/suggestion", body: Body(agentId: agentId, add: add))
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

    func setPaused(_ paused: Bool) async throws -> MarbleControl {
        try await changeControl(at: paused ? "/api/control/pause" : "/api/control/resume")
    }

    func stopEverything() async throws -> MarbleControl {
        try await changeControl(at: "/api/control/stop")
    }

    private func changeControl(at path: String) async throws -> MarbleControl {
        struct Response: Decodable { let control: MarbleControl }
        let response: Response = try await fetch(try makeRequest(path: path, method: "POST"))
        return response.control
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
    enum KnowledgeAction: String {
        case delete, restore
        case shareMemory = "share-memory"
        case editSkill = "edit-skill"
        case shareSkill = "share-skill"
        case confirmPruning = "confirm-pruning"
        case resolveConsent = "resolve-consent"
    }

    struct KnowledgeInput: Encodable {
        var shared: Bool?
        var name: String?
        var whenToUse: String?
        var steps: [String]?
        var agents: [String]?
        var selected: [String]?
        var factId: String?
    }

    func updateKnowledge(id: String, _ action: KnowledgeAction, _ input: KnowledgeInput) async throws -> MarbleMemory {
        let reply: MemoryReply = try await postJSON("/api/knowledge/\(encoded(id))/\(action.rawValue)", body: input)
        return reply.memory
    }
    private struct RoutinesReply: Decodable { var routines: MarbleRoutines }
    private struct OkReply: Decodable { var ok: Bool }

    private func encoded(_ value: String) -> String {
        value.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? value
    }

    func remember(text: String, owner: String = "") async throws -> MarbleMemory {
        struct Body: Encodable { var text: String; var owner: String }
        let reply: MemoryReply = try await postJSON("/api/memory", body: Body(text: text, owner: owner))
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

    /// Whether a routine runs the schedules it missed while the host was down.
    func setRoutineCatchUp(id: String, enabled: Bool) async throws {
        struct Body: Encodable { let enabled: Bool }
        try await post("/api/routines/\(encoded(id))/catch-up", body: Body(enabled: enabled))
    }

    func removeRoutine(id: String) async throws {
        try await post("/api/routines/\(encoded(id))/remove")
    }

    // MARK: - Prism and tooling

    func prism() async throws -> MarblePrism {
        try await fetch(try makeRequest(route: .prism))
    }

    struct MarbleDraft: Codable {
        var name: String
        var role: String
        var glyph: String
        var hue: Int
        var workspace: String
        var persona: String
        var card: MarbleAgentCard? = nil
        var duplicateId: String? = nil
    }

    /// A marble edit; nil fields keep their current value on the host.
    struct MarbleUpdate: Encodable {
        var name: String?
        var role: String?
        var glyph: String?
        var hue: Int?
        var workspace: String?
        var persona: String?
        var card: MarbleAgentCard?
    }

    private struct PrismReply: Decodable { var prism: MarblePrism }

    struct DispatchReply: Decodable {
        var dispatched: [String]
        var marbles: [String]
        var swarm: Bool
        var entry_id: String?
        var client_message_id: String?
        var taskId: String?
    }

    @discardableResult
    func addMarble(_ draft: MarbleDraft) async throws -> MarblePrism {
        let reply: PrismReply = try await postJSON("/api/prism/marbles", body: draft)
        return reply.prism
    }

    /// Edit a marble — name, look, workspace, or its personality. A persona
    /// change restarts the marble's conversation on the host.
    @discardableResult
    func updateMarble(id: String, _ update: MarbleUpdate) async throws -> MarblePrism {
        let reply: PrismReply = try await postJSON(
            "/api/prism/marbles/\(encoded(id))/update",
            body: update
        )
        return reply.prism
    }

    /// The shared chat's timeline, newest entries after `after`.
    func everyone(before: String? = nil) async throws -> MarbleEveryonePage {
        try await fetch(try makeRequest(route: .everyone(
            after: nil, main: true, before: before, limit: MarbleHistoryWindow.pageSize
        )))
    }

    /// Empty the shared Everyone timeline; other chats are untouched.
    func clearEveryone() async throws {
        let _: OkReply = try await postJSON("/api/everyone/clear", body: Optional<String>.none)
    }

    /// One message on the shared plane; the marbles it names answer.
    @discardableResult
    func sendEveryone(text: String, threadId: String? = nil, clientMessageId: UUID? = nil) async throws -> MarbleEveryoneReply {
        try await postJSON("/api/everyone", body: MarbleTurnBody(
            text: text, conversationId: nil, threadId: threadId, clientMessageId: clientMessageId
        ))
    }

    func removeMarble(id: String) async throws {
        let _: OkReply = try await postJSON(
            "/api/prism/marbles/\(encoded(id))/remove",
            body: Optional<String>.none
        )
    }

    /// Hand one prompt to several marbles at once. Empty `marbles` means all.
    func dispatchSwarm(text: String, marbles: [String], clientMessageId: UUID? = nil) async throws -> DispatchReply {
        struct Body: Encodable {
            var text: String
            var marbles: [String]?
            var client_message_id: UUID?
        }
        return try await postJSON(
            "/api/prism/dispatch",
            body: Body(text: text, marbles: marbles.isEmpty ? nil : marbles, client_message_id: clientMessageId)
        )
    }

    func tools() async throws -> MarbleToolInventory {
        try await fetch(try makeRequest(route: .tools))
    }

    /// The workspace crate map, read when the sheet that shows it opens.
    func crates() async throws -> MarbleCrateMap {
        try await fetch(try makeRequest(route: .crates))
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
    ///
    /// Raw bytes — never `bytes.lines`: `AsyncLineSequence` silently drops the
    /// empty lines that complete SSE frames, so keepalive-only streams and
    /// every event would be invisible.
    enum StreamUpdate {
        case opened
        case activity
        case baseline(UInt64)
        case event(MarbleEvent)

        init?(frame: MarbleSSEFrame, decoder: JSONDecoder) {
            if frame.event == "hello" {
                guard let cursor = frame.replayCursor else { return nil }
                self = .baseline(cursor)
            } else {
                guard let event = try? decoder.decode(MarbleEvent.self, from: Data(frame.data.utf8)) else { return nil }
                self = .event(event)
            }
        }
    }

    func events(since: UInt64) -> AsyncThrowingStream<StreamUpdate, Error> {
        AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    let request = try makeRequest(
                        route: .events(since: since),
                        acceptsEventStream: true
                    )
                    let (bytes, response) = try await session.bytes(for: request)
                    try marbleValidateStatus(response)
                    continuation.yield(.opened)
                    var splitter = MarbleSSELineSplitter()
                    var parser = MarbleSSEParser()
                    for try await byte in bytes {
                        for line in splitter.feed(byte) {
                            if line.hasPrefix(":") { continuation.yield(.activity) }
                            guard let frame = parser.feed(line),
                                  let update = StreamUpdate(frame: frame, decoder: decoder)
                            else { continue }
                            continuation.yield(update)
                        }
                    }
                    if let last = splitter.finish() {
                        _ = parser.feed(last)
                    }
                    if let frame = parser.finish(),
                       let update = StreamUpdate(frame: frame, decoder: decoder) {
                        continuation.yield(update)
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
