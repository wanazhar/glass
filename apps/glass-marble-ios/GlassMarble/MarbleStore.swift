import ActivityKit
import Foundation
import Network
import OSLog
import SwiftUI
import UserNotifications

/// Observable product store shared by every iOS surface.
@MainActor
final class MarbleStore: ObservableObject {
    /// Kept for the views' tone mapping; the label itself comes from the
    /// Core reducer so every surface says the same word.
    enum Connection: String {
        case idle
        case connecting
        case live
        case offline
    }

    /// One conversation's in-flight reply parts; several chats can stream at
    /// once, so each keeps its own instead of sharing one buffer.
    private struct LiveBuffers: Equatable {
        var text = ""
        var reasoning = ""
        var taskId: String?

        var isEmpty: Bool {
            text.isEmpty && reasoning.isEmpty && taskId == nil
        }
    }

    @Published private(set) var state: MarbleState = .empty
    private var status = MarbleConnectionStatus() {
        didSet { connection = connectionFor(status.state) }
    }
    @Published private(set) var connection: Connection = .idle

    private func connectionFor(_ state: MarbleConnectionStatus.State) -> Connection {
        switch state {
        case .connected: return .live
        case .connecting: return .connecting
        case .offline: return .offline
        case .signedOut: return .idle
        }
    }
    @Published private(set) var endpoint: MarbleEndpoint?
    /// False until a host has been chosen and validated; drives onboarding.
    @Published private(set) var onboarded: Bool
    @Published var selectedConversationId: String? {
        didSet {
            guard oldValue != selectedConversationId else { return }
            // Seed the buffer from the host's task table so Stop/steer show
            // even for a turn that began before this surface was watching.
            if let id = selectedConversationId {
                var buffers = live[id] ?? LiveBuffers()
                if let taskId = mainChatTask(in: id, state: state)?.id, buffers.taskId == nil {
                    buffers.taskId = taskId
                    live[id] = buffers
                }
            }
            syncLive()
            Task { await self.refresh() }
        }
    }
    /// History for the selected conversation, fetched separately from state.
    @Published private(set) var conversationTranscript: [MarbleEntry] = []
    private var transcriptConversationId: String?
    private var transcriptCursor: UInt64 = 0
    private var transcriptRevision: Int = -1
    /// Everyone and each marble's own chat, paged from the host: a lean state
    /// poll carries no transcript, so these never come from `state`.
    @Published private(set) var everyoneTimeline: [MarbleEntry] = []
    @Published private(set) var marbleTranscripts: [String: [MarbleEntry]] = [:]
    @Published private(set) var threadUpdates: [String: MarbleThread] = [:]
    @Published private(set) var threadEventSequences: [String: UInt64] = [:]
    @Published private(set) var threadHostRevision: UInt64 = 0
    private var threadTaskIds: Set<String> = []
    private var chatsRevision: Int = -1
    /// Advanced on every host change or sign-out so late replies from the
    /// previous host are dropped rather than shown.
    private var hostGeneration = MarbleConnectionGeneration()
    @Published private(set) var liveText = ""
    @Published private(set) var liveReasoning = ""
    @Published private var toolsByTurn: [String: [MarbleToolActivity]] = [:]
    @Published private var reasoningByTurn: [String: String] = [:]
    private var reasoningTurnIds: [String] = []
    @Published private var awaitingTranscript: [String: String] = [:]
    @Published private var publishedEveryoneWork: [String: [String]] = [:]
    private var recentToolTurnIDs: [String] = []
    var liveTools: [MarbleToolActivity] {
        guard let activeTaskId, let selectedConversationId,
              !hasTranscriptToolRows(in: selectedConversationId, taskId: activeTaskId) else { return [] }
        return tools(for: activeTaskId)
    }
    @Published private(set) var activeTaskId: String?
    @Published var errorMessage: String?
    /// Last accepted Prism change, shown inside the Prism sheet.
    @Published var prismNotice: String?
    @Published var needsToken = false
    /// Reported by `/healthz` during refresh, for the About section.
    @Published private(set) var hostVersion: String?
    @Published private(set) var engineHealth: MarbleHealth.RuntimeHealth?

    private var client: MarbleClient?
    private let speaker = MarbleSpeaker()
    private var streamTask: Task<Void, Never>?
    private var sequence: UInt64 = 0
    private var hostPID: Int?
    private var started = false
    /// Per-conversation live buffers; `liveText` & friends mirror the selected one.
    @Published private var live: [String: LiveBuffers] = [:]
    private var backoffSeconds: Double = 1
    private var lastReconnectAt: TimeInterval = 0
    private var pathMonitor: NWPathMonitor?
    /// NWPathMonitor reports the current path on its first callback; only
    /// real changes after that are worth a reconnect.
    private var pathMonitorSeenFirst = false
    private var appInBackground = false
    private var activityStartedForTransition = false
    private var streamConnectedAt: Date?
    private let backgroundConnection = MarbleBackgroundConnection()
    private static let streamLog = Logger(subsystem: "com.wanazhar.glassmarble", category: "stream")
    private static let backgroundConnectionKey = "marble.backgroundConnection"
    private static let connectionPausedKey = "marble.connectionPaused"
    static weak var liveActivityStore: MarbleStore?
    @Published private(set) var locationPermissionDenied = false
    @Published private(set) var connectionPaused = UserDefaults.standard.bool(forKey: MarbleStore.connectionPausedKey)
    @Published var backgroundConnectionEnabled = UserDefaults.standard.bool(forKey: MarbleStore.backgroundConnectionKey) {
        didSet {
            UserDefaults.standard.set(backgroundConnectionEnabled, forKey: Self.backgroundConnectionKey)
            updateBackgroundConnection()
        }
    }
    private var notificationCursor = MarbleNotificationCursor()
    var shouldPresentReply: ((String, String?) -> Bool)?

    /// Local-notification opt-in, persisted between launches (default off).
    private static let localNotificationsKey = "marble.localNotifications"
    @Published var localNotificationsEnabled: Bool {
        didSet {
            UserDefaults.standard.set(localNotificationsEnabled, forKey: Self.localNotificationsKey)
            if localNotificationsEnabled { Task { await requestLocalNotificationPermission() } }
        }
    }

    init() {
        let endpoint = MarblePreferences.baseURL.map {
            MarbleEndpoint(baseURL: $0, token: MarblePreferences.token)
        }
        self.endpoint = endpoint
        self.onboarded = endpoint != nil
        self.localNotificationsEnabled = UserDefaults.standard.bool(
            forKey: Self.localNotificationsKey
        )
        if !connectionPaused { self.client = endpoint.map { MarbleClient(endpoint: $0) } }
        if let endpoint {
            restoreCache(for: endpoint)
        }
        backgroundConnection.authorizationChanged = { [weak self] in
            self?.updateBackgroundConnection()
        }
    }

    // MARK: - Derived product state

    var selectedConversation: MarbleConversation? {
        state.conversation(selectedConversationId)
    }

    var transcript: [MarbleEntry] {
        if transcriptConversationId == selectedConversationId,
           !conversationTranscript.isEmpty {
            return conversationTranscript.filter { $0.threadId == nil }
        }
        return state.transcript(for: selectedConversationId).filter { $0.threadId == nil }
    }

    var isWorking: Bool { activeTaskId != nil }

    /// One plain line under a chat row: what that chat needs or is doing,
    /// or nothing at all when it is just sitting there.
    func stateLine(for conversation: MarbleConversation) -> String? {
        let tasks = state.tasks.filter { $0.conversationId == conversation.id }
        if tasks.contains(where: { $0.status == "running" }) { return "Working…" }
        if let latest = tasks.sorted(by: { $0.at > $1.at }).first {
            switch latest.status {
            case "cancelled", "interrupted": return "Cancelled before replying"
            case "failed": return "Failed before replying"
            case "queued": return "Waiting"
            default: break
            }
        }
        return nil
    }

