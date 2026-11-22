import Foundation

/// Plain words for one recorded control decision: what Marble wanted to do
/// and how it was settled, instead of raw tool names and request ids. The
/// raw values stay available behind a tap.
enum MarbleDecisionWords {
    /// What the call would have done, e.g. "Write to workspace".
    static func action(tool: String) -> String {
        switch tool {
        case "glass.workspace.write": return "Write to workspace"
        case "glass.workspace.read": return "Read from workspace"
        case "glass.workspace.list": return "List workspace files"
        case "glass.workspace.search": return "Search workspace"
        case "glass.crates.list": return "List workspace crates"
        case "glass.marble.ask": return "Ask another Marble"
        default:
            var name = tool
            if name.hasPrefix("glass.") { name = String(name.dropFirst("glass.".count)) }
            let words = name
                .split(whereSeparator: { $0 == "." || $0 == "_" || $0 == "-" })
                .joined(separator: " ")
            guard let first = words.first else { return "Use a tool" }
            return first.uppercased() + words.dropFirst()
        }
    }

    /// How it was settled, e.g. "Denied by you".
    static func outcome(decision: String, reason: String) -> String {
        let lowered = reason.lowercased()
        switch lowered {
        case "approved on this surface": return "Allowed by you"
        case "declined on this surface": return "Denied by you"
        case "cancelled before this tool call ran": return "Cancelled before it ran"
        case "cancelled while this tool call was running": return "Stopped while running"
        case "host policy approves governed tools": return "Allowed by host policy"
        case "host policy declines governed tools": return "Denied by host policy"
        case "asking another marble needs no approval": return "Allowed automatically"
        default: break
        }
        if lowered.hasPrefix("grant ") { return "Allowed by a saved permission" }
        if lowered.contains("denied path rule") { return "Blocked: off-limits folder" }
        if lowered.contains("read-only") || lowered.contains("reads only") { return "Blocked: read-only mode" }
        if lowered.contains("profile answers governed calls") { return "Allowed automatically" }
        switch decision {
        case "approved": return "Allowed by you"
        case "allowed": return "Allowed automatically"
        case "denied": return "Denied"
        default:
            guard let first = decision.first else { return "Decided" }
            return first.uppercased() + decision.dropFirst()
        }
    }

    /// Whether the call went ahead, for the row's symbol and colour.
    static func wentAhead(decision: String) -> Bool {
        decision == "approved" || decision == "allowed"
    }
}
