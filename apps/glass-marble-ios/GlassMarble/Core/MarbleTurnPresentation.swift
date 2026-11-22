import Foundation

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

    var isWebSearch: Bool { ["web_search", "web.search", "glass.web.search", "glass.browser.search"].contains(name) }

    var title: String {
        if isWebSearch {
            return status == .completed ? "Searched the web" : "Search the web"
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
            return Self(id: entry.id, name: String(label), query: nil, status: status)
        }
        return Self(
            id: entry.id, name: String(label[..<summarySeparator.lowerBound]),
            query: String(label[summarySeparator.upperBound...]), status: status
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
            if case let .object(arguments) = event.data.arguments,
               let value = ["query", "q", "url", "path", "command"].compactMap({ key -> String? in
                   if case let .string(value) = arguments[key] { return value }
                   return nil
               }).first {
                calls[row].query = String(value.prefix(160))
            }
        case "tool_end":
            calls[row].status = event.data.approved == false ? .denied : event.data.isError == true ? .failed : .completed
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

    static func retryID(entry: MarbleEntry?, task: MarbleTask?) -> String? {
        if let entry, let failure = entry.failure {
            return failure.retryable && !entry.taskId.isEmpty ? entry.taskId : nil
        }
        guard let task, !task.id.isEmpty, message(for: task.status) != nil,
              !task.prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return nil }
        return task.id
    }
}

/// Attach separate reasoning records to the terminal row for the same task.
/// Each view supplies only its own chat or thread's entries.
struct MarbleReasoningAssociation {
    let attached: [String: String]
    let standalone: [MarbleEntry]

    init(_ entries: [MarbleEntry], keepVisible entryId: String? = nil) {
        let outcomes = Dictionary(entries.compactMap { entry -> (String, String)? in
            guard !entry.taskId.isEmpty, ["assistant", "failure", "error"].contains(entry.kind) else { return nil }
            return (entry.taskId, entry.id)
        }, uniquingKeysWith: { first, _ in first })
        var attached: [String: String] = [:]
        standalone = entries.filter { entry in
            guard entry.kind == "reasoning", entry.id != entryId,
                  let outcome = outcomes[entry.taskId] else { return true }
            if !entry.text.isEmpty {
                attached[outcome] = [attached[outcome], entry.text].compactMap { $0 }.joined(separator: "\n\n")
            }
            return false
        }
        self.attached = attached
    }
}
