import Foundation
import Network
import SwiftUI
#if canImport(UIKit)
import UIKit
#endif
import UserNotifications

/// Compile-time feature switches for this alpha.
enum MarbleFeatures {
    /// APNs needs an `aps-environment` entitlement the current signing does not
    /// have; remote registration stays off and local notifications cover the
    /// same alerts. The delegate hooks stay in place for when it is enabled.
    static let remotePush = false
}

/// Observable product store shared by every iOS surface.
@MainActor
final class MarbleStore: ObservableObject {
    enum Connection: String {
        case idle
        case connecting
        case live
        case offline

        var label: String {
            switch self {
            case .idle: return "idle"
            case .connecting: return "connecting"
            case .live: return "connected"
            case .offline: return "offline"
            }
        }
    }

    struct LiveTool: Identifiable, Equatable {
        let id = UUID()
        var name: String
        var summary: String
        var isError: Bool
    }

    /// One conversation's in-flight reply parts; several chats can stream at
    /// once, so each keeps its own instead of sharing one buffer.
    private struct LiveBuffers: Equatable {
        var text = ""
        var reasoning = ""
        var tools: [LiveTool] = []
        var taskId: String?

        var isEmpty: Bool {
            text.isEmpty && reasoning.isEmpty && tools.isEmpty && taskId == nil
        }
    }

    @Published private(set) var state: MarbleState = .empty
    @Published private(set) var connection: Connection = .idle
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
                if let taskId = state.runningTask(for: id)?.id, buffers.taskId == nil {
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
    @Published private(set) var liveText = ""
    @Published private(set) var liveReasoning = ""
    @Published private(set) var liveTools: [LiveTool] = []
    @Published private(set) var activeTaskId: String?
    @Published var errorMessage: String?
    /// Last accepted Prism change, shown inside the Prism sheet.
    @Published var prismNotice: String?
    @Published var needsToken = false
    /// Reported by `/healthz` during refresh, for the About section.
    @Published private(set) var hostVersion: String?
    @Published private(set) var engineHealth: MarbleHealth.RuntimeHealth?

    private var client: MarbleClient!
    private let speaker = MarbleSpeaker()
    private var streamTask: Task<Void, Never>?
    private var sequence: UInt64 = 0
    private var started = false
    /// Per-conversation live buffers; `liveText` & friends mirror the selected one.
    private var live: [String: LiveBuffers] = [:]
    private var backoffSeconds: Double = 1
    private var pathMonitor: NWPathMonitor?
    private var appActive = true
    /// Dedupe keys for local alerts (see MarbleAttention).
    private var notifiedKeys: Set<String> = []

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
        self.client = endpoint.map { MarbleClient(endpoint: $0) }
        self.localNotificationsEnabled = UserDefaults.standard.bool(
            forKey: Self.localNotificationsKey
        )
        NotificationCenter.default.addObserver(
            forName: .marblePushToken,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let token = notification.object as? String else { return }
            Task { await self?.registerPushToken(token) }
        }
        NotificationCenter.default.addObserver(
            forName: .marbleOpenConversation,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let id = notification.object as? String else { return }
            Task { @MainActor [weak self] in self?.open(conversationId: id) }
        }
    }

    // MARK: - Derived product state

    var selectedConversation: MarbleConversation? {
        state.conversation(selectedConversationId)
    }

    var transcript: [MarbleEntry] {
        if transcriptConversationId == selectedConversationId,
           !conversationTranscript.isEmpty {
            return conversationTranscript
        }
        return state.transcript(for: selectedConversationId)
    }

    var isWorking: Bool { activeTaskId != nil }

    /// Marble's own control layer, as published by the host.
    var control: MarbleControl { state.controlState }

    /// What the assistant is doing, when the host reports it.
    var presence: MarblePresence { state.presenceInfo }
    var memory: MarbleMemory { state.memoryState }
    var routines: MarbleRoutines { state.routineState }
    var voice: MarbleVoice { state.voiceSettings }
    var notificationState: MarbleNotificationSettings { state.notificationState }
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

    var statusLabel: String {
        if isWorking { return "working" }
        if presence.status == "waiting" { return "waiting for you" }
        if presence.status == "paused" || control.paused { return "paused" }
        if presence.status == "working" { return "working" }
        return connection.label
    }