    /// Marble's own control layer, as published by the host.
    var control: MarbleControl { state.controlState }

    /// What the assistant is doing, when the host reports it.
    var presence: MarblePresence { state.presenceInfo }
    var memory: MarbleMemory { state.memoryState }
    var routines: MarbleRoutines { state.routineState }
    var voice: MarbleVoice { state.voiceSettings }
    var prism: MarblePrism { state.prismState }
    var model: String { state.model ?? "" }

    /// The full tool inventory, fetched on demand because it changes slowly.
    @Published private(set) var toolInventory: MarbleToolInventory = .empty
    /// The workspace crate map, fetched when the control sheet opens.
    @Published private(set) var crates: MarbleCrateMap?
    @Published private(set) var cratesLoading = false
    @Published private(set) var cratesError: String?
    /// False until the first state arrives, so the transcript can show shaped
    /// placeholders instead of an empty conversation that looks settled.
    @Published private(set) var loaded = false

    /// The one status word every surface shows — the connection label with
    /// work overlays on top, so Chats, the chat header, and Settings never
    /// disagree about what Marble is doing.
    var statusLabel: String {
        if connectionPaused { return "Disconnected" }
        if isWorking { return "Working" }
        if presence.status == "waiting" { return "Waiting" }
        if presence.status == "paused" || control.paused { return "Paused" }
        if presence.status == "working" { return "Working" }
        return status.label
    }

    // MARK: - Lifecycle

    func start() {
        guard onboarded, !started else { return }
        started = true
        startPathMonitor()
        let oldActivities = Activity<MarbleConnectionActivityAttributes>.activities
        Task {
            for activity in oldActivities { await activity.end(nil, dismissalPolicy: .immediate) }
        }
        if !connectionPaused, !appInBackground { listen() }
    }

    /// A new host+token that passed onboarding validation.
    func apply(_ endpoint: MarbleEndpoint) {
        stopStreaming()
        setConnectionPaused(false)
        self.endpoint = endpoint
        MarblePreferences.save(endpoint)
        client = MarbleClient(endpoint: endpoint)
        onboarded = true
        sequence = 0
        hostPID = nil
        notificationCursor = MarbleNotificationCursor()
        needsToken = false
        conversationTranscript = []
        transcriptConversationId = nil
        transcriptCursor = 0
        transcriptRevision = -1
        hostGeneration.advance()
        resetChats()
        restoreCache(for: endpoint)
        live = [:]
        syncLive()
        started = true
        startPathMonitor()
        let generation = hostGeneration.current
        Task {
            await loginIfNeeded()
            guard hostGeneration.isCurrent(generation), !appInBackground else { return }
            listen()
        }
    }

    /// Forget host and token and return to onboarding.
    func signOut() {
        stopStreaming()
        setConnectionPaused(false)
        if let endpoint { Self.removeCache(for: endpoint) }
        MarblePreferences.signOut()
        client = nil
        endpoint = nil
        onboarded = false
        started = false
        sequence = 0
        hostPID = nil
        notificationCursor = MarbleNotificationCursor()
        state = .empty
        conversationTranscript = []
        transcriptConversationId = nil
        transcriptCursor = 0
        transcriptRevision = -1
        hostGeneration.advance()
        resetChats()
        live = [:]
        syncLive()
        status.signedOut()
        loaded = false
    }

    /// Reconnect right now: scene became active or the network path changed.
    /// Coalesced — a foregrounding, a path callback, and a retry can all land
    /// within a second of each other, and only one attempt should run.
    func reconnect(force: Bool = false) {
        guard onboarded, !appInBackground, !connectionPaused || force else { return }
        if connectionPaused, let endpoint {
            client = MarbleClient(endpoint: endpoint)
            setConnectionPaused(false)
        }
        let now = Date().timeIntervalSinceReferenceDate
        guard force || now - lastReconnectAt >= 1 else { return }
        lastReconnectAt = now
        backoffSeconds = 1
        listen()
    }

    func appWillResignActive() {
        guard backgroundConnectionEnabled, let streamConnectedAt, let endpoint else { return }
        activityStartedForTransition = backgroundConnection.startActivity(
            connectedAt: streamConnectedAt, host: endpoint.displayHost
        )
    }

    func appBecameActive() {
        let wasBackgrounded = appInBackground
        appInBackground = false
        if activityStartedForTransition, !wasBackgrounded {
            backgroundConnection.endActivity()
        }
        activityStartedForTransition = false
        updateBackgroundConnection()
        guard wasBackgrounded, streamConnectedAt == nil, !connectionPaused else { return }
        status.resume()
        reconnect(force: true)
    }

    func appDidEnterBackground() {
        appInBackground = true
        if !backgroundConnection.isKeepingAlive {
            status.suspend()
            stopStreaming()
        }
        saveCursor()
        saveCache()
    }

    static func disconnectFromLiveActivity() {
        guard let store = liveActivityStore else {
            // A Live Activity button can wake the app without creating a scene.
            UserDefaults.standard.set(true, forKey: connectionPausedKey)
            return
        }
        store.setConnectionPaused(true)
        store.stopStreaming()
        store.client = nil
        store.hostGeneration.advance()
        store.status = MarbleConnectionStatus()
    }

    private func setConnectionPaused(_ paused: Bool) {
        connectionPaused = paused
        UserDefaults.standard.set(paused, forKey: Self.connectionPausedKey)
    }

    private func updateBackgroundConnection() {
        let keepAlive = backgroundConnectionEnabled && streamConnectedAt != nil && !needsToken
        backgroundConnection.setLocationNeeded(keepAlive, canStart: !appInBackground)
        locationPermissionDenied = backgroundConnectionEnabled && backgroundConnection.permissionDenied
        if !backgroundConnection.isKeepingAlive {
            backgroundConnection.endActivity()
            if appInBackground, streamTask != nil {
                status.suspend()
                stopStreaming()
            }
        }
    }

    private func streamDisconnected() {
        if streamConnectedAt != nil { Self.streamLog.debug("SSE disconnected") }
        streamConnectedAt = nil
        backgroundConnection.setLocationNeeded(false, canStart: false)
        backgroundConnection.endActivity()
    }

    private func stopStreaming() {
        streamTask?.cancel()
        streamTask = nil
        streamDisconnected()
    }

    // MARK: - On-device cache

    private static let cursorKeyPrefix = "marble.eventCursor."
    private static let hostPIDKeyPrefix = "marble.hostPID."
    private static let notificationCursorKeyPrefix = "marble.replyNotificationCursor."

    private func saveNotificationCursor() {
        guard let endpoint,
              let data = try? JSONEncoder().encode(notificationCursor) else { return }
        UserDefaults.standard.set(data, forKey: Self.notificationCursorKeyPrefix + endpoint.baseURL.absoluteString)
    }

