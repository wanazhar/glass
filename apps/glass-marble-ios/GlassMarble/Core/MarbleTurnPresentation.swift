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

    static func apply(_ event: MarbleEvent, to calls: inout [Self]) {
        guard let name = event.data.tool else { return }
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
        if isLegacyProviderError(entry) || entry.kind == "error" { return true }
        return entry.kind == "system" && status.flatMap(message(for:)) != nil
    }
}