    // MARK: - Lifecycle

    func start() {
        guard onboarded, !started else { return }
        started = true
        startPathMonitor()
        Task {
            await refresh()
            listen()
            await registerForPush()
        }
    }

    /// Select the conversation a notification pointed at.
    func open(conversationId: String) {
        guard !conversationId.isEmpty else { return }
        if selectedConversationId != conversationId {
            selectedConversationId = conversationId
        }
    }

    /// A new host+token that passed onboarding validation.
    func apply(_ endpoint: MarbleEndpoint) {
        self.endpoint = endpoint
        MarblePreferences.save(endpoint)
        client = MarbleClient(endpoint: endpoint)
        onboarded = true
        streamTask?.cancel()
        sequence = 0
        needsToken = false
        conversationTranscript = []
        transcriptConversationId = nil
        transcriptCursor = 0
        transcriptRevision = -1
        live = [:]
        syncLive()
        started = true
        startPathMonitor()
        Task {
            await loginIfNeeded()
            await refresh()
            listen()
        }
    }

    /// Forget host and token and return to onboarding.
    func signOut() {
        MarblePreferences.signOut()
        streamTask?.cancel()
        streamTask = nil
        client = nil
        endpoint = nil
        onboarded = false
        started = false
        sequence = 0
        state = .empty
        conversationTranscript = []
        transcriptConversationId = nil
        transcriptCursor = 0
        transcriptRevision = -1
        live = [:]
        syncLive()
        connection = .idle
        loaded = false
    }

    /// Reconnect right now: scene became active or the network path changed.
    func reconnect() {
        guard onboarded else { return }
        backoffSeconds = 1
        Task {
            await refresh()
            await loadTranscript()
            listen()
        }
    }

    func appBecameActive() {
        appActive = true
        reconnect()
    }

    func appBecameInactive() {
        appActive = false
    }

    private func startPathMonitor() {
        guard pathMonitor == nil else { return }
        let monitor = NWPathMonitor()
        monitor.pathUpdateHandler = { [weak self] _ in
            Task { @MainActor [weak self] in
                // Any path change is worth a reconnection attempt: the stream
                // may be dead even when the path still reports satisfied.
                self?.reconnect()
            }
        }
        monitor.start(queue: DispatchQueue(label: "marble.pathmonitor"))
        pathMonitor = monitor
    }

    private func loginIfNeeded() async {
        guard let token = endpoint?.token, !token.isEmpty else { return }
        do {
            try await client.login(token: token)
        } catch {
            // The bearer header still authenticates every later request.
        }
    }

    // MARK: - Product actions

