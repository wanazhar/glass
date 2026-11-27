import Foundation

enum MarbleReplyReveal {
    static let interval: TimeInterval = 0.13
    static let maximumFrames = 10
    static let duration = interval * Double(maximumFrames)

    static func frames(for text: String) -> [String] {
        let words = text.indices.filter { index in
            !text[index].isWhitespace && (index == text.startIndex || text[text.index(before: index)].isWhitespace)
        }
        guard words.count > 1 else { return [text] }
        let stride = (words.count + maximumFrames - 1) / maximumFrames
        return Swift.stride(from: stride, to: words.count, by: stride).map { word in
            String(text[..<words[word]])
        } + [text]
    }
}

enum MarbleStreamingText {
    static func updating(_ current: String, for taskId: String, from event: MarbleEvent) -> String? {
        guard event.taskId == taskId else { return nil }
        switch event.kind {
        case "text":
            guard let delta = event.data.text, !delta.isEmpty else { return nil }
            return current + delta
        case "text_reset":
            return taskId.isEmpty || current.isEmpty ? nil : ""
        default:
            return nil
        }
    }
}

struct MarbleReplyNarration {
    struct Utterance {
        let taskId: String
        let text: String
        fileprivate let id: UUID
    }

    private enum State {
        case awaitingFinal(conversationId: String)
        case queued(UUID)
    }

    private var tasks: [String: State] = [:]

    mutating func awaitFinal(taskId: String, conversationId: String) {
        guard !taskId.isEmpty, tasks[taskId] == nil else { return }
        tasks[taskId] = .awaitingFinal(conversationId: conversationId)
    }

    mutating func finalReply(_ entry: MarbleEntry) -> Utterance? {
        guard entry.kind == "assistant", entry.threadId == nil, !entry.text.isEmpty,
              case let .awaitingFinal(conversationId) = tasks[entry.taskId],
              entry.conversationId == conversationId else { return nil }
        let utterance = Utterance(taskId: entry.taskId, text: entry.text, id: UUID())
        tasks[entry.taskId] = .queued(utterance.id)
        return utterance
    }

    func isQueued(_ utterance: Utterance) -> Bool {
        guard case let .queued(id) = tasks[utterance.taskId] else { return false }
        return id == utterance.id
    }

    mutating func take(_ utterance: Utterance) -> Bool {
        guard isQueued(utterance) else { return false }
        cancel(taskId: utterance.taskId)
        return true
    }

    mutating func cancel(taskId: String) {
        tasks.removeValue(forKey: taskId)
    }
}

struct MarbleToolActivity: Identifiable, Equatable {
    enum Status: String {
        case running = "Running"
        case completed = "Done"
        case finished = "Finished"
        case failed = "Failed"
        case denied = "Denied"
    }

    let id: String
    let name: String
    var query: String?
    var status: Status
    var callId: String? = nil
    var requestId: String? = nil
    var sender: String? = nil
    var entryIDs: [String: String] = [:]
    var threadId: String? = nil
    var sequence: UInt64 = 0
    var details: MarbleToolDetails? = nil
    var started: Date? = nil

    var isWebSearch: Bool { ["web_search", "web.search", "glass.web.search", "glass.browser.search"].contains(name) }

    var title: String {
        if isWebSearch { return "Searched the web" }
        if ["bash", "shell", "exec"].contains(name) { return "Ran a command" }
        if ["read", "glass.workspace.read"].contains(name), let query {
            return "Read \((query as NSString).lastPathComponent)"
        }
        if ["web_fetch", "web.fetch", "glass.web.fetch"].contains(name),
           let query, let host = URL(string: query)?.host {
            return "Opened \(host)"
        }
        return MarbleDecisionWords.action(tool: name)
    }

