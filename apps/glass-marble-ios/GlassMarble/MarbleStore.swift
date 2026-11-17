import Foundation
import SwiftUI

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
    @Published var selectedConversationId: String?
    @Published private(set) var liveText = ""
    @Published private(set) var liveReasoning = ""
    @Published private(set) var liveTools: [LiveTool] = []
    @Published private(set) var activeTaskId: String?
    @Published var errorMessage: String?
    @Published var needsToken = false

    private var client: MarbleClient
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
    }

    // MARK: - Derived product state

    var selectedConversation: MarbleConversation? {
        state.conversation(selectedConversationId)
    }

    var transcript: [MarbleEntry] {
        state.transcript(for: selectedConversationId)
    }

    var isWorking: Bool { activeTaskId != nil }

    var statusLabel: String {
        if isWorking { return "working" }
        return connection.label
    }

    // MARK: - Lifecycle

    func start() {
        guard !started else { return }
        started = true
        Task {
            await refresh()
            listen()
        }
    }

    func apply(_ endpoint: MarbleEndpoint) {
        self.endpoint = endpoint
        MarblePreferences.save(endpoint)
        client = MarbleClient(endpoint: endpoint)
        sequence = 0
        needsToken = false
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
                selectedConversationId = snapshot.conversations
                    .sorted { $0.updatedAt > $1.updatedAt }
                    .first?.id
            }
            activeTaskId = snapshot.runningTask?.id
            if activeTaskId == nil {
                liveText = ""
                liveReasoning = ""
                liveTools = []
            }
        } catch MarbleClientError.unauthorized {
            needsToken = true
            connection = .offline
        } catch {
            connection = .offline
            errorMessage = error.localizedDescription
        }
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
            activeTaskId = accepted.taskId
            liveText = ""
            liveReasoning = ""
            liveTools = []
            errorMessage = nil
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func cancelActiveTask() async {
        guard let taskId = activeTaskId else { return }
        do {
            try await client.cancel(taskId: taskId)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func decide(_ approval: MarbleApproval, allow: Bool) async {
        do {
            try await client.decide(requestId: approval.requestId, allow: allow)
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
        switch event.kind {
        case "text":
            liveText += event.data.text ?? ""
        case "reasoning":
            liveReasoning += event.data.text ?? ""
        case "tool_start":
            liveTools.append(
                LiveTool(name: event.data.tool ?? "tool", summary: "running…", isError: false)
            )
        case "tool_end":
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
            activeTaskId = nil
            liveText = ""
            liveReasoning = ""
            liveTools = []
            Task { await refresh() }
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