    func refresh() async {
        guard let client else { return }
        do {
            async let healthFetch = client.health()
            let snapshot = try await client.state()
            state = snapshot
            needsToken = false
            errorMessage = nil
            if connection != .live { connection = .live }
            if let health = try? await healthFetch {
                hostVersion = health.version
                engineHealth = health.runtime
            }
            if selectedConversationId == nil || snapshot.conversation(selectedConversationId) == nil {
                let newest = snapshot.conversations
                    .sorted { $0.updatedAt > $1.updatedAt }
                    .first?.id
                if selectedConversationId != newest {
                    selectedConversationId = newest
                }
            }
            // Several chats run at once, one engine lane each. This surface is
            // looking at one of them, so only that chat's turn makes it busy.
            // The buffer is the single source of truth for activeTaskId;
            // writing the buffer keeps later syncLive() calls from reverting it.
            let selectedTask = snapshot.runningTask(for: selectedConversationId)?.id
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
        } catch MarbleClientError.unauthorized {
            needsToken = true
            connection = .offline
            loaded = true
        } catch {
            connection = .offline
            loaded = true
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
        do {
            let page = try await client.transcript(
                conversationId: conversationId,
                after: transcriptCursor == 0 ? nil : transcriptCursor
            )
            var known = Set(conversationTranscript.map(Self.transcriptKey))
            for entry in page.entries where known.insert(Self.transcriptKey(entry)).inserted {
                conversationTranscript.append(entry)
            }
            transcriptCursor = max(transcriptCursor, page.cursor)
            transcriptRevision = state.revision
        } catch {
            // Keep the previous page; the next refresh retries.
        }
    }

    private static func transcriptKey(_ entry: MarbleEntry) -> String {
        "\(entry.at)|\(entry.kind)|\(entry.taskId)|\(entry.text)"
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
    func steer(_ text: String) async {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let client, !trimmed.isEmpty, let taskId = activeTaskId else { return }
        do {
            try await client.steer(
                text: trimmed,
                conversationId: selectedConversationId,
                taskId: taskId
            )
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    /// Repeat a failed, cancelled, or interrupted turn.
    func retry(_ task: MarbleTask) async {
        guard let client else { return }
        do {
            try await client.retry(taskId: task.id)
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    /// Transcript search across every conversation.
    func search(query: String) async throws -> [MarbleSearchHit] {
        guard let client else { return [] }
        return try await client.search(query: query)
    }

    func addMarble(name: String, role: String, glyph: String, hue: Int, workspace: String) async {
        guard let client else { return }
        do {
            let draft = MarbleClient.MarbleDraft(
                name: name,
                role: role,
                glyph: glyph,
                hue: hue,
                workspace: workspace
            )
            _ = try await client.addMarble(draft)
            errorMessage = nil
            prismNotice = "\(draft.name) joined the roster."
        } catch {
            prismNotice = nil
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    func removeMarble(_ marble: MarblePrismMarble) async {
        guard let client else { return }
        do {
            try await client.removeMarble(id: marble.id)
            errorMessage = nil
            prismNotice = "\(marble.name) left the roster. Its conversation stays."
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
            prismNotice = (reply.swarm ? "Swarm sent to " : "Sent to ")
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

    // MARK: - Notifications

    func saveNotifications(_ update: MarbleClient.NotificationUpdate) async {
        guard let client else { return }
        await perform { _ = try await client.saveNotifications(update) }
    }

    func testNotification() async -> Bool {
        guard let client else { return false }
        do {
            return try await client.testNotification()
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func removeDevice(_ device: MarbleDevice) async {
        guard let client else { return }
        do {
            try await client.removeDevice(tokenPrefix: device.tokenPrefix)
        } catch {
            errorMessage = error.localizedDescription
        }
        await refresh()
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

    func remember(_ text: String) async {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let client, !trimmed.isEmpty else { return }
        await perform { _ = try await client.remember(text: trimmed) }
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

    func addRoutine(title: String, prompt: String, schedule: String) async {
        let trimmedPrompt = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let client, !trimmedPrompt.isEmpty else { return }
        let resolvedSchedule = schedule.trimmingCharacters(in: .whitespacesAndNewlines)
        await perform {
            _ = try await client.addRoutine(
                title: title.trimmingCharacters(in: .whitespacesAndNewlines),
                prompt: trimmedPrompt,
                schedule: resolvedSchedule.isEmpty ? "every:1h" : resolvedSchedule,
                conversationId: self.selectedConversationId
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

    func registerPushToken(_ token: String) async {
        guard MarbleFeatures.remotePush, let client else { return }
        do {
            try await client.registerDevice(
                token: token,
                topic: Bundle.main.bundleIdentifier ?? ""
            )
        } catch {
            // Push is optional: a host without APNs credentials simply ignores it.
        }
    }

    private func registerForPush() async {
        guard MarbleFeatures.remotePush else { return }
        #if canImport(UIKit)
        let center = UNUserNotificationCenter.current()
        _ = try? await center.requestAuthorization(options: [.alert, .sound, .badge])
        UIApplication.shared.registerForRemoteNotifications()
        #endif
    }

    // MARK: - Local notifications

    func requestLocalNotificationPermission() async -> Bool {
        let settings = await UNUserNotificationCenter.current().notificationSettings()
        if settings.authorizationStatus == .authorized { return true }
        return (try? await UNUserNotificationCenter.current()
            .requestAuthorization(options: [.alert, .sound, .badge])) ?? false
    }

    /// Alert for an event that happened off-screen so a person can pull it back.
    private func postLocalNotification(conversationId: String, title: String, body: String) {
        guard localNotificationsEnabled else { return }
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        content.sound = .default
        content.userInfo = ["marble": ["conversationId": conversationId]]
        let request = UNNotificationRequest(
            identifier: "marble.\(UUID().uuidString)",
            content: content,
            trigger: nil
        )
        UNUserNotificationCenter.current().add(request)
    }

    /// Alerts only when the surface is not already looking at the result, and
    /// only once per turn — the host publishes two `settled` frames.
    private func notifyIfNeeded(event: MarbleEvent) {
        guard let alert = MarbleAttention.alert(for: event, seen: &notifiedKeys),
              !event.conversationId.isEmpty else { return }
        let visible = appActive && event.conversationId == (selectedConversationId ?? "")
        guard !visible else { return }
        let conversationTitle = state.conversation(event.conversationId)?.title ?? ""
        postLocalNotification(
            conversationId: event.conversationId,
            title: alert.title,
            body: conversationTitle.isEmpty ? alert.body : conversationTitle
        )
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
        streamTask?.cancel()
        connection = .connecting
        let client = self.client
        streamTask = Task { [weak self] in
            while !Task.isCancelled {
                guard let self, let client else { return }
                let since = self.sequence
                do {
                    for try await event in client.events(since: since) {
                        self.backoffSeconds = 1
                        self.handle(event)
                    }
                } catch MarbleClientError.unauthorized {
                    self.markUnauthorized()
                    return
                } catch {
                    if Task.isCancelled { return }
                    self.markOffline(error)
                }
                if Task.isCancelled { return }
                let delay = self.backoffSeconds
                self.backoffSeconds = min(self.backoffSeconds * 2, 30)
                try? await Task.sleep(for: .seconds(delay))
            }
        }
    }

    /// Mirror the selected conversation's buffers into the published fields.
    private func syncLive() {
        let buffers = live[selectedConversationId ?? ""] ?? LiveBuffers()
        liveText = buffers.text
        liveReasoning = buffers.reasoning
        liveTools = buffers.tools
        activeTaskId = buffers.taskId
    }

    private func handle(_ event: MarbleEvent) {
        sequence = max(sequence, event.sequence)
        if connection != .live { connection = .live }
        var buffers = live[event.conversationId] ?? LiveBuffers()
        let forActive = event.conversationId.isEmpty
            || event.conversationId == (selectedConversationId ?? "")
        switch event.kind {
        case "text":
            buffers.text += event.data.text ?? ""
        case "reasoning":
            buffers.reasoning += event.data.text ?? ""
        case "tool_start":
            buffers.tools.append(
                LiveTool(name: event.data.tool ?? "tool", summary: "running…", isError: false)
            )
        case "tool_end":
            let name = event.data.tool ?? "tool"
            let summary = (event.data.isError == true ? "failed · " : "done · ")
                + String((event.data.text ?? "").prefix(160))
            if let index = buffers.tools.firstIndex(where: {
                $0.name == name && $0.summary == "running…"
            }) {
                buffers.tools[index].summary = summary
                buffers.tools[index].isError = event.data.isError == true
            } else {
                buffers.tools.append(
                    LiveTool(name: name, summary: summary, isError: event.data.isError == true)
                )
            }
        case "approval_pending":
            Task { await refresh() }
        case "approval_resolved", "state":
            Task { await refresh() }
        case "settled":
            let reply = forActive ? buffers.text : ""
            buffers = LiveBuffers()
            Task {
                await refresh()
                if !reply.isEmpty { await speak(reply) }
            }
        case "error":
            if let message = event.data.message { errorMessage = message }
            Task { await refresh() }
        case "cancel_requested":
            errorMessage = nil
        case "turn_start":
            buffers.taskId = event.taskId.isEmpty ? buffers.taskId : event.taskId
        default:
            break
        }
        live[event.conversationId] = buffers.isEmpty ? nil : buffers
        if forActive { syncLive() }
        notifyIfNeeded(event: event)
    }

    private func markUnauthorized() {
        needsToken = true
        connection = .offline
    }

    private func markOffline(_ error: Error) {
        connection = .offline
        if let clientError = error as? MarbleClientError, clientError == .unauthorized {
            needsToken = true
            return
        }
        errorMessage = error.localizedDescription
    }
}
