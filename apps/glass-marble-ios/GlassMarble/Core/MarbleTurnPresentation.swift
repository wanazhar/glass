import Foundation

struct MarbleToolActivity: Identifiable, Equatable {
    enum Status: String {
        case running = "Running"
        case completed = "Done"
        case failed = "Failed"
        case denied = "Denied"
    }

    let id: String
    let name: String
    var query: String?
    var status: Status

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
        guard let name = event.data.tool ?? event.data.name else { return }
        let id = event.data.requestId ?? event.data.callId
        let index = calls.lastIndex {
            if let id { return $0.id == id }
            return $0.name == name && $0.status == .running
        }
        switch event.kind {
        case "tool_start":
            guard index == nil else { return }
            let query: String?
            if case let .object(arguments) = event.data.arguments,
               case let .string(value) = arguments["query"] ?? arguments["q"] {
                query = String(value.prefix(160))
            } else {
                query = nil
            }
            calls.append(Self(id: id ?? String(event.sequence), name: name, query: query, status: .running))
        case "tool_end":
            let status: Status = event.data.approved == false ? .denied : event.data.isError == true ? .failed : .completed
            if let index {
                calls[index].status = status
            } else {
                calls.append(Self(id: id ?? String(event.sequence), name: name, query: nil, status: status))
            }
        default:
            break
        }
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
