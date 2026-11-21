import Foundation
import SwiftUI
#if canImport(UIKit)
import UIKit
#endif
import UserNotifications

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

    @Published private(set) var state: MarbleState = .empty
    @Published private(set) var connection: Connection = .idle
    @Published private(set) var endpoint: MarbleEndpoint
    @Published var selectedConversationId: String? {
        didSet {
            guard oldValue != selectedConversationId else { return }
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

    private var client: MarbleClient
    private let speaker = MarbleSpeaker()
    private var streamTask: Task<Void, Never>?
    private var sequence: UInt64 = 0
    private var started = false

    init() {
        let endpoint = MarbleEndpoint(
            baseURL: MarblePreferences.baseURL,
            token: MarblePreferences.token
        )
        self.endpoint = endpoint
        self.client = MarbleClient(endpoint: endpoint)
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
            self?.open(conversationId: id)
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
        guard !started else { return }
        started = true
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

    func apply(_ endpoint: MarbleEndpoint) {
        self.endpoint = endpoint
        MarblePreferences.save(endpoint)
        client = MarbleClient(endpoint: endpoint)
        sequence = 0
        needsToken = false
        conversationTranscript = []
        transcriptConversationId = nil
        transcriptCursor = 0
        transcriptRevision = -1
        Task {
            await loginIfNeeded()
            await refresh()
            listen()
        }
    }

    private func loginIfNeeded() async {
        guard let token = endpoint.token, !token.isEmpty else { return }
        do {
            try await client.login(token: token)
        } catch {
            // The bearer header still authenticates every later request.
        }
    }

    // MARK: - Product actions

    func refresh() async {
        do {
            let snapshot = try await client.state()
            state = snapshot
            needsToken = false
            errorMessage = nil
            if connection != .live { connection = .live }
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
            activeTaskId = snapshot.runningTask(for: selectedConversationId)?.id
            loaded = true
            if activeTaskId == nil {
                liveText = ""
                liveReasoning = ""
                liveTools = []
            }
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
        guard let conversationId = selectedConversationId else {
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
        guard !trimmed.isEmpty else { return }
        do {
            let accepted = try await client.send(
                text: trimmed,
                conversationId: selectedConversationId
            )
            selectedConversationId = accepted.conversationId
            // A marble answers in its own engine and reports its task through the
            // next state poll, so an empty id must not look like a live turn.
            activeTaskId = accepted.taskId.isEmpty ? nil : accepted.taskId
            if let marble = accepted.marbleName {
                prismNotice = "\(marble) is answering in its own conversation."
            }
            liveText = ""
            liveReasoning = ""
            liveTools = []
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    /// Send a correction into the turn that is running now.
    func steer(_ text: String) async {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, let taskId = activeTaskId else { return }
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

    func addMarble(name: String, role: String, glyph: String, hue: Int, workspace: String) async {
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
        guard !trimmed.isEmpty else { return }
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
        do {
            toolInventory = try await client.tools()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    /// Read the workspace crate map. A host outside a workspace has none, which
    /// is a plain answer rather than a failure of the sheet.
    func loadCrates() async {
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
        guard let taskId = activeTaskId else { return }
        do {
            try await client.cancel(taskId: taskId)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func decide(_ approval: MarbleApproval, allow: Bool, remember: Bool = false) async {
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
        do {
            try await client.setProfile(profile)
        } catch {
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    func setPaused(_ paused: Bool) async {
        do {
            try await client.setPaused(paused)
        } catch {
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    func stopEverything() async {
        do {
            try await client.stopEverything()
        } catch {
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    func revokeGrant(_ grantId: String) async {
        do {
            try await client.revokeGrant(grantId)
        } catch {
            errorMessage = error.localizedDescription
        }
        await refresh()
    }

    func login(token: String) async {
        let trimmed = token.trimmingCharacters(in: .whitespacesAndNewlines)
        do {
            try await client.login(token: trimmed)
            apply(MarbleEndpoint(baseURL: endpoint.baseURL, token: trimmed))
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    // MARK: - Remembering

    func remember(_ text: String) async {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        await perform { _ = try await self.client.remember(text: trimmed) }
    }

    func rename(_ conversation: MarbleConversation, title: String) async {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        await perform { try await self.client.renameConversation(id: conversation.id, title: trimmed) }
    }

    func delete(_ conversation: MarbleConversation) async {
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
        guard !trimmed.isEmpty else { return }
        await perform {
            _ = try await self.client.updateMemory(id: fact.id, text: trimmed, kind: fact.kind)
        }
    }

    func forget(_ fact: MarbleMemoryFact) async {
        await perform { _ = try await self.client.forgetMemory(id: fact.id) }
    }

    func accept(_ suggestion: MarbleMemorySuggestion) async {
        await perform { _ = try await self.client.acceptSuggestion(id: suggestion.id) }
    }

    func dismiss(_ suggestion: MarbleMemorySuggestion) async {
        await perform { _ = try await self.client.dismissSuggestion(id: suggestion.id) }
    }

    // MARK: - Routines

    func addRoutine(title: String, prompt: String, schedule: String) async {
        let trimmedPrompt = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedPrompt.isEmpty else { return }
        let resolvedSchedule = schedule.trimmingCharacters(in: .whitespacesAndNewlines)
        await perform {
            _ = try await self.client.addRoutine(
                title: title.trimmingCharacters(in: .whitespacesAndNewlines),
                prompt: trimmedPrompt,
                schedule: resolvedSchedule.isEmpty ? "every:1h" : resolvedSchedule,
                conversationId: self.selectedConversationId
            )
        }
    }

    /// Align `daily:` routines with this phone's timezone.
    func useLocalRoutineTime() async {
        let minutes = TimeZone.current.secondsFromGMT() / 60
        await perform { try await self.client.setRoutineOffset(minutes: minutes) }
    }

    func run(_ routine: MarbleRoutine) async {
        await perform { try await self.client.runRoutine(id: routine.id) }
    }

    func setRoutine(_ routine: MarbleRoutine, enabled: Bool) async {
        await perform { try await self.client.setRoutine(id: routine.id, enabled: enabled) }
    }

    func remove(_ routine: MarbleRoutine) async {
        await perform { try await self.client.removeRoutine(id: routine.id) }
    }

    // MARK: - Voice and push

    func toggleVoice() async {
        let wasEnabled = voice.enabled
        await perform { _ = try await self.client.setVoice(enabled: !wasEnabled) }
        if wasEnabled { speaker.stop() }
    }

    func speakLastReply(_ text: String) {
        Task { await speak(text) }
    }

    private func speak(_ text: String) async {
        guard voice.enabled, !text.isEmpty else { return }
        var spoken = text
        if let shaped = try? await client.speak(text: text), !shaped.isEmpty {
            spoken = shaped
        }
        speaker.speak(spoken, rate: Float(voice.rate))
    }

    func registerPushToken(_ token: String) async {
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
        #if canImport(UIKit)
        let center = UNUserNotificationCenter.current()
        _ = try? await center.requestAuthorization(options: [.alert, .sound, .badge])
        UIApplication.shared.registerForRemoteNotifications()
        #endif
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
                guard let self else { return }
                let since = self.sequence
                do {
                    for try await event in client.events(since: since) {
                        self.handle(event)
                    }
                } catch MarbleClientError.unauthorized {
                    self.markUnauthorized()
                    return
                } catch {
                    self.markOffline(error)
                }
                if Task.isCancelled { return }
                try? await Task.sleep(for: .seconds(2))
            }
        }
    }

    private func handle(_ event: MarbleEvent) {
        sequence = max(sequence, event.sequence)
        if connection != .live { connection = .live }
        // Live text belongs to one conversation. Another chat's turn keeps
        // running on the host; its words reach this surface through the
        // transcript when that chat is opened.
        let forActive = event.conversationId.isEmpty
            || event.conversationId == (selectedConversationId ?? "")
        switch event.kind {
        case "text":
            guard forActive else { break }
            liveText += event.data.text ?? ""
        case "reasoning":
            guard forActive else { break }
            liveReasoning += event.data.text ?? ""
        case "tool_start":
            guard forActive else { break }
            liveTools.append(
                LiveTool(name: event.data.tool ?? "tool", summary: "running…", isError: false)
            )
        case "tool_end":
            guard forActive else { break }
            let name = event.data.tool ?? "tool"
            let summary = (event.data.isError == true ? "failed · " : "done · ")
                + String((event.data.text ?? "").prefix(160))
            if let index = liveTools.firstIndex(where: { $0.name == name && $0.summary == "running…" }) {
                liveTools[index].summary = summary
                liveTools[index].isError = event.data.isError == true
            } else {
                liveTools.append(
                    LiveTool(name: name, summary: summary, isError: event.data.isError == true)
                )
            }
        case "approval_pending", "approval_resolved", "state":
            Task { await refresh() }
        case "settled":
            let reply = forActive ? liveText : ""
            if forActive {
                activeTaskId = nil
                liveText = ""
                liveReasoning = ""
                liveTools = []
            }
            Task {
                await refresh()
                if !reply.isEmpty { await speak(reply) }
            }
        case "error":
            if let message = event.data.message { errorMessage = message }
            Task { await refresh() }
        case "cancel_requested":
            errorMessage = nil
        default:
            break
        }
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