    /// The host stores transcript tool rows as "name · summary · status".
    /// Split on the outside separators so a query may itself contain one.
    static func fromTranscript(_ entry: MarbleEntry) -> Self? {
        guard entry.kind == "tool", let statusSeparator = entry.text.range(of: " · ", options: .backwards) else {
            return nil
        }
        let status: Status
        switch entry.text[statusSeparator.upperBound...] {
        case "ok": status = .completed
        case "running": status = .running
        case "failed": status = .failed
        case "denied": status = .denied
        case "finished": status = .finished
        default: return nil
        }
        let label = entry.text[..<statusSeparator.lowerBound]
        guard let summarySeparator = label.range(of: " · ") else {
            return Self(id: entry.id, name: String(label), query: nil, status: status, details: entry.toolDetails)
        }
        return Self(
            id: entry.id, name: String(label[..<summarySeparator.lowerBound]),
            query: String(label[summarySeparator.upperBound...]), status: status, details: entry.toolDetails
        )
    }

    static func apply(_ event: MarbleEvent, to calls: inout [Self]) {
        let name = event.data.tool ?? event.data.name
        let callId = event.data.callId
        let requestId = event.data.requestId
        let id = callId ?? requestId
        let index = calls.lastIndex {
            if let callId, let previous = $0.callId { return callId == previous }
            if let requestId, let previous = $0.requestId { return requestId == previous }
            return id == nil && event.kind == "tool_end" && $0.name == name && $0.status == .running
        }
        let row: Int
        if let index {
            row = index
        } else {
            guard let name else { return }
            row = calls.count
            calls.append(Self(id: id ?? String(event.sequence), name: name, query: nil,
                              status: .running, sequence: event.sequence))
        }
        calls[row].callId = callId ?? calls[row].callId
        calls[row].requestId = requestId ?? calls[row].requestId
        if let sender = event.data.sender, !sender.isEmpty { calls[row].sender = sender }
        if let entryId = event.data.toolEntryId { calls[row].entryIDs[event.conversationId] = entryId }
        if let threadId = event.data.threadId { calls[row].threadId = threadId }
        switch event.kind {
        case "tool_start":
            if calls[row].started == nil { calls[row].started = Date() }
            if case let .object(arguments) = event.data.arguments,
               let value = ["query", "q", "url", "path", "command"].compactMap({ key -> String? in
                   if case let .string(value) = arguments[key] { return value }
                   return nil
               }).first {
                calls[row].query = String(value.prefix(160))
                calls[row].details = MarbleToolDetails(command: value, output: "", elapsedMs: nil)
            }
            if let title = event.data.title {
                calls[row].details = MarbleToolDetails(command: title, output: "", elapsedMs: nil)
            }
        case "tool_end":
            calls[row].status = event.data.approved == false ? .denied : event.data.isError == true ? .failed : .completed
            let elapsed = calls[row].started.map { UInt64(max(0, Date().timeIntervalSince($0) * 1000)) }
            calls[row].details = MarbleToolDetails(command: calls[row].details?.command ?? "",
                                                  output: event.data.text ?? "", elapsedMs: elapsed)
        default:
            break
        }
    }

    static func finishRunning(_ calls: inout [Self]) {
        for index in calls.indices where calls[index].status == .running {
            calls[index].status = .finished
        }
    }

    var transcriptText: String {
        let end: String
        switch status {
        case .running: end = "running"
        case .completed: end = "ok"
        case .finished: end = "finished"
        case .failed: end = "failed"
        case .denied: end = "denied"
        }
        return [name, query, end].compactMap { $0 }.joined(separator: " · ")
    }

    static func insertingLiveRows(
        _ callsByTask: [String: [Self]], into entries: [MarbleEntry],
        activeTasks: Set<String>, conversationId: String, threadId: String?
    ) -> [MarbleEntry] {
        guard !callsByTask.isEmpty else { return entries }
        let durableIDs = Set(entries.map(\.id))
        let visibleTasks = activeTasks.union(entries.map(\.taskId))
        let live = callsByTask.flatMap { taskId, calls -> [(UInt64, MarbleEntry)] in
            guard visibleTasks.contains(taskId) else { return [] }
            return calls.compactMap { call in
                guard call.threadId == threadId, let id = call.entryIDs[conversationId], !durableIDs.contains(id)
                else { return nil }
                var entry = MarbleEntry(conversationId: conversationId, kind: "tool", text: call.transcriptText,
                                        taskId: taskId, at: 0, speakerName: call.sender ?? "")
                entry.serverID = id
                entry.threadId = threadId
                entry.toolDetails = call.details
                return (call.sequence, entry)
            }
        }.sorted { $0.0 < $1.0 }
        var rows = entries
        for (_, entry) in live {
            // Everyone's reserved reply slot precedes its tools.
            let index = rows.firstIndex {
                $0.taskId == entry.taskId && ["assistant", "failure"].contains($0.kind)
                    && $0.conversationId != everyoneConversationId
            }
            rows.insert(entry, at: index ?? rows.endIndex)
        }
        return rows
    }
}

