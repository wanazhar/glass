import Foundation

/// `GET /api/history`: ordered transcript records ending at `before`.
public struct HistoryPage: Sendable, Hashable, Decodable {
    public struct Item: Sendable, Hashable, Decodable {
        public var index: Int
        public var record: JSONValue
        public var previewTruncated: Bool?
        public var outputBytes: Int?

        var entry: TimelineEntry? {
            TimelineEntry.from(record: record, key: "rec-\(index)")
        }
    }
    public struct Omitted: Sendable, Hashable, Decodable {
        public var recordOffset: Int
        public var bytes: Int
        public var readable: Bool
    }

    public var records: [Item]
    public var omitted: [Omitted]?
    /// Byte offset of the window start; pass as `before` to read older history. 0 means the start.
    public var before: Int
    public var totalBytes: Int?
    public var omittedLargeRecord: Bool?

    public var hasOlder: Bool { before > 0 }
}

/// A rendered timeline entry, built from history records and live events.
public enum TimelineEntry: Sendable, Hashable, Identifiable {
    case user(id: String, text: String)
    case assistant(id: String, text: String, streaming: Bool)
    case reasoning(id: String, text: String)
    case tool(id: String, name: String, summary: String, output: String, failed: Bool?)
    case notice(id: String, text: String, isError: Bool)
    case checklist(id: String, tasks: [String])
    case image(id: String, mediaType: String, base64: String)
    case largeRecord(id: String, offset: Int, bytes: Int)
    case turnTiming(id: String, seconds: Int)

    public var id: String {
        switch self {
        case .user(let id, _), .assistant(let id, _, _), .reasoning(let id, _), .tool(let id, _, _, _, _),
             .notice(let id, _, _), .checklist(let id, _), .image(let id, _, _), .largeRecord(let id, _, _),
             .turnTiming(let id, _):
            id
        }
    }

    /// Maps one history record (`record.k`) like the web `record()` renderer.
    public static func from(record r: JSONValue, key: String) -> TimelineEntry? {
        let text = r["t"]?.stringValue ?? ""
        switch r["k"]?.stringValue {
        case "user": return .user(id: key, text: text)
        case "assistant": return text.isEmpty ? nil : .assistant(id: key, text: text, streaming: false)
        case "reasoning": return text.isEmpty ? nil : .reasoning(id: key, text: text)
        case "call", "tool_call":
            let raw = r["args"]?.stringValue ?? r["arguments"]?.stringValue ?? "{}"
            let args = (try? JSONDecoder().decode(JSONValue.self, from: Data(raw.utf8))) ?? .null
            let summary = args["command"]?.stringValue ?? args["path"]?.stringValue
                ?? args["file_path"]?.stringValue ?? args["file"]?.stringValue ?? ""
            return .tool(id: r["id"]?.stringValue ?? key, name: r["name"]?.stringValue ?? "tool", summary: summary, output: "", failed: nil)
        case "bash":
            let cmd = r["cmd"]?.stringValue ?? ""
            let code = r["code"]?.doubleValue
            return .tool(id: r["id"]?.stringValue ?? key, name: "bash", summary: cmd,
                         output: "$ \(cmd)\n\(r["out"]?.stringValue ?? "")", failed: code.map { $0 != 0 })
        case "tool":
            let failed = r["err"]?.boolValue ?? (text.range(of: #"^(?:error:|\[exit [1-9]\d*\]|\[(?:killed|timed out|cancelled))"#,
                                    options: [.regularExpression, .caseInsensitive]) != nil)
            return .tool(id: r["id"]?.stringValue ?? key, name: r["name"]?.stringValue ?? "tool", summary: "", output: text, failed: failed)
        case "meta":
            return text.range(of: "error:|failed:|stopped:", options: .regularExpression) != nil
                ? .notice(id: key, text: text, isError: true) : nil
        case "custom":
            if r["c"]?.stringValue == "display/turn-seconds",
               let raw = r["d"]?.stringValue, let seconds = Int(raw), seconds >= 0 {
                return .turnTiming(id: key, seconds: seconds)
            }
            guard r["c"]?.stringValue == "state/todo", let d = r["d"]?.stringValue,
                  let board = try? JSONDecoder().decode(JSONValue.self, from: Data(d.utf8)) else { return nil }
            if board["hidden"]?.boolValue == true { return .checklist(id: key, tasks: []) }
            let tasks = (board["tasks"]?.arrayValue ?? []).compactMap { $0["text"]?.stringValue ?? $0["title"]?.stringValue ?? $0.stringValue }
            return .checklist(id: key, tasks: tasks)
        case "image":
            let type = r["media_type"]?.stringValue ?? r["mime"]?.stringValue ?? ""
            guard ["image/png", "image/jpeg", "image/gif", "image/webp"].contains(type), let data = r["data"]?.stringValue else { return nil }
            return .image(id: key, mediaType: type, base64: data)
        default: return nil
        }
    }
}
