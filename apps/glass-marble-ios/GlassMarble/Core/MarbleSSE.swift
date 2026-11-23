import Foundation

/// One parsed server-sent event frame.
struct MarbleSSEFrame: Equatable {
    var id: String?
    var event: String?
    var data: String

    /// The replay boundary captured by the host when it subscribes this client.
    var replayCursor: UInt64? {
        guard event == "hello" else { return nil }
        struct Hello: Decodable { let cursor: UInt64 }
        return (try? JSONDecoder().decode(Hello.self, from: Data(data.utf8)))?.cursor
    }
}

/// Incremental SSE parser decoupled from the transport: feed whole lines
/// (without the newline) and take the frames that complete on a blank line.
/// Handles `id:`/`event:`/`data:` fields, multi-line `data`, and `:` comments.
struct MarbleSSEParser {
    private var event = MarbleSSEFrame(id: nil, event: nil, data: "")

    /// Feed one line; returns a completed frame on a blank line, else nil.
    mutating func feed(_ line: String) -> MarbleSSEFrame? {
        if line.isEmpty {
            guard event.id != nil || event.event != nil || !event.data.isEmpty else {
                return nil
            }
            let completed = event
            event = MarbleSSEFrame(id: nil, event: nil, data: "")
            return completed
        }
        if line.hasPrefix(":") { return nil }
        let field: Substring
        var value: Substring
        if let colon = line.firstIndex(of: ":") {
            field = line[..<colon]
            value = line[line.index(after: colon)...]
            if value.hasPrefix(" ") { value = value.dropFirst() }
        } else {
            field = line[...]
            value = ""
        }
        switch field {
        case "id":
            event.id = String(value)
        case "event":
            event.event = String(value)
        case "data":
            if event.data.isEmpty {
                event.data = String(value)
            } else {
                event.data += "\n" + value
            }
        case "retry":
            break
        default:
            break
        }
        return nil
    }

    /// A frame left open when the stream ends without a trailing blank line.
    mutating func finish() -> MarbleSSEFrame? {
        feed("")
    }
}