struct MarbleFailureInfo: Codable, Equatable {
    let category: String
    let retryable: Bool
    let diagnostics: String
}

enum MarbleTurnFailure {
    static func message(for status: String) -> String? {
        switch status {
        case "failed": "Marble couldn’t finish this reply."
        case "cancelled": "Marble stopped before finishing."
        case "interrupted": "Marble was interrupted before finishing."
        default: nil
        }
    }

    static func isLegacyProviderError(_ entry: MarbleEntry) -> Bool {
        ["assistant", "system", "error"].contains(entry.kind)
            && entry.text.trimmingCharacters(in: .whitespacesAndNewlines).hasPrefix("[Generation failed]")
    }

    static func representsFailure(_ entry: MarbleEntry, status: String?) -> Bool {
        if ["failure", "error"].contains(entry.kind) || isLegacyProviderError(entry) { return true }
        return entry.kind == "system" && status.flatMap(message(for:)) != nil
    }

    static func isStoppedPartialReply(_ entry: MarbleEntry, taskStatus: String?) -> Bool {
        entry.kind == "assistant" && (entry.cancelled == true || taskStatus == "cancelled")
    }

    static func retryID(entry: MarbleEntry?, task: MarbleTask?) -> String? {
        if let entry, let failure = entry.failure {
            return failure.retryable && !entry.taskId.isEmpty ? entry.taskId : nil
        }
        if let entry, entry.cancelled == true, !entry.taskId.isEmpty { return entry.taskId }
        guard let task, !task.id.isEmpty, message(for: task.status) != nil,
              !task.prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
        return task.id
    }
}

/// Attach separate reasoning records to the terminal row for the same task.
/// Each view supplies only its own chat or thread's entries.
struct MarbleReasoningAssociation {
    let attached: [String: String]
    let tools: [String: [MarbleEntry]]
    let standalone: [MarbleEntry]
    let timestamps: [String: String]

    init(_ entries: [MarbleEntry], keepVisible entryId: String? = nil) {
        let outcomes = Dictionary(entries.compactMap { entry -> (String, String)? in
            guard !entry.taskId.isEmpty, ["pending", "assistant", "failure", "error"].contains(entry.kind) else { return nil }
            return (entry.taskId, entry.id)
        }, uniquingKeysWith: { first, _ in first })
        var attached: [String: String] = [:]
        var tools: [String: [MarbleEntry]] = [:]
        standalone = entries.filter { entry in
            if entry.kind == "tool", entry.id != entryId, let outcome = outcomes[entry.taskId] {
                tools[outcome, default: []].append(entry)
                return false
            }
            guard entry.kind == "reasoning", entry.id != entryId,
                  let outcome = outcomes[entry.taskId] else { return true }
            if !entry.text.isEmpty {
                attached[outcome] = [attached[outcome], entry.text].compactMap { $0 }.joined(separator: "\n\n")
            }
            return false
        }
        self.attached = attached
        self.tools = tools
        var timestamps: [String: String] = [:]
        var previous: Date?
        for entry in standalone where entry.at > 0 && ["user", "assistant", "failure"].contains(entry.kind) {
            let date = Date(timeIntervalSince1970: Double(entry.at) / 1_000)
            if previous.map({ !Calendar.current.isDate($0, inSameDayAs: date) }) ?? true {
                timestamps[entry.id] = date.formatted(date: .abbreviated, time: .omitted)
            } else if let previous, date.timeIntervalSince(previous) > 300 {
                timestamps[entry.id] = date.formatted(date: .omitted, time: .shortened)
            }
            previous = date
        }
        self.timestamps = timestamps
    }
}