    private static func cacheURL(for endpoint: MarbleEndpoint) -> URL? {
        FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask).first?
            .appendingPathComponent(MarbleChatCache.fileName(for: endpoint.baseURL.absoluteString))
    }

    private static func removeCache(for endpoint: MarbleEndpoint) {
        if let url = cacheURL(for: endpoint) { try? FileManager.default.removeItem(at: url) }
        UserDefaults.standard.removeObject(forKey: cursorKeyPrefix + endpoint.baseURL.absoluteString)
        UserDefaults.standard.removeObject(forKey: hostPIDKeyPrefix + endpoint.baseURL.absoluteString)
        UserDefaults.standard.removeObject(forKey: notificationCursorKeyPrefix + endpoint.baseURL.absoluteString)
    }

    /// Show the last fetched chats for this host straight away, and resume
    /// its event stream from the last event seen.
    private func restoreCache(for endpoint: MarbleEndpoint) {
        if let data = UserDefaults.standard.data(forKey: Self.notificationCursorKeyPrefix + endpoint.baseURL.absoluteString) {
            notificationCursor = (try? JSONDecoder().decode(MarbleNotificationCursor.self, from: data)) ?? MarbleNotificationCursor()
        }
        sequence = UInt64(UserDefaults.standard.integer(
            forKey: Self.cursorKeyPrefix + endpoint.baseURL.absoluteString
        ))
        hostPID = UserDefaults.standard.object(forKey: Self.hostPIDKeyPrefix + endpoint.baseURL.absoluteString) as? Int
        guard let url = Self.cacheURL(for: endpoint),
              let data = try? Data(contentsOf: url),
              let cache = MarbleChatCache.decoded(data) else { return }
        everyoneTimeline = cache.everyone
        marbleTranscripts = cache.transcripts
    }

    private func saveCache() {
        guard let endpoint, let url = Self.cacheURL(for: endpoint) else { return }
        let cache = MarbleChatCache(everyone: everyoneTimeline, transcripts: marbleTranscripts)
        try? cache.encoded().write(to: url, options: .atomic)
    }

    private func saveCursor() {
        guard let endpoint else { return }
        UserDefaults.standard.set(
            Int(clamping: sequence),
            forKey: Self.cursorKeyPrefix + endpoint.baseURL.absoluteString
        )
    }

    private func startPathMonitor() {
        guard pathMonitor == nil else { return }
        let monitor = NWPathMonitor()
        monitor.pathUpdateHandler = { [weak self] _ in
            Task { @MainActor [weak self] in
                guard let self else { return }
                if !self.pathMonitorSeenFirst {
                    self.pathMonitorSeenFirst = true
                    return
                }
                // Any path change is worth a reconnection attempt: the stream
                // may be dead even when the path still reports satisfied.
                self.reconnect()
            }
        }
        monitor.start(queue: DispatchQueue(label: "marble.pathmonitor"))
        pathMonitor = monitor
    }

    private func loginIfNeeded() async {
        guard let client, let token = endpoint?.token, !token.isEmpty else { return }
        do {
            try await client.login(token: token)
        } catch {
            // The bearer header still authenticates every later request.
        }
    }

    // MARK: - Product actions

    func refresh() async {
        guard let client else { return }
        let generation = hostGeneration.current
        do {
            async let healthFetch = client.health()
            let snapshot = try await client.state()
            guard hostGeneration.isCurrent(generation) else { return }
            state = snapshot
            let finished = Set(snapshot.tasks.filter { !$0.isRunning && $0.status != "queued" }.map(\.id))
            if !everyoneTaskIds.isDisjoint(with: finished) { everyoneTaskIds.subtract(finished) }
            needsToken = false
            errorMessage = nil
            status.refreshSucceeded()
            if let health = try? await healthFetch {
                guard hostGeneration.isCurrent(generation) else { return }
                hostVersion = health.version
                engineHealth = health.runtime
            }
            guard hostGeneration.isCurrent(generation) else { return }
            // Navigation owns which chat is on screen. A refresh may only
            // clear a selection whose conversation disappeared — never choose
            // one, or a poll would re-push a chat the person just left.
            if let selected = selectedConversationId,
               snapshot.conversation(selected) == nil {
                selectedConversationId = nil
            }
            // Several chats run at once, one engine lane each. This surface is
            // looking at one of them, so only that chat's turn makes it busy.
            // The buffer is the single source of truth for activeTaskId;
            // writing the buffer keeps later syncLive() calls from reverting it.
            let selectedTask = mainChatTask(in: selectedConversationId, state: snapshot)?.id
            let selectedId = selectedConversationId ?? ""
            var buffers = live[selectedId] ?? LiveBuffers()
            if buffers.taskId != selectedTask {
                buffers = selectedTask.map { taskId -> LiveBuffers in
                    var b = buffers
                    b.taskId = taskId
                    return b
                } ?? LiveBuffers()
                live[selectedId] = buffers.isEmpty ? nil : buffers
                syncLive()
            }
            loaded = true
            await loadTranscript()
            await loadChats()
        } catch MarbleClientError.unauthorized {
            guard hostGeneration.isCurrent(generation) else { return }
            markUnauthorized()
            status.refreshFailed("The token was rejected")
            loaded = true
        } catch {
            guard hostGeneration.isCurrent(generation) else { return }
            loaded = true
            guard status.refreshFailed(error.localizedDescription) else {
                // Just back in the app: the network is often still waking up.
                Task {
                    try? await Task.sleep(for: .seconds(1))
                    await self.refresh()
                }
                return
            }
            errorMessage = error.localizedDescription
        }
    }

    private func resetChats() {
        resetChatActivity()
        everyoneTimeline = []
        marbleTranscripts = [:]
        chatsRevision = -1
    }

    private func resetChatActivity() {
        everyoneTaskIds = []
        toolsByTurn = [:]
        reasoningByTurn = [:]
        reasoningTurnIds = []
        awaitingTranscript = [:]
        publishedEveryoneWork = [:]
        recentToolTurnIDs = []
        retryingTurns = []
        retryFailures = []
        threadUpdates = [:]
        threadEventSequences = [:]
        threadTaskIds = []
    }

    /// Fetch the Everyone timeline and every marble's own chat once per
    /// document revision.
    private func loadChats() async {
        guard let client, chatsRevision != state.revision else { return }
        let revision = state.revision
        let generation = hostGeneration.current
        do {
            let timeline = try await client.everyone()
            var transcripts: [String: [MarbleEntry]] = [:]
            for marble in prism.marbles where !marble.conversationId.isEmpty {
                do {
                    let page = try await client.transcript(
                        conversationId: marble.conversationId, after: nil
                    )
                    transcripts[marble.conversationId] = page.entries
                } catch MarbleClientError.notFound {
                    transcripts[marble.conversationId] = []
                }
            }
            guard hostGeneration.isCurrent(generation), chatsRevision <= revision else { return }
            everyoneTimeline = timeline
            marbleTranscripts = transcripts
            chatsRevision = revision
            publishedEveryoneWork = Dictionary(uniqueKeysWithValues: prism.marbles.compactMap { marble in
                guard isAgentWorking(marble) else { return nil }
                let tasks = activeTaskIds(for: marble)
                return tasks.isEmpty ? nil : (marble.id, tasks)
            })
            for id in transcripts.keys where state.runningTask(for: id) == nil {
                awaitingTranscript.removeValue(forKey: id)
            }
            saveCache()
        } catch {
            guard hostGeneration.isCurrent(generation) else { return }
            errorMessage = error.localizedDescription
        }
    }

    /// Fetch the selected conversation's history separately, resuming from the
    /// last cursor so a refresh does not carry every conversation at once.
    private func loadTranscript() async {
        guard let client, let conversationId = selectedConversationId else {
            conversationTranscript = []
            transcriptConversationId = nil
            transcriptCursor = 0
            transcriptRevision = -1
            return
        }
        if transcriptConversationId != conversationId {
            transcriptConversationId = conversationId
            conversationTranscript = []
            transcriptCursor = 0
            transcriptRevision = -1
        }
        guard transcriptRevision != state.revision || conversationTranscript.isEmpty else {
            return
        }
        let generation = hostGeneration.current
        let revision = state.revision
        let after = transcriptCursor == 0 ? nil : transcriptCursor
        do {
            let page = try await client.transcript(conversationId: conversationId, after: after)
            guard hostGeneration.isCurrent(generation), selectedConversationId == conversationId,
                  transcriptRevision <= revision else { return }
            if after == nil { conversationTranscript = [] }
            var offsets = Dictionary(conversationTranscript.enumerated().map {
                (Self.transcriptKey($0.element), $0.offset)
            }, uniquingKeysWith: { first, _ in first })
            for entry in page.entries {
                let key = Self.transcriptKey(entry)
                if let index = offsets[key] {
                    if conversationTranscript[index] != entry { conversationTranscript[index] = entry }
                } else {
                    offsets[key] = conversationTranscript.count
                    conversationTranscript.append(entry)
                }
            }
            transcriptCursor = max(transcriptCursor, page.cursor)
            transcriptRevision = revision
            if state.runningTask(for: conversationId) == nil {
                awaitingTranscript.removeValue(forKey: conversationId)
            }
        } catch MarbleClientError.notFound {
            guard hostGeneration.isCurrent(generation), selectedConversationId == conversationId else { return }
            state.conversations.removeAll { $0.id == conversationId }
            selectedConversationId = nil
            conversationTranscript = []
            errorMessage = "This chat no longer exists"
        } catch {
            guard hostGeneration.isCurrent(generation), selectedConversationId == conversationId else { return }
            errorMessage = error.localizedDescription
        }
    }

    private static func transcriptKey(_ entry: MarbleEntry) -> String {
        entry.serverID ?? "\(entry.at)|\(entry.kind)|\(entry.taskId)|\(entry.text)"
    }

    func send(_ text: String) async {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let client, !trimmed.isEmpty else { return }
        do {
            let accepted = try await client.send(
                text: trimmed,
                conversationId: selectedConversationId
            )
            selectedConversationId = accepted.conversationId
            // A marble answers in its own engine and reports its task through the
            // next state poll, so an empty id must not look like a live turn.
            live[accepted.conversationId] = LiveBuffers(
                taskId: accepted.taskId.isEmpty ? nil : accepted.taskId
            )
            syncLive()
            if let marble = accepted.marbleName {
                prismNotice = "\(marble) is answering in its own conversation."
            }
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    /// Send a correction into the turn that is running now.
    /// Returns whether the host accepted the correction, so the composer
    /// keeps the draft when it did not.
    @discardableResult
    func steer(_ text: String) async -> Bool {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let client, !trimmed.isEmpty else { return false }
        guard let taskId = activeTaskId else {
            errorMessage = "Nothing is running to correct"
            return false
        }
        do {
            try await client.steer(
                text: trimmed,
                conversationId: selectedConversationId,
                taskId: taskId
            )
            errorMessage = nil
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    /// Repeat a failed, cancelled, or interrupted turn.
    @Published private(set) var retryingTurns: Set<String> = []
    @Published private(set) var retryFailures: Set<String> = []

    func retry(taskId: String) async {
        guard let client, !taskId.isEmpty, retryingTurns.insert(taskId).inserted else { return }
        let generation = hostGeneration.current
        retryFailures.remove(taskId)
        do {
            try await client.retry(taskId: taskId)
            guard hostGeneration.isCurrent(generation) else { return }
            await refresh()
        } catch {
            guard hostGeneration.isCurrent(generation) else { return }
            retryFailures.insert(taskId)
        }
        guard hostGeneration.isCurrent(generation) else { return }
        retryingTurns.remove(taskId)
    }

    func tools(for taskID: String) -> [MarbleToolActivity] { toolsByTurn[taskID] ?? [] }

    func hasTranscriptToolRows(in conversationId: String, taskId: String) -> Bool {
        guard !taskId.isEmpty else { return false }
        let entries: [MarbleEntry]
        if conversationId == everyoneConversationId {
            entries = everyoneEntries
        } else if conversationId == selectedConversationId {
            entries = transcript
        } else {
            entries = marbleTranscripts[conversationId] ?? []
        }
        return entries.contains { $0.kind == "tool" && $0.taskId == taskId }
    }

    func reasoning(for taskId: String) -> String {
        guard !taskId.isEmpty else { return "" }
        if let streamed = reasoningByTurn[taskId] { return streamed }
        guard let task = state.tasks.first(where: { $0.id == taskId }),
              let entries = marbleTranscripts[task.conversationId] else { return "" }
        return entries.filter { $0.taskId == taskId && $0.kind == "reasoning" }.map(\.text).joined(separator: "\n\n")
    }

    func isTaskWorking(_ task: MarbleTask) -> Bool {
        task.isRunning && prism.marbles.contains { marble in
            isAgentWorking(marble) && activeTaskIds(for: marble).contains(task.id)
        }
    }

    func isAwaitingReply(in conversationId: String?, entries: [MarbleEntry]) -> Bool {
        guard let conversationId, let taskId = awaitingTranscript[conversationId] else { return false }
        return !entries.contains { $0.taskId == taskId && ["assistant", "failure", "error"].contains($0.kind) }
    }

    func isAgentWorking(_ marble: MarblePrismMarble) -> Bool {
        marble.isWorking(presence: presence.agents?[marble.id])
    }

    func agentState(for marble: MarblePrismMarble) -> String {
        marble.displayState(presence: presence.agents?[marble.id])
    }

    private func activeTaskIds(for marble: MarblePrismMarble) -> [String] {
        marble.activeTaskIds(presence: presence.agents?[marble.id])
    }

    func agentName(for taskId: String) -> String? {
        prism.marbles.first { activeTaskIds(for: $0).contains(taskId) }?.name
    }

    func latestTurn(in conversationID: String?) -> MarbleTask? {
        guard let conversationID, !conversationID.isEmpty else { return nil }
        return state.tasks.last { $0.conversationId == conversationID && !threadTaskIds.contains($0.id) }
    }

    private func mainChatTask(in id: String?, state: MarbleState) -> MarbleTask? {
        guard let id else { return nil }
        return state.tasks.first { $0.conversationId == id && $0.isRunning && !threadTaskIds.contains($0.id) }
    }

    var everyoneTurns: [MarbleTask] {
        let ids = Set(everyoneEntries.map(\.taskId)).union(everyoneTaskIds)
        return state.tasks.filter {
            !threadTaskIds.contains($0.id) && (ids.contains($0.id) || $0.conversationId == everyoneConversationId)
        }
    }

    /// Transcript search across every conversation.
    func search(query: String, threadId: String? = nil) async throws -> [MarbleSearchResult] {
        try await withChatClient { try await $0.messageSearch(query: query, threadId: threadId) }
    }

    func threads(in conversationId: String) async throws -> [MarbleThread] {
        try await withChatClient { try await $0.threads(conversationId: conversationId) }
    }

    func thread(id: String) async throws -> MarbleThreadPage {
        try await withChatClient { try await $0.thread(id: id) }
    }

    func startThread(from entry: MarbleEntry) async throws -> MarbleThread {
        guard let id = entry.serverID else { throw MarbleClientError.notFound }
        let thread = try await withChatClient {
            try await $0.createThread(conversationId: entry.conversationId, parentEntryId: id)
        }
        threadUpdates[thread.id] = thread
        return thread
    }

    func threadSummary(for entry: MarbleEntry) -> MarbleThreadSummary? {
        if let id = entry.thread?.id {
            return threadUpdates[id]?.summary ?? entry.thread
        }
        guard let id = entry.serverID else { return nil }
        return threadUpdates.values.first {
            $0.conversationId == entry.conversationId && $0.parentEntryId == id
        }?.summary
    }

    func transcript(conversationId: String, around entryId: String) async throws -> [MarbleEntry] {
        let page = try await withChatClient {
            try await $0.transcript(conversationId: conversationId, around: entryId)
        }
        return page.entries.filter { $0.threadId == nil }
    }

    func sendInThread(_ text: String, thread: MarbleThread) async throws {
        try await withChatClient { client in
            if thread.conversationId == everyoneConversationId {
                let reply = try await client.sendEveryone(text: text, threadId: thread.id)
                guard reply.ok else { throw MarbleClientError.conflict("The reply wasn’t accepted.") }
            } else {
                let reply = try await client.send(text: text, conversationId: thread.conversationId, threadId: thread.id)
                guard reply.accepted else { throw MarbleClientError.conflict("The reply wasn’t accepted.") }
            }
        }
    }

    private func withChatClient<T>(_ operation: (MarbleClient) async throws -> T) async throws -> T {
        guard let client else { throw MarbleClientError.offline("") }
        let generation = hostGeneration.current
        let result = try await operation(client)
        try Task.checkCancellation()
        guard hostGeneration.isCurrent(generation) else { throw CancellationError() }
        return result
    }

    func addMarble(
        name: String, role: String = "", glyph: String, hue: Int,
        workspace: String = "", persona: String = ""
    ) async {
        guard let client else { return }
        do {
            let draft = MarbleClient.MarbleDraft(
                name: name,
                role: role,
                glyph: glyph,
                hue: hue,
                workspace: workspace,
                persona: persona
            )
            _ = try await client.addMarble(draft)
            errorMessage = nil
            prismNotice = "\(draft.name) is here."
        } catch {
            prismNotice = nil
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    /// Edit one marble: name, look, or its personality.
    func updateMarble(_ id: String, update: MarbleClient.MarbleUpdate) async {
        guard let client else { return }
        do {
            _ = try await client.updateMarble(id: id, update)
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    /// Save a marble's personality; the host restarts its conversation so the
    /// next turn actually speaks with the new voice.
    func savePersona(_ marble: MarblePrismMarble, persona: String) async {
        await updateMarble(marble.id, update: .init(persona: persona))
    }

    /// Default-Marble turns this iPhone started from Everyone.
    @Published private(set) var everyoneTaskIds: Set<String> = []

    /// Names to show a thinking bubble for under the Everyone timeline.
    var everyoneThinking: [MarblePrismMarble] {
        let visibleOutcomes = Set(everyoneEntries.filter {
            ["assistant", "failure", "error"].contains($0.kind)
        }.map(\.taskId))
        return prism.marbles.filter { marble in
            let activeIds = activeTaskIds(for: marble)
            let mainIds = activeIds.filter { !threadTaskIds.contains($0) }
            return MarbleEveryoneTimeline.showsThinking(
                working: isAgentWorking(marble) && (activeIds.isEmpty || !mainIds.isEmpty),
                activeTaskIds: mainIds,
                previousTaskIds: (publishedEveryoneWork[marble.id] ?? []).filter { !threadTaskIds.contains($0) },
                visibleOutcomes: visibleOutcomes
            )
        }
    }

    /// The shared plane's timeline, as last fetched from the host.
    var everyoneEntries: [MarbleEntry] {
        let entries = everyoneTimeline.isEmpty ? state.transcript(for: everyoneConversationId) : everyoneTimeline
        return entries.filter { $0.threadId == nil }
    }

    /// Settings → Data: wipe the shared plane's history on the host.
    func clearEveryone() async {
        guard let client else { return }
        do {
            try await client.clearEveryone()
            await refresh()
        } catch {
            prismNotice = "Couldn't clear Everyone: \(error.localizedDescription)"
        }
    }

    /// One message to Everyone; marbles it names answer, Marble answers
    /// otherwise. Returns the plain reply so the composer can say who's busy.
    @discardableResult
    func sendEveryone(_ text: String) async -> MarbleEveryoneReply? {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let client, !trimmed.isEmpty else { return nil }
        do {
            let reply = try await client.sendEveryone(text: trimmed)
            if let taskId = reply.taskId, !taskId.isEmpty { everyoneTaskIds.insert(taskId) }
            errorMessage = nil
            Task { await refresh() }
            return reply
        } catch {
            errorMessage = error.localizedDescription
            return nil
        }
    }

    /// A one-to-one message for one marble: it answers in its own chat, which
    /// is its durable conversation once the first turn creates it.
    func sendToMarble(_ marble: MarblePrismMarble, text: String) async {
        await dispatchSwarm(text: text, marbles: [marble.id])
    }

    /// The marble a transcript row is speaking as, for its label and dot.
    func speakerStyle(for entry: MarbleEntry) -> MarbleSpeakerStyle? {
        MarbleSpeakerStyle.for(entry: entry, marbles: prism.marbles)
    }

    /// A marble's own chat transcript once it has a durable conversation.
    func marbleTranscript(_ marble: MarblePrismMarble) -> [MarbleEntry] {
        let entries = marbleTranscripts[marble.conversationId] ?? state.transcript(for: marble.conversationId)
        return entries.filter { $0.threadId == nil }
    }

    /// Stop the turn a marble is working on in its own chat.
    func stop(_ marble: MarblePrismMarble) async {
        guard let client, !marble.taskId.isEmpty else { return }
        do {
            try await client.cancel(taskId: marble.taskId)
        } catch {
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    func removeMarble(_ marble: MarblePrismMarble) async {
        guard let client else { return }
        do {
            try await client.removeMarble(id: marble.id)
            errorMessage = nil
            prismNotice = "\(marble.name) was removed. Its chat stays readable."
        } catch {
            prismNotice = nil
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    /// Fan one prompt out to the whole roster, or to the marbles named.
    func dispatchSwarm(text: String, marbles: [String] = []) async {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let client, !trimmed.isEmpty else { return }
        do {
            let reply = try await client.dispatchSwarm(text: trimmed, marbles: marbles)
            errorMessage = nil
            prismNotice = (reply.swarm ? "Sent to all Marbles: " : "Sent to ")
                + reply.names.joined(separator: ", ") + "."
        } catch {
            prismNotice = nil
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    func loadTools() async {
        guard let client else { return }
        do {
            toolInventory = try await client.tools()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    /// Read the workspace crate map. A host outside a workspace has none, which
    /// is a plain answer rather than a failure of the sheet.
    func loadCrates() async {
        guard let client else { return }
        cratesLoading = true
        do {
            crates = try await client.crates()
            cratesError = nil
        } catch {
            crates = nil
            cratesError = error.localizedDescription
        }
        cratesLoading = false
    }

    func cancelActiveTask() async {
        guard let client, let taskId = activeTaskId else { return }
        do {
            try await client.cancel(taskId: taskId)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func decide(_ approval: MarbleApproval, allow: Bool, remember: Bool = false) async {
        guard let client else { return }
        do {
            try await client.decide(
                requestId: approval.requestId,
                allow: allow,
                remember: remember
            )
        } catch {
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    // MARK: - Control layer

    func setProfile(_ profile: String) async {
        guard let client else { return }
        do {
            try await client.setProfile(profile)
        } catch {
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    func setPaused(_ paused: Bool) async {
        guard let client else { return }
        do {
            try await client.setPaused(paused)
        } catch {
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    func stopEverything() async {
        guard let client else { return }
        do {
            try await client.stopEverything()
        } catch {
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    func revokeGrant(_ grantId: String) async {
        guard let client else { return }
        do {
            try await client.revokeGrant(grantId)
        } catch {
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    /// Choose the model new conversations use; `nil` means the engine default.
    func setModel(_ model: String?) async {
        guard let client else { return }
        do {
            try await client.setModel(model ?? "")
        } catch {
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    /// Download an export document to a temp file for sharing.
    func exportFile(conversationId: String? = nil) async -> URL? {
        guard let client else { return nil }
        do {
            let data = try await client.export(conversationId: conversationId)
            let name = conversationId == nil
                ? "marble-export.json"
                : "marble-conversation-\(conversationId!.suffix(8)).json"
            let url = FileManager.default.temporaryDirectory.appendingPathComponent(name)
            try data.write(to: url, options: .atomic)
            return url
        } catch {
            errorMessage = error.localizedDescription
            return nil
        }
    }

    func setRoutineQuiet(_ quiet: MarbleQuietHours) async {
        guard let client else { return }
        do {
            try await client.setRoutineQuiet(quiet)
        } catch {
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    func login(token: String) async {
        let trimmed = token.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let client, let baseURL = endpoint?.baseURL else { return }
        do {
            try await client.login(token: trimmed)
            apply(MarbleEndpoint(baseURL: baseURL, token: trimmed))
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    // MARK: - Remembering

    func remember(_ text: String, owner: String = "") async {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let client, !trimmed.isEmpty else { return }
        await perform { _ = try await client.remember(text: trimmed, owner: owner) }
    }

    /// The facts one marble keeps, plus the shared ones every marble gets.
    func memories(for marbleId: String) -> [MarbleMemoryFact] {
        memory.facts.filter { $0.owner.isEmpty || $0.owner == marbleId }
    }

    func rename(_ conversation: MarbleConversation, title: String) async {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let client, !trimmed.isEmpty else { return }
        await perform { try await client.renameConversation(id: conversation.id, title: trimmed) }
    }

    func delete(_ conversation: MarbleConversation) async {
        guard let client else { return }
        do {
            try await client.deleteConversation(id: conversation.id)
            if selectedConversationId == conversation.id {
                conversationTranscript = []
                selectedConversationId = nil
            }
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    func update(_ fact: MarbleMemoryFact, text: String) async {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let client, !trimmed.isEmpty else { return }
        await perform {
            _ = try await client.updateMemory(id: fact.id, text: trimmed, kind: fact.kind)
        }
    }

    func forget(_ fact: MarbleMemoryFact) async {
        guard let client else { return }
        await perform { _ = try await client.forgetMemory(id: fact.id) }
    }

    func accept(_ suggestion: MarbleMemorySuggestion) async {
        guard let client else { return }
        await perform { _ = try await client.acceptSuggestion(id: suggestion.id) }
    }

    func dismiss(_ suggestion: MarbleMemorySuggestion) async {
        guard let client else { return }
        await perform { _ = try await client.dismissSuggestion(id: suggestion.id) }
    }

    // MARK: - Routines

    func addRoutine(title: String, prompt: String, schedule: String, conversationId: String? = nil) async {
        let trimmedPrompt = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let client, !trimmedPrompt.isEmpty else { return }
        let resolvedSchedule = schedule.trimmingCharacters(in: .whitespacesAndNewlines)
        await perform {
            _ = try await client.addRoutine(
                title: title.trimmingCharacters(in: .whitespacesAndNewlines),
                prompt: trimmedPrompt,
                schedule: resolvedSchedule.isEmpty ? "every:1h" : resolvedSchedule,
                conversationId: conversationId ?? self.selectedConversationId
            )
        }
    }

    /// Align `daily:` routines with this phone's timezone.
    func useLocalRoutineTime() async {
        guard let client else { return }
        let minutes = TimeZone.current.secondsFromGMT() / 60
        await perform { try await client.setRoutineOffset(minutes: minutes) }
    }

    func run(_ routine: MarbleRoutine) async {
        guard let client else { return }
        await perform { try await client.runRoutine(id: routine.id) }
    }

    func setRoutine(_ routine: MarbleRoutine, enabled: Bool) async {
        guard let client else { return }
        await perform { try await client.setRoutine(id: routine.id, enabled: enabled) }
    }

    /// Stop one running task from anywhere (Activity's "Running now" list).
    func cancel(_ task: MarbleTask) async {
        guard let client else { return }
        do {
            try await client.cancel(taskId: task.id)
        } catch {
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    func setRoutineCatchUp(_ routine: MarbleRoutine, enabled: Bool) async {
        guard let client else { return }
        do {
            try await client.setRoutineCatchUp(id: routine.id, enabled: enabled)
        } catch {
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    func remove(_ routine: MarbleRoutine) async {
        guard let client else { return }
        await perform { try await client.removeRoutine(id: routine.id) }
    }

    // MARK: - Voice and push

    func toggleVoice() async {
        guard let client else { return }
        let wasEnabled = voice.enabled
        await perform { _ = try await client.setVoice(enabled: !wasEnabled) }
        if wasEnabled { speaker.stop() }
    }

    func speakLastReply(_ text: String) {
        Task { await speak(text) }
    }

    private func speak(_ text: String) async {
        guard let client, voice.enabled, !text.isEmpty else { return }
        var spoken = text
        if let shaped = try? await client.speak(text: text), !shaped.isEmpty {
            spoken = shaped
        }
        speaker.speak(spoken, rate: Float(voice.rate))
    }

    // MARK: - Local notifications

    func requestLocalNotificationPermission() async -> Bool {
        let settings = await UNUserNotificationCenter.current().notificationSettings()
        if settings.authorizationStatus == .authorized { return true }
        return (try? await UNUserNotificationCenter.current()
            .requestAuthorization(options: [.alert, .sound, .badge])) ?? false
    }

    private func handleReply(_ event: MarbleEvent, liveAfter: UInt64?) {
        let previous = notificationCursor
        let reply = notificationCursor.consume(event, liveAfter: liveAfter)
        if previous != notificationCursor { saveNotificationCursor() }
        let conversationId = event.conversationId
        let threadId = event.data.threadId
        guard let reply, localNotificationsEnabled,
              appInBackground ? backgroundConnection.isKeepingAlive
                  : shouldPresentReply?(conversationId, threadId) == true else { return }
        notificationCursor.notified(reply.entry)
        saveNotificationCursor()
        let content = UNMutableNotificationContent()
        content.title = reply.title
        content.body = String(reply.text.prefix(180))
        content.sound = .default
        var marble = ["conversationId": conversationId]
        marble["threadId"] = threadId
        content.userInfo = ["marble": marble]
        let request = UNNotificationRequest(
            identifier: "marble.\(reply.entry.conversationId).\(reply.entry.id)",
            content: content,
            trigger: nil
        )
        UNUserNotificationCenter.current().add(request)
    }

    private func perform(_ operation: () async throws -> Void) async {
        do {
            try await operation()
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    // MARK: - Streaming

    private func listen() {
        stopStreaming()
        status.reconnectStarted()
        let client = self.client
        let generation = hostGeneration.current
        streamTask = Task { [weak self] in
            var generation = generation
            while !Task.isCancelled {
                guard let self, let client else { return }
                var endedReason: String? = nil
                do {
                    let health = try await client.health(timeout: 3)
                    guard !Task.isCancelled, self.hostGeneration.isCurrent(generation) else { return }
                    self.prepareForStream(health)
                    generation = self.hostGeneration.current
                    var liveAfter: UInt64?
                    stream: for try await update in client.events(since: self.sequence) {
                        guard !Task.isCancelled, self.hostGeneration.isCurrent(generation) else { return }
                        switch update {
                        case .opened:
                            self.status.streamOpened()
                            self.backoffSeconds = 1
                            self.threadHostRevision &+= 1
                            let openedGeneration = generation
                            // A newly restarted host may not send any live event.
                            Task {
                                guard self.hostGeneration.isCurrent(openedGeneration) else { return }
                                await self.refresh()
                            }
                        case .activity:
                            self.status.streamEvent()
                            if let connectedAt = self.streamConnectedAt {
                                self.backgroundConnection.refreshActivity(connectedAt: connectedAt)
                            }
                        case .baseline(let cursor):
                            // A restart can land between /healthz and /api/events.
                            if cursor < self.sequence { break stream }
                            liveAfter = cursor
                            self.streamConnectedAt = .now
                            Self.streamLog.debug("SSE connected")
                            self.updateBackgroundConnection()
                        case .event(let event):
                            self.status.streamEvent()
                            self.handle(event, liveAfter: liveAfter)
                        }
                    }
                } catch MarbleClientError.unauthorized {
                    guard !Task.isCancelled, self.hostGeneration.isCurrent(generation) else { return }
                    self.markUnauthorized()
                    return
                } catch {
                    guard !Task.isCancelled, self.hostGeneration.isCurrent(generation) else { return }
                    endedReason = error.localizedDescription
                    self.markOffline(error)
                }
                if Task.isCancelled { return }
                self.streamDisconnected()
                self.status.streamEnded(endedReason)
                self.loaded = true
                if self.appInBackground {
                    self.streamTask = nil
                    return
                }
                status.reconnectStarted()
                let delay = self.backoffSeconds
                self.backoffSeconds = min(self.backoffSeconds * 2, 4)
                try? await Task.sleep(for: .seconds(delay))
            }
        }
    }

    private func prepareForStream(_ health: MarbleHealth) {
        let resume = MarbleResumeCursor.resume(
            saved: sequence, hostSequence: health.sequence, previousPID: hostPID, hostPID: health.pid
        )
        sequence = resume.since
        if resume.restarted {
            // Keep cached rows visible while fetching fresh transcripts.
            hostGeneration.advance()
            resetChatActivity()
            live = [:]
            syncLive()
            transcriptRevision = -1
            transcriptCursor = 0
            chatsRevision = -1
            saveCursor()
        }
        if let pid = health.pid, hostPID != pid {
            hostPID = pid
            if let endpoint {
                UserDefaults.standard.set(pid, forKey: Self.hostPIDKeyPrefix + endpoint.baseURL.absoluteString)
            }
        }
    }

    /// Mirror the selected conversation's buffers into the published fields.
    private func syncLive() {
        let buffers = live[selectedConversationId ?? ""] ?? LiveBuffers()
        liveText = buffers.text
        liveReasoning = buffers.reasoning
        activeTaskId = buffers.taskId
    }

    private func recordTool(_ event: MarbleEvent) {
        guard !event.taskId.isEmpty else { return }
        var calls = tools(for: event.taskId)
        MarbleToolActivity.apply(event, to: &calls)
        guard !calls.isEmpty else { return }
        if toolsByTurn[event.taskId] == nil {
            if recentToolTurnIDs.count >= 64 {
                toolsByTurn.removeValue(forKey: recentToolTurnIDs.removeFirst())
            }
            recentToolTurnIDs.append(event.taskId)
        }
        toolsByTurn[event.taskId] = calls
    }

    private func recordReasoning(_ text: String, taskId: String) {
        guard !taskId.isEmpty, !text.isEmpty else { return }
        if reasoningByTurn[taskId] == nil {
            if reasoningTurnIds.count >= 64 {
                reasoningByTurn.removeValue(forKey: reasoningTurnIds.removeFirst())
            }
            reasoningTurnIds.append(taskId)
        }
        reasoningByTurn[taskId, default: ""] += text
    }

    private func handle(_ event: MarbleEvent, liveAfter: UInt64? = nil) {
        guard event.sequence > sequence else { return }
        sequence = event.sequence
        if event.kind == "reply" {
            handleReply(event, liveAfter: liveAfter)
            saveCursor()
            if let threadId = event.data.threadId {
                threadEventSequences[threadId] = event.sequence
            }
            Task { await refresh() }
            return
        }
        if let threadId = event.data.threadId {
            if !event.taskId.isEmpty, threadTaskIds.insert(event.taskId).inserted,
               live[event.conversationId]?.taskId == event.taskId {
                live.removeValue(forKey: event.conversationId)
                if selectedConversationId == event.conversationId { syncLive() }
            }
            if ["everyone", "settled"].contains(event.kind) {
                threadEventSequences[threadId] = event.sequence
            }
            if ["state", "approval_pending", "approval_resolved", "error", "settled"].contains(event.kind) {
                Task { await refresh() }
            }
            if event.kind == "settled" { saveCursor() }
            return
        }
        var buffers = live[event.conversationId] ?? LiveBuffers()
        let forActive = event.conversationId.isEmpty
            || event.conversationId == (selectedConversationId ?? "")
        switch event.kind {
        case "text":
            buffers.text += event.data.text ?? ""
        case "reasoning":
            let text = event.data.text ?? ""
            if !event.taskId.isEmpty && buffers.taskId == nil {
                buffers.taskId = event.taskId
            }
            if buffers.taskId == event.taskId || event.taskId.isEmpty {
                buffers.reasoning += text
            }
            recordReasoning(text, taskId: event.taskId)
        case "tool_start", "tool_end":
            recordTool(event)
        case "approval_pending":
            Task { await refresh() }
        case "approval_resolved", "state":
            Task { await refresh() }
        case "everyone":
            everyoneTimeline = MarbleEveryoneTimeline.applying(event, to: everyoneTimeline)
            if everyoneTimeline.isEmpty { saveCache() }
            chatsRevision = -1
            Task { await refresh() }
        case "thread":
            if let thread = event.data.thread {
                threadUpdates[thread.id] = thread
                threadEventSequences[thread.id] = event.sequence
            }
        case "settled":
            guard buffers.taskId == nil || event.taskId.isEmpty || buffers.taskId == event.taskId else { break }
            let reply = forActive ? buffers.text : ""
            if !event.taskId.isEmpty, !event.conversationId.isEmpty, event.data.cancelled != true {
                awaitingTranscript[event.conversationId] = event.taskId
            }
            buffers = LiveBuffers()
            saveCursor()
            Task {
                await refresh()
                if !reply.isEmpty { await speak(reply) }
            }
        case "error":
            if event.taskId.isEmpty {
                errorMessage = "Marble couldn’t finish the request."
            } else if buffers.taskId == event.taskId {
                buffers = LiveBuffers()
            }
            Task { await refresh() }
        case "cancel_requested":
            errorMessage = nil
        case "turn_start":
            if !event.taskId.isEmpty && buffers.taskId != event.taskId {
                buffers = LiveBuffers(taskId: event.taskId)
            }
        default:
            break
        }
        live[event.conversationId] = buffers.isEmpty ? nil : buffers
        if forActive { syncLive() }
    }

    private func markUnauthorized() {
        needsToken = true
        stopStreaming()
    }

    private func markOffline(_ error: Error) {
        if let clientError = error as? MarbleClientError, clientError == .unauthorized {
            markUnauthorized()
            return
        }
        // iOS drops connections in the background; that is not news.
        guard !appInBackground, !status.suspended else { return }
        errorMessage = error.localizedDescription
    }
}

#if DEBUG
extension MarbleStore {
    /// In-memory fixture for XCUITest runs (`-MarbleUITestFixture`): a seeded
    /// host state and a timer that keeps "streaming" so scroll-follow and
    /// navigation can be exercised without any network. Debug builds only;
    /// release binaries must not contain this symbol.
    static func uiTestFixture() -> MarbleStore {
        let store = MarbleStore()
        store.onboarded = true
        store.loaded = true
        store.status.streamOpened()
        var state = MarbleState.empty
        let now = UInt64(Date().timeIntervalSince1970)
        state.conversations = (0..<200).map { index in
            MarbleConversation(
                id: "fixture-\(index)",
                title: "Fixture conversation \(index)",
                taskIds: ["task-\(index)"],
                updatedAt: now - UInt64(index * 60)
            )
        }
        state.transcript = (0..<300).map { index in
            MarbleEntry(
                conversationId: "fixture-0",
                kind: index.isMultiple(of: 2) ? "user" : "assistant",
                text: "Fixture message \(index)",
                taskId: "task-0",
                at: now - UInt64((300 - index) * 5)
            )
        }
        // A second chat carries a short transcript so row-navigation tests
        // can assert its own messages appear, not conversation 0's.
        for index in 0..<3 {
            state.transcript.append(MarbleEntry(
                conversationId: "fixture-2", kind: index == 0 ? "user" : "assistant",
                text: "Fixture message \(index)", taskId: "task-2", at: now - 100 + UInt64(index)
            ))
        }
        state.transcript.insert(
            MarbleEntry(conversationId: "fixture-0", kind: "reasoning",
                        text: "Working out the fixture reply step by step.",
                        taskId: "task-0", at: now - 10),
            at: 200
        )
        // A cancelled turn so the "Cancelled before replying" footnote shows.
        state.tasks.append(MarbleTask(
            id: "task-cancelled", conversationId: "fixture-0", status: "cancelled",
            owner: "agent", prompt: "a cancelled fixture turn", at: now - 5
        ))
        // Activity: one approval waiting, one running turn, recent decisions.
        state.approvals = [
            MarbleApproval(
                requestId: "req-fixture-1", callId: "call-1", tool: "glass.write",
                arguments: .object(["path": .string("Sources/App.swift"), "content": .string("…")]),
                conflict: nil
            )
        ]
        state.tasks.append(MarbleTask(
            id: "task-running", conversationId: "fixture-1", status: "running",
            owner: "agent", prompt: "renaming the fixture files", at: now - 30
        ))
        state.control = MarbleControl(
            profile: "workspace",
            paused: false,
            grants: [
                MarbleGrant(
                    id: "grant-1", tool: "glass.read",
                    pathPrefix: "", createdAt: now - 3600, uses: 12
                )
            ],
            deniedPaths: ["/private/keys"],
            ledger: [
                MarbleControlEntry(
                    at: now - 600, conversationId: "fixture-0", taskId: "task-0",
                    tool: "glass.read", path: "README.md", decision: "allowed",
                    reason: "read profile"
                ),
                MarbleControlEntry(
                    at: now - 300, conversationId: "fixture-0", taskId: "task-0",
                    tool: "glass.write", path: "/private/keys/x", decision: "denied",
                    reason: "off-limits folder"
                ),
            ],
            queue: [],
            profiles: [
                MarbleProfileOption(
                    id: "observe", label: "Read only",
                    description: "read the workspace", allowsWrites: false, prompts: false
                ),
                MarbleProfileOption(
                    id: "workspace", label: "Ask before changes",
                    description: "ask first", allowsWrites: true, prompts: true
                ),
                MarbleProfileOption(
                    id: "unattended", label: "Don't ask",
                    description: "act without asking", allowsWrites: true, prompts: false
                ),
            ]
        )
        state.memory = MarbleMemory(
            facts: [
                MarbleMemoryFact(
                    id: "fact-1", text: "The project builds with Swift 6.4.2.",
                    kind: "fact", createdAt: now - 7200, uses: 3
                )
            ],
            suggestions: [
                MarbleMemorySuggestion(
                    id: "sug-1", text: "Azhar prefers concise replies.",
                    kind: "preference", reason: "seen in three conversations"
                )
            ],
            count: 1, limit: nil
        )
        state.routines = MarbleRoutines(
            routines: [
                MarbleRoutine(
                    id: "routine-1", title: "Morning check",
                    prompt: "Summarise overnight builds", schedule: "daily:09:30",
                    conversationId: nil, enabled: true, createdAt: now - 86400,
                    lastRunAt: now - 3600, nextRunAt: now + 50400, runs: 7,
                    lastTaskId: "task-9", lastStatus: "completed",
                    missedRuns: nil, missedLastGap: nil, catchUp: true
                ),
                MarbleRoutine(
                    id: "routine-2", title: "Frequent sweep",
                    prompt: "Sweep the fixture workspace", schedule: "every:2h",
                    conversationId: nil, enabled: false, createdAt: now - 172800,
                    lastRunAt: 0, nextRunAt: 0, runs: 0,
                    lastTaskId: "", lastStatus: "",
                    missedRuns: nil, missedLastGap: nil, catchUp: nil
                ),
            ],
            count: 2, limit: nil, enabled: 1,
            offsetMinutes: 0, quietHours: .off
        )
        // Two named marbles plus the default: the roster, the Everyone
        // timeline, and one ask between them, all without a network.
        state.prism = MarblePrism(
            marbles: [
                MarblePrismMarble(
                    id: "marble", name: "Marble", role: "the default assistant",
                    glyph: "circle", hue: 210, workspace: "", state: "idle",
                    conversationId: "", taskId: "", lastAt: 0, runs: 3,
                    lastError: "", persona: "", isDefault: true
                ),
                MarblePrismMarble(
                    id: "m-ada", name: "Ada", role: "",
                    glyph: "star", hue: 45, workspace: "", state: "idle",
                    conversationId: "fixture-ada", taskId: "", lastAt: 0, runs: 2,
                    lastError: "",
                    persona: "Warm and encouraging; celebrates progress and explains things simply.",
                    isDefault: false
                ),
                MarblePrismMarble(
                    id: "m-ben", name: "Ben", role: "",
                    glyph: "hex", hue: 145, workspace: "", state: "working",
                    conversationId: "fixture-ben", taskId: "task-ben-1", lastAt: 0,
                    runs: 5, lastError: "",
                    persona: "Careful and methodical; checks work twice and says what it verified.",
                    isDefault: false
                ),
            ],
            limit: 8, swarms: 0, dispatched: 4, working: 1, engine: true
        )
        state.tasks.append(MarbleTask(
            id: "task-ben-1", conversationId: "fixture-ben", status: "running",
            owner: "prism", prompt: "checking the fixture tests", at: now - 20
        ))
        state.conversations.append(MarbleConversation(
            id: "everyone", title: "Everyone",
            taskIds: [], updatedAt: now
        ))
        state.transcript.append(contentsOf: [
            MarbleEntry(
                conversationId: "everyone", kind: "user",
                text: "@Ada can you look at the failing tests?",
                taskId: "", at: now - 40, speaker: "user", speakerName: "You"
            ),
            MarbleEntry(
                conversationId: "everyone", kind: "handoff",
                text: "Ada asked Ben to check the failing tests.",
                taskId: "task-ada-1", at: now - 35,
                speaker: "m-ada", speakerName: "Ada",
                handoff: MarbleHandoff(
                    from: "m-ada", fromName: "Ada", to: "m-ben", toName: "Ben",
                    instruction: "check the failing tests",
                    status: "asked", summary: ""
                )
            ),
            MarbleEntry(
                conversationId: "everyone", kind: "handoff",
                text: "Ben finished: three tests were flaky; I re-ran them.",
                taskId: "task-ada-1", at: now - 30,
                speaker: "m-ada", speakerName: "Ada",
                handoff: MarbleHandoff(
                    from: "m-ada", fromName: "Ada", to: "m-ben", toName: "Ben",
                    instruction: "check the failing tests",
                    status: "finished",
                    summary: "three tests were flaky; I re-ran them"
                )
            ),
            MarbleEntry(
                conversationId: "everyone", kind: "assistant",
                text: "Ben checked the fixture tests — three were flaky and are re-running now.",
                taskId: "task-ada-1", at: now - 28,
                speaker: "m-ada", speakerName: "Ada"
            ),
        ])
        state.transcript.append(contentsOf: [
            MarbleEntry(
                conversationId: "fixture-ada", kind: "user",
                text: "hello Ada", taskId: "task-ada-0", at: now - 50,
                speaker: "user", speakerName: "You"
            ),
            MarbleEntry(
                conversationId: "fixture-ada", kind: "assistant",
                text: "Hello! Lovely to meet you — what shall we work on?",
                taskId: "task-ada-0", at: now - 48,
                speaker: "m-ada", speakerName: "Ada"
            ),
        ])
        store.state = state
        store.conversationTranscript = state.transcript
        store.transcriptConversationId = "fixture-0"
        // A turn keeps emitting text so streaming-follow behavior is testable.
        var tick = 0
        Timer.scheduledTimer(withTimeInterval: 0.4, repeats: true) { [weak store] _ in
            guard let store else { return }
            tick += 1
            store.liveText = "Streaming fixture reply \(tick) " + String(repeating: "…", count: tick % 40)
        }
        return store
    }
}
#endif
