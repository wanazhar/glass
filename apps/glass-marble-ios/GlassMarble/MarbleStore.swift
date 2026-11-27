import ActivityKit
import Foundation
import Network
import OSLog
import SwiftUI
import UserNotifications

/// Observable product store shared by every iOS surface.
@MainActor
final class MarbleStore: ObservableObject {
    #if DEBUG
    private var fixtureThreads: [String: MarbleThreadPage]?
    private var fixturePendingEveryone: [MarbleEntry]?
    private var fixtureEveryoneSend = false
    #endif
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

    private struct GroupReplyPreview: Equatable {
        var text = ""
        // Previously rendered blocks provide the bubble's minimum height
        // while the current block starts streaming.
        var retracted: [String] = []
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
            Task { await self.loadTranscript() }
        }
    }
    @Published private var transcriptHistoryLoadedFor: String?
    private var transcriptRevision: Int = -1
    /// Everyone and each marble's own chat, paged from the host: a lean state
    /// poll carries no transcript, so these never come from `state`.
    @Published private(set) var everyoneTimeline: [MarbleEntry] = []
    @Published private var everyoneHistoryLoaded = false
    @Published private(set) var marbleTranscripts: [String: [MarbleEntry]] = [:]
    @Published private(set) var historyBefore: [String: String] = [:]
    @Published private(set) var threadPages: [String: MarbleThreadPage] = [:]
    private var loadingOlder: [String: String] = [:]
    private var loadingOlderThreads: [String: String] = [:]
    private var deletedConversationIDs: Set<String> = []
    private var conversationDeletions: [String: UInt64] = [:]
    @Published private(set) var historyErrors: Set<String> = []
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
    @Published private(set) var hostBuild: MarbleHealth.Build?
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
    var shouldPresentReply: ((String, String?, Bool) -> Bool)?

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
        transcript(for: selectedConversationId)
    }

    func transcript(for id: String?) -> [MarbleEntry] {
        guard let id else { return [] }
        return (marbleTranscripts[id] ?? state.transcript(for: id)).filter { $0.threadId == nil }
    }

    var isWorking: Bool { activeTaskId != nil }

    var everyoneHistoryLoading: Bool {
        #if DEBUG
        if fixturePendingEveryone != nil { return true }
        #endif
        return !everyoneHistoryLoaded && client != nil && !status.isOffline
            && !historyErrors.contains(everyoneConversationId)
    }

    func transcriptHistoryLoading(_ conversationId: String?) -> Bool {
        guard let conversationId, client != nil, !status.isOffline else { return false }
        return transcriptHistoryLoadedFor != conversationId && !historyErrors.contains(conversationId)
    }

    func marbleHistoryLoading(_ marble: MarblePrismMarble) -> Bool {
        !marble.conversationId.isEmpty && marbleTranscripts[marble.conversationId] == nil
            && client != nil && !status.isOffline && !historyErrors.contains(marble.conversationId)
    }

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
        hostBuild = nil
        engineHealth = nil
        setConnectionPaused(false)
        self.endpoint = endpoint
        MarblePreferences.save(endpoint)
        client = MarbleClient(endpoint: endpoint)
        onboarded = true
        sequence = 0
        hostPID = nil
        notificationCursor = MarbleNotificationCursor()
        needsToken = false
        transcriptHistoryLoadedFor = nil
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
        hostBuild = nil
        engineHealth = nil
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
        transcriptHistoryLoadedFor = nil
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
        // The live stream owns retries while background keep-alive is armed.
        guard force || !backgroundConnection.isKeepingAlive else { return }
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
        guard wasBackgrounded, streamTask == nil, !connectionPaused else { return }
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

    func sceneDisconnected() {
        appInBackground = true
        status.suspend()
        stopStreaming()
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
        let keepAlive = backgroundConnectionEnabled && !connectionPaused && !needsToken
            && (streamConnectedAt != nil || backgroundConnection.isKeepingAlive)
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

    private func streamDisconnected(keepingAlive: Bool = false) {
        let wasConnected = streamConnectedAt != nil
        streamConnectedAt = nil
        if wasConnected { Self.streamLog.debug("SSE disconnected") }
        if keepingAlive {
            if wasConnected { backgroundConnection.showReconnecting() }
            return
        }
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
        historyBefore = cache.before ?? [:]
        everyoneTimeline = cachedEntries(cache.everyone, for: everyoneConversationId)
        marbleTranscripts = Dictionary(uniqueKeysWithValues: cache.transcripts.map { id, entries in
            (id, cachedEntries(entries, for: id))
        })
        threadPages = cache.threads ?? [:]
    }

    private func saveCache(discardOnFailure: Bool = false) {
        guard let endpoint, let url = Self.cacheURL(for: endpoint) else { return }
        var cache = MarbleChatCache(everyone: [], transcripts: [:], before: [:])
        for (id, entries) in marbleTranscripts {
            let (rows, before) = MarbleHistoryWindow.cached(entries, before: historyBefore[id])
            cache.transcripts[id] = rows
            cache.before?[id] = before
        }
        let (rows, before) = MarbleHistoryWindow.cached(
            everyoneTimeline, before: historyBefore[everyoneConversationId]
        )
        cache.everyone = rows
        cache.before?[everyoneConversationId] = before
        cache.threads = threadPages.mapValues { page in
            var snapshot = page
            (snapshot.entries, snapshot.nextBefore) = MarbleHistoryWindow.cached(page.entries, before: page.nextBefore)
            return snapshot
        }
        do {
            try cache.encoded().write(to: url, options: .atomic)
        } catch {
            if discardOnFailure { try? FileManager.default.removeItem(at: url) }
        }
    }

    private func cachedEntries(_ entries: [MarbleEntry], for id: String) -> [MarbleEntry] {
        let (rows, before) = MarbleHistoryWindow.cached(entries, before: historyBefore[id])
        historyBefore[id] = before
        return rows
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

    private var refreshing = false
    private var needsRefresh = false

    func refresh() async {
        needsRefresh = true
        guard !refreshing else { return }
        refreshing = true
        while needsRefresh {
            needsRefresh = false
            await refreshPass()
        }
        refreshing = false
    }

    private func refreshPass() async {
        guard let client else { return }
        let generation = hostGeneration.current
        do {
            async let healthFetch = client.health()
            var snapshot = try await client.state()
            guard hostGeneration.isCurrent(generation) else { return }
            removeDeletedConversations(from: &snapshot)
            let previousChats = Set(state.conversations.map(\.id))
            if !previousChats.isSubset(of: Set(snapshot.conversations.map(\.id))) {
                invalidateEveryoneHistory()
            }
            if state != snapshot { state = snapshot }
            let finished = Set(snapshot.tasks.filter { !$0.isRunning && $0.status != "queued" }.map(\.id))
            if !everyoneTaskIds.isDisjoint(with: finished) { everyoneTaskIds.subtract(finished) }
            needsToken = false
            errorMessage = nil
            status.refreshSucceeded()
            if let health = try? await healthFetch {
                guard hostGeneration.isCurrent(generation) else { return }
                updateHostHealth(health)
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
            if buffers.taskId != selectedTask && (selectedTask != nil || buffers.text.isEmpty) {
                buffers = selectedTask.map { taskId -> LiveBuffers in
                    var b = buffers
                    b.taskId = taskId
                    return b
                } ?? LiveBuffers()
                live[selectedId] = buffers.isEmpty ? nil : buffers
                syncLive()
            }
            if !loaded { loaded = true }
            async let selectedHistory: Void = loadTranscript()
            await loadChats()
            await selectedHistory
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
        outgoing = []
        draftResolutions = [:]
        everyoneTimeline = []
        everyoneHistoryLoaded = false
        marbleTranscripts = [:]
        historyBefore = [:]
        threadPages = [:]
        loadingOlder = [:]
        loadingOlderThreads = [:]
        deletedConversationIDs = []
        conversationDeletions = [:]
        everyoneHistoryRevision = -1
        everyoneHistoryGeneration = 0
        everyoneHistoryNeedsReload = false
        everyoneHistoryRequest = nil
        marbleHistoryRevisions = [:]
        historyErrors = []
        chatsRevision = -1
    }

    private func resetChatActivity() {
        everyoneTaskIds = []
        toolsByTurn = [:]
        settledToolTasks = []
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
        liveEntries = [:]
        automaticRetries = []
        groupReplyPreviews = [:]
        privateReplySizing = [:]
        streamedReplies = [:]
        streamedTaskIDs = []
        narration = MarbleReplyNarration()
        if narratedTaskId != nil { speaker.stop(); narratedTaskId = nil }
        pendingEventRefresh?.cancel()
        pendingEventRefresh = nil
    }

    /// Fetch the Everyone timeline and every marble's own chat once per
    /// document revision.
    private func loadChats() async {
        guard client != nil, chatsRevision != state.revision || everyoneHistoryNeedsReload else { return }
        let revision = state.revision
        let generation = hostGeneration.current
        let marbleIds = Set(prism.marbles.filter {
            marbleTranscripts[$0.conversationId] == nil || isAgentWorking($0)
                || $0.conversationId == displayedMarbleConversation
                || awaitingTranscript[$0.conversationId] != nil || liveEntries[$0.conversationId] != nil
        }.map(\.conversationId)).subtracting(["", selectedConversationId ?? ""])
        await withTaskGroup(of: Void.self) { group in
            for id in marbleIds {
                group.addTask { await self.loadMarbleHistory(id, revision: revision) }
            }
            group.addTask { await self.loadEveryoneHistory(revision: revision) }
        }
        guard hostGeneration.isCurrent(generation), chatsRevision <= revision else { return }
        chatsRevision = revision
        publishedEveryoneWork = Dictionary(uniqueKeysWithValues: prism.marbles.compactMap { marble in
            guard isAgentWorking(marble) else { return nil }
            let tasks = activeTaskIds(for: marble)
            return tasks.isEmpty ? nil : (marble.id, tasks)
        })
        saveCache()
    }

    private var everyoneHistoryRevision = -1
    private var everyoneHistoryGeneration: UInt64 = 0
    private var everyoneHistoryNeedsReload = false
    private var everyoneHistoryRequest: UUID?
    private var marbleHistoryRevisions: [String: Int] = [:]

    private func invalidateEveryoneHistory() {
        // A newest page only proves its own contents. An overlapping
        // prefix can still contain mirrors removed by deleting a chat.
        everyoneHistoryGeneration &+= 1
        everyoneHistoryNeedsReload = true
        everyoneHistoryRequest = nil
    }

    private func loadEveryoneHistory(revision: Int, resetWindow: Bool = false) async {
        guard let client else { return }
        let generation = hostGeneration.current
        let historyGeneration = everyoneHistoryGeneration
        let request = UUID()
        everyoneHistoryRequest = request
        defer {
            if everyoneHistoryRequest == request { everyoneHistoryRequest = nil }
        }
        do {
            let page = try await client.everyone()
            guard hostGeneration.isCurrent(generation), everyoneHistoryGeneration == historyGeneration,
                  everyoneHistoryRevision <= revision else { return }
            if historyErrors.contains(everyoneConversationId) { historyErrors.remove(everyoneConversationId) }
            updateHistory(everyoneConversationId, entries: page.entries,
                          nextBefore: page.hasMore == true ? page.nextBefore : nil,
                          reset: resetWindow || everyoneHistoryNeedsReload)
            everyoneHistoryNeedsReload = false
            confirmEntries(page.entries, conversationId: everyoneConversationId)
            confirmOutgoing(page.entries, to: .everyone())
            if !everyoneHistoryLoaded { everyoneHistoryLoaded = true }
            everyoneHistoryRevision = revision
        } catch {
            guard hostGeneration.isCurrent(generation), everyoneHistoryGeneration == historyGeneration,
                  everyoneHistoryRevision <= revision else { return }
            if !historyErrors.contains(everyoneConversationId) { historyErrors.insert(everyoneConversationId) }
            errorMessage = error.localizedDescription
        }
    }

    func openEveryoneHistory() async {
        #if DEBUG
        if let pending = fixturePendingEveryone {
            try? await Task.sleep(for: .seconds(5))
            guard !Task.isCancelled else { return }
            everyoneTimeline = pending
            fixturePendingEveryone = nil
            everyoneHistoryLoaded = true
            return
        }
        #endif
        guard !everyoneHistoryLoaded, everyoneHistoryRequest == nil else { return }
        await loadEveryoneHistory(revision: state.revision)
    }

    func openMarbleHistory(_ marble: MarblePrismMarble) async {
        guard !marble.conversationId.isEmpty else { return }
        displayedMarbleConversation = marble.conversationId
        if streamedReplies[marble.conversationId] == nil,
           let buffer = live[marble.conversationId],
           let taskId = buffer.taskId, !buffer.text.isEmpty {
            presentStreamedReply(buffer.text, in: marble.conversationId, taskId: taskId)
        }
        await loadMarbleHistory(marble.conversationId, revision: state.revision)
    }

    private var displayedMarbleConversation: String?

    func closeMarbleHistory(_ id: String) {
        if displayedMarbleConversation == id { displayedMarbleConversation = nil }
    }

    private func loadMarbleHistory(_ id: String, revision: Int, resetWindow: Bool = false) async {
        guard let client, !deletedConversationIDs.contains(id) else { return }
        let generation = hostGeneration.current
        do {
            let page = try await client.transcript(conversationId: id)
            guard hostGeneration.isCurrent(generation), !Task.isCancelled,
                  !deletedConversationIDs.contains(id),
                  (marbleHistoryRevisions[id] ?? -1) <= revision else { return }
            if historyErrors.contains(id) { historyErrors.remove(id) }
            updateHistory(id, entries: page.entries, nextBefore: page.hasMore == true ? page.nextBefore : nil,
                          reset: resetWindow)
            confirmEntries(page.entries, conversationId: id)
            if let marble = prism.marbles.first(where: { $0.conversationId == id && !$0.isDefault }) {
                confirmOutgoing(page.entries, to: .marble(marble.id))
            }
            marbleHistoryRevisions[id] = revision
            if state.runningTask(for: id) == nil { awaitingTranscript.removeValue(forKey: id) }
        } catch MarbleClientError.notFound {
            // A named Marble can exist before its first conversation is ready.
            guard hostGeneration.isCurrent(generation), !Task.isCancelled,
                  !deletedConversationIDs.contains(id),
                  (marbleHistoryRevisions[id] ?? -1) <= revision else { return }
            if marbleTranscripts[id] == nil { marbleTranscripts[id] = [] }
        } catch {
            guard hostGeneration.isCurrent(generation), !Task.isCancelled,
                  !deletedConversationIDs.contains(id) else { return }
            if !historyErrors.contains(id) { historyErrors.insert(id) }
            errorMessage = error.localizedDescription
        }
    }

    /// Refresh the selected chat's latest page, retaining any loaded older pages.
    private func loadTranscript(resetWindow: Bool = false) async {
        guard let client else { return }
        guard let conversationId = selectedConversationId else {
            transcriptHistoryLoadedFor = nil
            transcriptRevision = -1
            return
        }
        if transcriptHistoryLoadedFor != conversationId {
            transcriptRevision = -1
        }
        guard resetWindow || transcriptRevision != state.revision else { return }
        let generation = hostGeneration.current
        let revision = state.revision
        do {
            let page = try await client.transcript(conversationId: conversationId)
            guard hostGeneration.isCurrent(generation), selectedConversationId == conversationId,
                  transcriptRevision <= revision else { return }
            if historyErrors.contains(conversationId) { historyErrors.remove(conversationId) }
            updateHistory(conversationId, entries: page.entries,
                          nextBefore: page.hasMore == true ? page.nextBefore : nil, reset: resetWindow)
            confirmEntries(page.entries, conversationId: conversationId)
            confirmOutgoing(page.entries, to: .conversation(conversationId))
            transcriptRevision = revision
            if transcriptHistoryLoadedFor != conversationId { transcriptHistoryLoadedFor = conversationId }
            if state.runningTask(for: conversationId) == nil {
                awaitingTranscript.removeValue(forKey: conversationId)
            }
        } catch MarbleClientError.notFound {
            guard hostGeneration.isCurrent(generation), selectedConversationId == conversationId else { return }
            purgeConversation(conversationId)
            errorMessage = "This chat no longer exists"
        } catch {
            guard hostGeneration.isCurrent(generation), selectedConversationId == conversationId else { return }
            if !historyErrors.contains(conversationId) { historyErrors.insert(conversationId) }
            errorMessage = error.localizedDescription
        }
    }

    private func historyEntries(_ id: String) -> [MarbleEntry] {
        id == everyoneConversationId ? everyoneTimeline : marbleTranscripts[id] ?? []
    }

    private func setHistory(_ entries: [MarbleEntry], for id: String, before: String?) {
        if historyBefore[id] != before { historyBefore[id] = before }
        if id == everyoneConversationId {
            if everyoneTimeline != entries { everyoneTimeline = entries }
        } else if marbleTranscripts[id] != entries {
            marbleTranscripts[id] = entries
        }
    }

    private func updateHistory(_ id: String, entries: [MarbleEntry], nextBefore: String?, reset: Bool) {
        let (rows, cursor) = MarbleHistoryWindow.refreshing(
            historyEntries(id), before: historyBefore[id], with: entries, nextBefore: nextBefore, invalidated: reset
        )
        setHistory(rows, for: id, before: cursor)
    }

    func loadOlderHistory(_ id: String, beforePrepend: () -> Void = {}) async {
        guard let client, let before = historyBefore[id], loadingOlder[id] == nil,
              !deletedConversationIDs.contains(id),
              id != everyoneConversationId || !everyoneHistoryNeedsReload else { return }
        let generation = hostGeneration.current
        let historyGeneration = everyoneHistoryGeneration
        loadingOlder[id] = before
        defer {
            if hostGeneration.isCurrent(generation) { loadingOlder[id] = nil }
        }
        do {
            let entries: [MarbleEntry]
            let next: String?
            let paged: Bool
            if id == everyoneConversationId {
                let page = try await client.everyone(before: before)
                (entries, next, paged) = (page.entries, page.hasMore == true ? page.nextBefore : nil, page.hasMore != nil)
            } else {
                let page = try await client.transcript(conversationId: id, before: before)
                (entries, next, paged) = (page.entries, page.hasMore == true ? page.nextBefore : nil, page.hasMore != nil)
            }
            guard hostGeneration.isCurrent(generation), historyBefore[id] == before,
                  id != everyoneConversationId || everyoneHistoryGeneration == historyGeneration else { return }
            if paged {
                let current = historyEntries(id)
                let rows = MarbleHistoryWindow.prepending(entries, to: current)
                if rows.count > current.count { beforePrepend() }
                setHistory(rows, for: id, before: next)
            } else {
                await reloadNewestHistory(id)
            }
        } catch MarbleClientError.unknownHistoryCursor {
            guard hostGeneration.isCurrent(generation), historyBefore[id] == before,
                  id != everyoneConversationId || everyoneHistoryGeneration == historyGeneration else { return }
            await reloadNewestHistory(id)
        } catch {
            // Leave the cursor in place so another upward scroll can retry.
        }
    }

    private func reloadNewestHistory(_ id: String) async {
        if id == everyoneConversationId {
            await loadEveryoneHistory(revision: state.revision, resetWindow: true)
        } else if selectedConversationId == id {
            await loadTranscript(resetWindow: true)
        } else {
            await loadMarbleHistory(id, revision: state.revision, resetWindow: true)
        }
    }

    @Published private var outgoing: [MarbleOutgoing] = []

    func lastOutgoing(to target: MarbleSendTarget) -> UUID? {
        outgoing.last(where: { $0.target == target })?.id
    }

    func outgoingPresentation(_ entries: [MarbleEntry], to target: MarbleSendTarget,
                              conversationId: String?) -> MarbleOutgoingPresentation {
        let messages = outgoing.filter { $0.target == target }.map { message in
            var message = message
            if case let .accepted(taskId?, _) = message.delivery,
               state.tasks.contains(where: { $0.id == taskId && ["cancelled", "interrupted"].contains($0.status) }) {
                message.responders = []
            }
            return message
        }
        let liveRows = conversationId.flatMap { liveEntries[$0] } ?? []
        let visible = MarbleEveryoneTimeline.applying(liveRows.filter { $0.threadId == target.threadId }, to: entries)
        var presentation = MarbleOutgoingPresentation(durable: visible, outgoing: messages,
                                                      conversationId: conversationId ?? "")
        if let conversationId, target.threadId == nil, conversationId != everyoneConversationId,
           let preview = streamedReplies[conversationId],
           !presentation.entries.contains(where: { $0.kind == "assistant" && $0.taskId == preview.taskId }) {
            presentation.entries.append(preview)
            presentation.responders = []
        }
        for index in presentation.entries.indices where presentation.entries[index].kind == "assistant"
            && presentation.entries[index].conversationId != everyoneConversationId {
            let row = presentation.entries[index]
            if streamedTaskIDs.contains(row.taskId) {
                presentation.entries[index].presentationID = "reply-\(row.taskId)"
            }
        }
        return presentation
    }

    func send(_ text: String, to target: MarbleSendTarget, entries: [MarbleEntry],
              onConversation: ((String) -> Void)? = nil) -> Bool {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        #if DEBUG
        let isFixtureSend = fixtureEveryoneSend && target == .everyone()
        #else
        let isFixtureSend = false
        #endif
        guard client != nil || isFixtureSend, !trimmed.isEmpty,
              canSend(to: target) else { return false }
        let responders: [String]
        switch target {
        case .everyone: responders = MarbleMentions.responders(in: trimmed, roster: prism.marbles)
        case .marble(let id): responders = [id]
        case .conversation(let id, _):
            responders = [prism.marbles.first(where: { $0.conversationId == id })?.id ?? "marble"]
        }
        let previous = target.isEveryone
            ? outgoingPresentation(entries, to: target, conversationId: everyoneConversationId).entries
            : entries
        let message = MarbleOutgoing(target: target, text: trimmed, responders: responders, entries: previous)
        if let onConversation { draftResolutions[message.id] = onConversation }
        outgoing.append(message)
        if !isFixtureSend { transmit(message) }
        return true
    }

    func retrySend(_ id: UUID) {
        guard client != nil,
              let index = outgoing.firstIndex(where: { $0.id == id && $0.delivery == .failed }),
              canSend(to: outgoing[index].target) else { return }
        outgoing[index].delivery = .sending
        transmit(outgoing[index])
    }

    private func canSend(to target: MarbleSendTarget) -> Bool {
        if let id = target.conversationId,
           isDeletedConversation(id) || conversationDeletions[id] == hostGeneration.current {
            return false
        }
        return MarbleOutgoing.canStart(target, alongside: outgoing)
    }

    private func transmit(_ message: MarbleOutgoing) {
        Task {
            var conversationID: String?
            var taskID: String?
            var entryID: String?
            var clientMessageID: String?
            var busyNames: [String] = []
            do {
                try await withChatClient { client in
                    switch message.target {
                    case .everyone(let thread):
                        let reply = try await client.sendEveryone(
                            text: message.text, threadId: thread, clientMessageId: message.id
                        )
                        guard reply.ok else { throw MarbleClientError.conflict("The message wasn’t accepted.") }
                        taskID = reply.taskId
                        entryID = reply.entryId
                        clientMessageID = reply.clientMessageID
                        busyNames = reply.busy
                    case .conversation(let id, let thread):
                        let reply = try await client.send(
                            text: message.text, conversationId: id, threadId: thread, clientMessageId: message.id
                        )
                        guard reply.accepted else { throw MarbleClientError.conflict("The message wasn’t accepted.") }
                        conversationID = reply.conversationId
                        taskID = reply.taskId
                        entryID = reply.entry_id
                        clientMessageID = reply.client_message_id
                    case .marble(let id):
                        guard let marble = prism.marbles.first(where: { $0.id == id }) else {
                            throw MarbleClientError.notFound
                        }
                        if marble.conversationId.isEmpty {
                            let reply = try await client.dispatchSwarm(
                                text: message.text, marbles: [id], clientMessageId: message.id
                            )
                            entryID = reply.entry_id
                            clientMessageID = reply.client_message_id
                            taskID = reply.taskId
                        } else {
                            let reply = try await client.send(
                                text: message.text, conversationId: marble.conversationId, clientMessageId: message.id
                            )
                            guard reply.accepted else { throw MarbleClientError.conflict("The message wasn’t accepted.") }
                            taskID = reply.taskId
                            entryID = reply.entry_id
                            clientMessageID = reply.client_message_id
                        }
                    }
                }
                if let clientMessageID, UUID(uuidString: clientMessageID) != message.id {
                    throw MarbleClientError.decoding("The send response used a different message ID.")
                }
                guard let index = outgoing.firstIndex(where: { $0.id == message.id }) else { return }
                var accepted = outgoing[index]
                let noEmptyTask = taskID.flatMap { $0.isEmpty ? nil : $0 }
                let previousTask: String?
                if case let .accepted(taskId, _) = accepted.delivery { previousTask = taskId }
                else { previousTask = nil }
                accepted.delivery = .accepted(taskId: noEmptyTask ?? previousTask,
                                              entryId: accepted.confirmedEntryID ?? entryID)
                let busyIDs = Set(prism.marbles.filter { marble in
                    busyNames.contains { $0.caseInsensitiveCompare(marble.name) == .orderedSame }
                        || (marble.isDefault && busyNames.contains("Marble"))
                }.map(\.id))
                accepted.responders.removeAll { busyIDs.contains($0) }
                let resolved = moveDraft(&accepted, to: conversationID)
                outgoing[index] = accepted
                if let noEmptyTask, message.target == .everyone() { everyoneTaskIds.insert(noEmptyTask) }
                if let resolved { finishDraftResolution(message.id, conversationID: resolved) }
                await refresh()
            } catch {
                guard let index = outgoing.firstIndex(where: { $0.id == message.id }) else { return }
                outgoing[index].recordFailure(MarbleClientError.from(error))
                if outgoing[index].delivery == .pending { scheduleEventRefresh() }
            }
        }
    }

    private var draftResolutions: [UUID: (String) -> Void] = [:]

    private func moveDraft(_ message: inout MarbleOutgoing, to conversationID: String?) -> String? {
        guard case .conversation(nil, nil) = message.target,
              let conversationID, !conversationID.isEmpty else { return nil }
        message.target = .conversation(conversationID)
        return conversationID
    }

    private func finishDraftResolution(_ id: UUID, conversationID: String) {
        if selectedConversationId == nil { selectedConversationId = conversationID }
        draftResolutions.removeValue(forKey: id)?(conversationID)
    }

    private func confirmOutgoing(_ entries: [MarbleEntry], to target: MarbleSendTarget) {
        guard outgoing.contains(where: { $0.target == target && $0.confirmedEntryID == nil }) else { return }
        var messages = outgoing
        var claimed = Set(messages.compactMap(\.confirmedEntryID))
        for index in messages.indices where messages[index].target == target && messages[index].confirmedEntryID == nil {
            guard let match = messages[index].match(in: entries), claimed.insert(match.id).inserted else { continue }
            messages[index].confirm(match)
        }
        if messages != outgoing { outgoing = messages }
    }

    /// Send a correction into the turn that is running now.
    /// Returns whether the host accepted the correction, so the composer
    /// keeps the draft when it did not.
    @discardableResult
    func steer(_ text: String, conversationId: String?, taskId: String?) async -> Bool {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let client, !trimmed.isEmpty else { return false }
        guard let taskId else {
            errorMessage = "Nothing is running to correct"
            return false
        }
        do {
            try await client.steer(
                text: trimmed,
                conversationId: conversationId,
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

    func toolActivity(for entry: MarbleEntry) -> MarbleToolActivity? {
        guard var tool = MarbleToolActivity.fromTranscript(entry) else { return nil }
        if let live = tools(for: entry.taskId).first(where: { $0.entryIDs[entry.conversationId] == entry.id }) {
            // Keep the durable label; a persisted start row can lag the SSE end.
            if live.status != .running { tool.status = live.status }
            tool.sender = live.sender
        }
        if tool.status == .running && (settledToolTasks.contains(entry.taskId)
            || state.tasks.contains(where: { $0.id == entry.taskId && !$0.isRunning && $0.status != "queued" })) {
            tool.status = .finished
        }
        return tool
    }

    func withLiveToolRows(_ entries: [MarbleEntry], conversationId: String?, threadId: String? = nil) -> [MarbleEntry] {
        guard let conversationId, !toolsByTurn.isEmpty else { return entries }
        var activeTasks = Set(state.tasks.lazy.filter(\.isRunning).map(\.id))
        if let task = live[conversationId]?.taskId, !settledToolTasks.contains(task) { activeTasks.insert(task) }
        return MarbleToolActivity.insertingLiveRows(
            toolsByTurn, into: entries, activeTasks: activeTasks,
            conversationId: conversationId, threadId: threadId
        )
    }

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
            || tools(for: taskId).contains { $0.entryIDs[conversationId] != nil }
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
        #if DEBUG
        if let fixtureThreads {
            return fixtureThreads.values.map(\.thread).filter { $0.conversationId == conversationId }
        }
        #endif
        return try await withChatClient { try await $0.threads(conversationId: conversationId) }
    }

    func threadPage(_ id: String) -> MarbleThreadPage? {
        #if DEBUG
        if let page = fixtureThreads?[id] { return page }
        #endif
        return threadPages[id]
    }

    func loadThreadHistory(_ id: String, resetWindow: Bool = false) async throws {
        #if DEBUG
        if fixtureThreads?[id] != nil { return }
        #endif
        var page = try await withChatClient { try await $0.thread(id: id) }
        let conversation = page.thread.conversationId
        guard !deletedConversationIDs.contains(conversation) else { throw MarbleClientError.notFound }
        let target: MarbleSendTarget = conversation == everyoneConversationId
            ? .everyone(thread: id) : .conversation(conversation, thread: id)
        confirmOutgoing(page.entries, to: target)
        confirmEntries(page.entries, conversationId: conversation)
        let previous = resetWindow ? nil : threadPages[id]
        (page.entries, page.nextBefore) = MarbleHistoryWindow.refreshing(
            previous?.entries ?? [], before: previous?.nextBefore, with: page.entries,
            nextBefore: page.hasMore == true ? page.nextBefore : nil
        )
        if threadPages[id] != page {
            threadPages[id] = page
            saveCache()
        }
    }

    func loadOlderThread(_ id: String, beforePrepend: () -> Void = {}) async {
        guard let before = threadPages[id]?.nextBefore, loadingOlderThreads[id] == nil else { return }
        let generation = hostGeneration.current
        loadingOlderThreads[id] = before
        defer {
            if hostGeneration.isCurrent(generation) { loadingOlderThreads[id] = nil }
        }
        do {
            var older = try await withChatClient { try await $0.thread(id: id, before: before) }
            guard let current = threadPages[id], current.nextBefore == before else { return }
            if older.hasMore == nil {
                try await loadThreadHistory(id, resetWindow: true)
                return
            }
            older.entries = MarbleHistoryWindow.prepending(older.entries, to: current.entries)
            if older.hasMore != true { older.nextBefore = nil }
            if older.entries.count > current.entries.count { beforePrepend() }
            threadPages[id] = older
        } catch MarbleClientError.unknownHistoryCursor {
            guard hostGeneration.isCurrent(generation), threadPages[id]?.nextBefore == before else { return }
            try? await loadThreadHistory(id, resetWindow: true)
        } catch {
            // Keep the current rows and allow another scroll to retry.
        }
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
    ) async -> String? {
        guard let client else {
            errorMessage = "Reconnect to your host to create a Marble."
            return nil
        }
        let existing = Set(prism.marbles.map(\.id))
        do {
            let draft = MarbleClient.MarbleDraft(
                name: name,
                role: role,
                glyph: glyph,
                hue: hue,
                workspace: workspace,
                persona: persona
            )
            let updated = try await client.addMarble(draft)
            errorMessage = nil
            prismNotice = "\(draft.name) is here."
            await refresh()
            return updated.marbles.first { !existing.contains($0.id) && $0.name == draft.name }?.id
        } catch {
            prismNotice = nil
            errorMessage = error.localizedDescription
        }
        return nil
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
            setHistory([], for: everyoneConversationId, before: nil)
            threadPages = threadPages.filter { $0.value.thread.conversationId != everyoneConversationId }
            await refresh()
        } catch {
            prismNotice = "Couldn't clear Everyone: \(error.localizedDescription)"
        }
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

    func createConversation() async throws -> MarbleConversation {
        let conversation: MarbleConversation
        #if DEBUG
        if fixtureThreads != nil {
            conversation = MarbleConversation(
                id: UUID().uuidString, title: "", taskIds: [],
                updatedAt: UInt64(Date().timeIntervalSince1970 * 1000)
            )
            state.conversations.append(conversation)
            return conversation
        }
        #endif
        conversation = try await withChatClient { try await $0.createConversation() }
        if state.conversation(conversation.id) == nil {
            state.conversations.append(conversation)
        }
        scheduleEventRefresh()
        return conversation
    }

    func rename(_ conversation: MarbleConversation, title: String) async {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let client, !trimmed.isEmpty else { return }
        await perform { try await client.renameConversation(id: conversation.id, title: trimmed) }
    }

    func isDeletedConversation(_ id: String) -> Bool {
        deletedConversationIDs.contains(id)
    }

    func delete(conversationId id: String) async -> Bool {
        let generation = hostGeneration.current
        guard !isDeletedConversation(id), conversationDeletions[id] != generation else { return false }
        guard !outgoing.contains(where: { $0.target.conversationId == id && $0.delivery == .sending }) else {
            errorMessage = "Wait for the message to send before deleting this chat."
            return false
        }
        conversationDeletions[id] = generation
        defer {
            if conversationDeletions[id] == generation { conversationDeletions.removeValue(forKey: id) }
        }
        do {
            try await withChatClient { try await $0.deleteConversation(id: id) }
            purgeConversation(id)
            errorMessage = nil
            scheduleEventRefresh()
            return true
        } catch is CancellationError {
            return false
        } catch {
            guard hostGeneration.isCurrent(generation) else { return false }
            errorMessage = error.localizedDescription
            await refresh()
            return false
        }
    }

    private func removeDeletedConversations(from snapshot: inout MarbleState) {
        guard !deletedConversationIDs.isEmpty else { return }
        snapshot.conversations.removeAll { deletedConversationIDs.contains($0.id) }
        snapshot.transcript.removeAll { deletedConversationIDs.contains($0.conversationId) }
        snapshot.tasks.removeAll { deletedConversationIDs.contains($0.conversationId) }
    }

    private func purgeConversation(_ id: String) {
        guard deletedConversationIDs.insert(id).inserted else { return }
        invalidateEveryoneHistory()
        let transcript = marbleTranscripts[id] ?? state.transcript(for: id)
        let taskIDs = Set(state.tasks.filter { $0.conversationId == id }.map(\.id)
            + transcript.map(\.taskId).filter { !$0.isEmpty })
        var snapshot = state
        removeDeletedConversations(from: &snapshot)
        state = snapshot
        if selectedConversationId == id { selectedConversationId = nil }
        if transcriptHistoryLoadedFor == id {
            transcriptHistoryLoadedFor = nil
            transcriptRevision = -1
        }
        var cache = MarbleChatCache(everyone: everyoneTimeline, transcripts: marbleTranscripts)
        cache.removeConversation(id)
        marbleTranscripts = cache.transcripts
        if everyoneTimeline != cache.everyone { everyoneTimeline = cache.everyone }
        everyoneTimeline.removeAll { taskIDs.contains($0.taskId) }
        liveEntries[everyoneConversationId]?.removeAll { taskIDs.contains($0.taskId) }
        marbleHistoryRevisions.removeValue(forKey: id)
        historyBefore.removeValue(forKey: id)
        threadPages = threadPages.filter { $0.value.thread.conversationId != id }
        historyErrors.remove(id)
        awaitingTranscript.removeValue(forKey: id)
        live.removeValue(forKey: id)
        liveEntries.removeValue(forKey: id)
        streamedReplies.removeValue(forKey: id)
        for taskID in taskIDs {
            reasoningByTurn.removeValue(forKey: taskID)
            toolsByTurn.removeValue(forKey: taskID)
            if privateReplySizing[taskID] != nil { privateReplySizing.removeValue(forKey: taskID) }
            cancelNarration(for: taskID)
        }
        reasoningTurnIds.removeAll { taskIDs.contains($0) }
        recentToolTurnIDs.removeAll { taskIDs.contains($0) }
        streamedTaskIDs.subtract(taskIDs)
        settledToolTasks.subtract(taskIDs)
        threadTaskIds.subtract(taskIDs)
        retryingTurns.subtract(taskIDs)
        retryFailures.subtract(taskIDs)
        outgoing.removeAll { $0.target.conversationId == id }
        let removedThreads = threadUpdates.values.filter { $0.conversationId == id }.map(\.id)
        for threadID in removedThreads {
            threadUpdates.removeValue(forKey: threadID)
            threadEventSequences.removeValue(forKey: threadID)
        }
        saveCache(discardOnFailure: true)
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

    private var narration = MarbleReplyNarration()
    private var narratedTaskId: String?

    func toggleVoice() async {
        guard let client else { return }
        let wasEnabled = voice.enabled
        await perform { _ = try await client.setVoice(enabled: !wasEnabled) }
        if wasEnabled {
            narration = MarbleReplyNarration()
            speaker.stop()
            narratedTaskId = nil
        }
    }

    func speakLastReply(_ text: String) {
        Task { await speak(text) }
    }

    private func speak(_ text: String, utterance: MarbleReplyNarration.Utterance? = nil) async {
        guard let client, voice.enabled, !text.isEmpty else { return }
        if let utterance, !narration.isQueued(utterance) { return }
        var spoken = text
        if let shaped = try? await client.speak(text: text), !shaped.isEmpty {
            spoken = shaped
        }
        if let utterance, !narration.take(utterance) { return }
        guard voice.enabled else { return }
        narratedTaskId = utterance?.taskId
        speaker.speak(spoken, rate: Float(voice.rate))
    }

    private func cancelNarration(for taskId: String) {
        narration.cancel(taskId: taskId)
        if narratedTaskId == taskId {
            speaker.stop()
            narratedTaskId = nil
        }
    }

    private func narrateFinalReply(_ entry: MarbleEntry) {
        guard voice.enabled, entry.conversationId == selectedConversationId,
              let utterance = narration.finalReply(entry) else { return }
        Task { await speak(utterance.text, utterance: utterance) }
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
        guard let reply, localNotificationsEnabled else { return }
        let conversationId = event.conversationId
        let threadId = event.data.threadId
        let mirroredInEveryone = MarbleNotificationCursor.isMirroredInEveryone(
            event, entries: (liveEntries[everyoneConversationId] ?? []) + everyoneEntries
        )
        guard appInBackground ? backgroundConnection.isKeepingAlive
            : shouldPresentReply?(conversationId, threadId, mirroredInEveryone) == true else { return }
        notificationCursor.notified(reply.entry)
        saveNotificationCursor()
        let content = UNMutableNotificationContent()
        content.title = reply.title
        content.body = String(reply.text.prefix(180))
        content.sound = .default
        var marble = ["conversationId": conversationId]
        marble["threadId"] = threadId
        if mirroredInEveryone { marble["mirroredInEveryone"] = "true" }
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

    private func updateHostHealth(_ health: MarbleHealth) {
        if hostBuild != health.build { hostBuild = health.build }
        if engineHealth != health.runtime { engineHealth = health.runtime }
    }

    private func listen() {
        stopStreaming()
        status.reconnectStarted()
        let client = self.client
        let generation = hostGeneration.current
        streamTask = Task { [weak self] in
            var generation = generation
            var delivery = MarbleReplyDeliveryWindow()
            while !Task.isCancelled {
                guard let self, let client else { return }
                var endedReason: String? = nil
                do {
                    let health = try await client.health(timeout: 3)
                    guard !Task.isCancelled, self.hostGeneration.isCurrent(generation) else { return }
                    self.updateHostHealth(health)
                    if self.prepareForStream(health) {
                        delivery.hostRestarted(keepingAlive: self.backgroundConnection.isKeepingAlive)
                    }
                    generation = self.hostGeneration.current
                    stream: for try await update in client.events(since: self.sequence) {
                        guard !Task.isCancelled, self.hostGeneration.isCurrent(generation) else { return }
                        switch update {
                        case .opened:
                            self.status.streamOpened()
                            self.threadHostRevision &+= 1
                            let openedGeneration = generation
                            // A newly restarted host may not send any live event.
                            Task {
                                guard self.hostGeneration.isCurrent(openedGeneration) else { return }
                                await self.refresh()
                            }
                        case .activity:
                            self.status.streamEvent()
                        case .baseline(let cursor):
                            // A restart can land between /healthz and /api/events.
                            if cursor < self.sequence { break stream }
                            delivery.opened(at: cursor, keepingAlive: self.backgroundConnection.isKeepingAlive)
                            self.backoffSeconds = 1
                            let connectedAt = Date.now
                            self.streamConnectedAt = connectedAt
                            Self.streamLog.debug("SSE connected")
                            self.updateBackgroundConnection()
                            self.backgroundConnection.refreshActivity(connectedAt: connectedAt)
                        case .event(let event):
                            self.status.streamEvent()
                            self.handle(event, liveAfter: delivery.liveAfter)
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
                let keepingAlive = self.backgroundConnection.isKeepingAlive
                delivery.interrupted(keepingAlive: keepingAlive)
                self.streamDisconnected(keepingAlive: keepingAlive)
                self.status.streamEnded(endedReason)
                self.loaded = true
                if self.appInBackground && !keepingAlive {
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

    private func prepareForStream(_ health: MarbleHealth) -> Bool {
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
            loadingOlder = [:]
            loadingOlderThreads = [:]
            chatsRevision = -1
            everyoneHistoryRevision = -1
            marbleHistoryRevisions = [:]
            saveCursor()
        }
        if let pid = health.pid, hostPID != pid {
            hostPID = pid
            if let endpoint {
                UserDefaults.standard.set(pid, forKey: Self.hostPIDKeyPrefix + endpoint.baseURL.absoluteString)
            }
        }
        return resume.restarted
    }

    /// Mirror the selected conversation's buffers into the published fields.
    private func syncLive() {
        let buffers = live[selectedConversationId ?? ""] ?? LiveBuffers()
        if liveText != buffers.text { liveText = buffers.text }
        if liveReasoning != buffers.reasoning { liveReasoning = buffers.reasoning }
        if activeTaskId != buffers.taskId { activeTaskId = buffers.taskId }
    }

    @Published private var streamedReplies: [String: MarbleEntry] = [:]
    private var streamedTaskIDs: Set<String> = []

    private func presentStreamedReply(_ text: String, in conversationId: String, taskId: String) {
        streamedTaskIDs.insert(taskId)
        var preview = MarbleEntry(
            conversationId: conversationId, kind: "assistant", text: text, taskId: taskId, at: 0
        )
        preview.presentationID = "reply-\(taskId)"
        streamedReplies[conversationId] = preview
    }

    @Published private var liveEntries: [String: [MarbleEntry]] = [:]
    @Published private(set) var automaticRetries: Set<String> = []
    @Published private var groupReplyPreviews: [String: GroupReplyPreview] = [:]
    @Published private var privateReplySizing: [String: [String]] = [:]
    private var pendingEventRefresh: Task<Void, Never>?

    func pendingReplyText(for taskId: String) -> String? {
        groupReplyPreviews[taskId]?.text
    }

    func retractedReplyBlocks(for entry: MarbleEntry) -> [String] {
        if entry.kind == "pending" { return groupReplyPreviews[entry.taskId]?.retracted ?? [] }
        if entry.serverID == nil, entry.presentationID != nil { return privateReplySizing[entry.taskId] ?? [] }
        return []
    }

    private func scheduleEventRefresh() {
        guard pendingEventRefresh == nil else { return }
        let generation = hostGeneration.current
        pendingEventRefresh = Task {
            try? await Task.sleep(for: .milliseconds(40))
            guard hostGeneration.isCurrent(generation), !Task.isCancelled else { return }
            pendingEventRefresh = nil
            await refresh()
        }
    }

    private func presentReply(_ event: MarbleEvent) {
        guard let id = event.data.entryId else { return }
        let durable = event.data.threadId.flatMap { threadPages[$0]?.entries } ?? historyEntries(event.conversationId)
        let reserved = liveEntries[event.conversationId]?.first { $0.id == id }
            ?? durable.first { $0.id == id }
        guard var entry = MarbleEveryoneTimeline.settling(
            event, reserved: reserved, at: UInt64(Date().timeIntervalSince1970 * 1000)
        ) else { return }
        if entry.speaker.isEmpty {
            entry.speaker = prism.marbles.first { $0.name == entry.speakerName }?.id ?? ""
        }
        presentEntry(entry)
        if entry.kind == "failure" {
            cancelNarration(for: entry.taskId)
        } else {
            narrateFinalReply(entry)
        }
        if automaticRetries.contains(entry.taskId) { automaticRetries.remove(entry.taskId) }
        if event.conversationId == everyoneConversationId, groupReplyPreviews[entry.taskId] != nil {
            groupReplyPreviews.removeValue(forKey: entry.taskId)
        }
        if entry.threadId == nil {
            if live[event.conversationId]?.taskId == event.taskId {
                live.removeValue(forKey: event.conversationId)
                if selectedConversationId == event.conversationId { syncLive() }
            }
            if streamedReplies[event.conversationId]?.taskId == event.taskId {
                streamedReplies.removeValue(forKey: event.conversationId)
            }
            if privateReplySizing[entry.taskId] != nil { privateReplySizing.removeValue(forKey: entry.taskId) }
        }
    }

    private func presentEntry(_ entry: MarbleEntry) {
        let previous = liveEntries[entry.conversationId] ?? []
        let rows = MarbleEveryoneTimeline.applying([entry], to: previous)
        if rows != previous { liveEntries[entry.conversationId] = rows }
    }

    private func presentOutgoingEntry(_ entry: MarbleEntry) {
        guard let clientID = entry.clientMessageID.flatMap(UUID.init(uuidString:)),
              let index = outgoing.firstIndex(where: {
                  $0.id == clientID && $0.target.threadId == entry.threadId
              }) else { return }
        var message = outgoing[index]
        switch message.target {
        case .everyone:
            guard entry.conversationId == everyoneConversationId else { return }
        case .conversation(let id, _):
            guard entry.conversationId != everyoneConversationId, id == nil || id == entry.conversationId else { return }
        case .marble(let id):
            let known = prism.marbles.first { $0.id == id }?.conversationId ?? ""
            guard entry.conversationId != everyoneConversationId,
                  known.isEmpty || known == entry.conversationId else { return }
        }
        if message.confirmedEntryID != entry.id { presentEntry(entry) }
        message.confirm(entry)
        let resolved = moveDraft(&message, to: entry.conversationId)
        if outgoing[index] != message { outgoing[index] = message }
        if let resolved { finishDraftResolution(clientID, conversationID: resolved) }
    }

    private func confirmEntries(_ entries: [MarbleEntry], conversationId: String) {
        if conversationId == selectedConversationId, voice.enabled {
            for entry in entries { narrateFinalReply(entry) }
        }
        if let rows = liveEntries[conversationId] {
            let durable = Dictionary(entries.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
            let remaining = rows.filter { update in
                guard let saved = durable[update.id] else { return true }
                return saved.kind == "pending" && (update.kind != "pending" || saved.taskId != update.taskId)
            }
            if remaining != rows { liveEntries[conversationId] = remaining.isEmpty ? nil : remaining }
        }
        if let buffers = live[conversationId], let taskId = buffers.taskId,
           entries.contains(where: { $0.taskId == taskId && ["assistant", "failure"].contains($0.kind) }) {
            live.removeValue(forKey: conversationId)
            if streamedReplies[conversationId] != nil { streamedReplies.removeValue(forKey: conversationId) }
            if privateReplySizing[taskId] != nil { privateReplySizing.removeValue(forKey: taskId) }
            if selectedConversationId == conversationId { syncLive() }
        }
    }

    private var settledToolTasks: Set<String> = []

    private func finishTools(for taskId: String) {
        guard !taskId.isEmpty, settledToolTasks.insert(taskId).inserted else { return }
        var calls = tools(for: taskId)
        MarbleToolActivity.finishRunning(&calls)
        if !calls.isEmpty && toolsByTurn[taskId] != calls { toolsByTurn[taskId] = calls }
    }

    private func recordTool(_ event: MarbleEvent) {
        guard !event.taskId.isEmpty else { return }
        var calls = tools(for: event.taskId)
        MarbleToolActivity.apply(event, to: &calls)
        if settledToolTasks.contains(event.taskId) { MarbleToolActivity.finishRunning(&calls) }
        guard !calls.isEmpty else { return }
        if toolsByTurn[event.taskId] == nil {
            if recentToolTurnIDs.count >= 64 {
                let oldest = recentToolTurnIDs.removeFirst()
                toolsByTurn.removeValue(forKey: oldest)
            }
            recentToolTurnIDs.append(event.taskId)
        }
        if toolsByTurn[event.taskId] != calls { toolsByTurn[event.taskId] = calls }
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
        guard !deletedConversationIDs.contains(event.conversationId) else { return }
        if event.kind == "conversation", let id = event.data.deleted, !id.isEmpty {
            purgeConversation(id)
            scheduleEventRefresh()
            return
        }
        if ["settled", "reply", "error"].contains(event.kind) { finishTools(for: event.taskId) }
        if ["tool_start", "tool_end"].contains(event.kind) { recordTool(event) }
        if event.kind == "text_reset" || event.kind == "error" {
            cancelNarration(for: event.taskId)
        }
        let taskId = event.data.taskId ?? event.taskId
        if event.kind == "task_retry" {
            if !taskId.isEmpty { automaticRetries.insert(taskId) }
            return
        }
        if event.kind == "reasoning" { recordReasoning(event.data.text ?? "", taskId: event.taskId) }
        if let text = MarbleStreamingText.updating(groupReplyPreviews[taskId]?.text ?? "", for: taskId, from: event),
           groupReplyPreviews[taskId] != nil || hasPendingGroupReply(taskId) {
            var preview = groupReplyPreviews[taskId] ?? GroupReplyPreview()
            if event.kind == "text_reset" { preview.retracted.append(preview.text) }
            preview.text = text
            groupReplyPreviews[taskId] = preview
        }
        if event.kind == "entry" {
            if let entry = event.entryUpdate, entry.kind == "pending" || entry.kind == "system",
               liveAfter.map({ event.sequence > $0 }) == true {
                presentEntry(entry)
                if entry.kind == "system" {
                    automaticRetries.remove(entry.taskId)
                    if entry.conversationId == everyoneConversationId, groupReplyPreviews[entry.taskId] != nil {
                        groupReplyPreviews.removeValue(forKey: entry.taskId)
                    }
                }
            }
            if let entry = event.userEntry { presentOutgoingEntry(entry) }
            if let threadId = event.entryUpdate?.threadId { threadEventSequences[threadId] = event.sequence }
            scheduleEventRefresh()
            return
        }
        if event.kind == "reply" {
            if let liveAfter, event.sequence > liveAfter { presentReply(event) }
            handleReply(event, liveAfter: liveAfter)
            saveCursor()
            if let threadId = event.data.threadId {
                threadEventSequences[threadId] = event.sequence
            }
            scheduleEventRefresh()
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
                scheduleEventRefresh()
            }
            if event.kind == "settled" { saveCursor() }
            return
        }
        var buffers = live[event.conversationId] ?? LiveBuffers()
        let forActive = event.conversationId.isEmpty
            || event.conversationId == (selectedConversationId ?? "")
        switch event.kind {
        case "text", "text_reset":
            if event.kind == "text", buffers.taskId == nil, !event.taskId.isEmpty { buffers.taskId = event.taskId }
            guard let text = MarbleStreamingText.updating(buffers.text, for: buffers.taskId ?? "", from: event) else {
                break
            }
            let existing = streamedReplies[event.conversationId]
            if var preview = existing, preview.taskId == event.taskId {
                if event.kind == "text_reset" {
                    privateReplySizing[event.taskId, default: []].append(preview.text)
                }
                preview.text = text
                streamedReplies[event.conversationId] = preview
            } else if event.kind == "text", !event.taskId.isEmpty,
                      forActive || event.conversationId == displayedMarbleConversation {
                presentStreamedReply(text, in: event.conversationId, taskId: event.taskId)
            }
            buffers.text = text
        case "reasoning":
            let text = event.data.text ?? ""
            if !event.taskId.isEmpty && buffers.taskId == nil {
                buffers.taskId = event.taskId
            }
            if buffers.taskId == event.taskId || event.taskId.isEmpty {
                buffers.reasoning += text
            }
        case "approval_pending":
            scheduleEventRefresh()
        case "approval_resolved", "state":
            scheduleEventRefresh()
        case "everyone":
            everyoneTimeline = MarbleEveryoneTimeline.applying(event, to: everyoneTimeline)
            if event.data.cleared != nil {
                historyBefore[everyoneConversationId] = nil
                threadPages = threadPages.filter { $0.value.thread.conversationId != everyoneConversationId }
                saveCache()
            }
            chatsRevision = -1
            scheduleEventRefresh()
        case "thread":
            if let thread = event.data.thread {
                threadUpdates[thread.id] = thread
                threadEventSequences[thread.id] = event.sequence
            }
        case "settled":
            guard buffers.taskId == nil || event.taskId.isEmpty || buffers.taskId == event.taskId else { break }
            if !event.taskId.isEmpty, !event.conversationId.isEmpty, event.data.cancelled != true {
                awaitingTranscript[event.conversationId] = event.taskId
                if forActive, voice.enabled, liveAfter.map({ event.sequence > $0 }) == true {
                    // The durable reply follows settlement. Only its final
                    // text (or the confirmed transcript) can be narrated.
                    narration.awaitFinal(taskId: event.taskId, conversationId: event.conversationId)
                }
            } else if event.data.cancelled == true {
                cancelNarration(for: event.taskId)
            }
            if buffers.text.isEmpty || event.data.cancelled == true { buffers = LiveBuffers() }
            if event.data.cancelled == true && streamedReplies[event.conversationId] != nil {
                streamedReplies.removeValue(forKey: event.conversationId)
            }
            if event.data.cancelled == true, privateReplySizing[event.taskId] != nil {
                privateReplySizing.removeValue(forKey: event.taskId)
            }
            saveCursor()
            scheduleEventRefresh()
        case "error":
            if event.taskId.isEmpty {
                errorMessage = "Marble couldn’t finish the request."
            } else if buffers.taskId == event.taskId {
                buffers = LiveBuffers()
            }
            scheduleEventRefresh()
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

    private func hasPendingGroupReply(_ taskId: String) -> Bool {
        guard !taskId.isEmpty else { return false }
        let live = liveEntries[everyoneConversationId] ?? []
        if let update = live.first(where: { $0.taskId == taskId && $0.kind != "user" }) {
            return update.kind == "pending"
        }
        return everyoneTimeline.contains { $0.taskId == taskId && $0.kind == "pending" }
            || threadPages.values.contains { page in
                page.thread.conversationId == everyoneConversationId
                    && page.entries.contains { $0.taskId == taskId && $0.kind == "pending" }
            }
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
        store.client = nil
        store.endpoint = nil
        store.needsToken = false
        store.resetChats()
        store.started = true
        store.connectionPaused = false
        store.onboarded = true
        store.loaded = true
        store.fixtureEveryoneSend = ProcessInfo.processInfo.arguments.contains("-MarbleUITestEveryoneSend")
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
        state.transcript += (0..<60).map { index in
            var entry = MarbleEntry(conversationId: everyoneConversationId, kind: "assistant",
                                    text: "Everyone history row \(index)", taskId: "everyone-fixture-\(index)",
                                    at: now + UInt64(index), speaker: "m-ada", speakerName: "Ada")
            entry.serverID = "everyone-fixture-entry-\(index)"
            return entry
        }
        if ProcessInfo.processInfo.arguments.contains("-MarbleUITestDelayedEveryone") {
            store.fixturePendingEveryone = state.transcript(for: everyoneConversationId)
            state.transcript.removeAll { $0.conversationId == everyoneConversationId }
        }
        store.fixtureThreads = [:]
        for count in [2, 60] {
            let id = "fixture-thread-\(count)"
            let parentId = "\(id)-parent"
            let fields: [String: Any] = [
                "id": id, "conversation_id": everyoneConversationId, "parent_entry_id": parentId,
                "parent_preview": "Thread with \(count) fixture replies", "reply_count": count,
                "last_activity_at": now + UInt64(count), "created_at": now
            ]
            if let data = try? JSONSerialization.data(withJSONObject: fields),
               let thread = try? JSONDecoder().decode(MarbleThread.self, from: data) {
                let rows = (0...count).map { index in
                    var entry = MarbleEntry(conversationId: everyoneConversationId, kind: "assistant",
                                            text: "Thread \(count) history row \(index)", taskId: "\(id)-\(index)",
                                            at: now + UInt64(index))
                    entry.serverID = index == 0 ? parentId : "\(id)-entry-\(index)"
                    return entry
                }
                store.fixtureThreads?[id] = MarbleThreadPage(thread: thread, entries: rows)
            }
        }
        store.state = state
        // The long chat keeps emitting text so streaming-follow behavior is testable.
        Task { @MainActor [weak store] in
            var tick = 0
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(400))
                guard let store else { return }
                tick += 1
                store.live["fixture-0", default: LiveBuffers()].text =
                    "Streaming fixture reply \(tick) " + String(repeating: "…", count: tick % 40)
                store.syncLive()
            }
        }
        return store
    }
}
#endif
