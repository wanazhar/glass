import Foundation

/// One `data:` payload of `GET /api/events`: either a reset snapshot or new events after a cursor.
public struct EventFrame: Sendable, Hashable, Decodable {
    public var reset: Bool
    public var cursor: UInt64
    public var active: Bool
    public var control: JSONValue?
    public var interaction: JSONValue?
    public var live: [GlassEvent]
    public var liveClipped: Bool
    public var events: [GlassEvent]

    enum CodingKeys: String, CodingKey { case reset, cursor, active, control, interaction, live, liveClipped, events }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        reset = try c.decodeIfPresent(Bool.self, forKey: .reset) ?? false
        cursor = try c.decodeIfPresent(UInt64.self, forKey: .cursor) ?? 0
        active = try c.decodeIfPresent(Bool.self, forKey: .active) ?? false
        control = try c.decodeIfPresent(JSONValue.self, forKey: .control)
        interaction = try c.decodeIfPresent(JSONValue.self, forKey: .interaction).flatMap { $0 == .null ? nil : $0 }
        live = try c.decodeIfPresent([GlassEvent].self, forKey: .live) ?? []
        liveClipped = try c.decodeIfPresent(Bool.self, forKey: .liveClipped) ?? false
        events = try c.decodeIfPresent([GlassEvent].self, forKey: .events) ?? []
    }

    public var canWrite: Bool { control?["can_write"]?.boolValue ?? false }
}

/// A live event (`t` field). Unknown types decode to `.unknown` so new gateway events never break the stream.
public enum GlassEvent: Sendable, Hashable, Decodable {
    case turnStart(requestID: String?, text: String?, alreadyRecorded: Bool)
    case text(delta: String)
    case reasoning(delta: String)
    case tool(callID: String?, name: String, command: String)
    case toolEnd(callID: String?, output: String, summary: String, truncated: Bool)
    case done
    case waiting
    case queued(requestID: String?)
    case duplicate(message: String?)
    case steering(requestID: String?)
    case error(message: String, settled: Bool, provider: Bool, requestID: String?)
    case notice(message: String)
    case disconnected(message: String?)
    case control(canWrite: Bool)
    case interaction(JSONValue)
    case interactionDone
    case commandStart(name: String?)
    case commandDone
    /// A `command` waits for the running turn; it will run once.
    case commandQueued(requestID: String?)
    /// Reply to a `command` message: `result` or `error`, matched by request ID.
    case commandResult(requestID: String?, result: JSONValue?, error: String?)
    case commandCatalogue(JSONValue)
    case meta(JSONValue)
    case unknown(type: String, payload: JSONValue)

    public init(from decoder: any Decoder) throws {
        let v = try JSONValue(from: decoder)
        let s: (String) -> String? = { v[$0]?.stringValue }
        let b: (String) -> Bool = { v[$0]?.boolValue ?? false }
        let type = s("t") ?? ""
        switch type {
        case "turn_start": self = .turnStart(requestID: s("request_id"), text: s("text"), alreadyRecorded: b("already_recorded"))
        case "text": self = .text(delta: s("delta") ?? "")
        case "reasoning": self = .reasoning(delta: s("delta") ?? "")
        case "tool": self = .tool(callID: s("call_id"), name: s("name") ?? "tool", command: s("command") ?? "")
        case "tool_end": self = .toolEnd(callID: s("call_id"), output: s("output") ?? "", summary: s("summary") ?? "", truncated: b("truncated"))
        case "done": self = .done
        case "waiting": self = .waiting
        case "queued": self = .queued(requestID: s("request_id"))
        case "duplicate": self = .duplicate(message: s("message"))
        case "steering": self = .steering(requestID: s("request_id"))
        case "error", "provider_error":
            self = .error(message: s("message") ?? "The operation failed", settled: b("settled"), provider: type == "provider_error", requestID: s("request_id"))
        case "retrying", "provider_notice":
            let attempt = v["attempt"]?.doubleValue.map { " · retry \(Int($0))" } ?? ""
            self = .notice(message: s("message") ?? "Provider cooldown\(attempt)")
        case "disconnected": self = .disconnected(message: s("message"))
        case "control": self = .control(canWrite: b("can_write"))
        case "interaction": self = .interaction(v)
        case "interaction_done": self = .interactionDone
        case "command_start": self = .commandStart(name: s("name"))
        case "command_done": self = .commandDone
        case "command_queued": self = .commandQueued(requestID: s("request_id"))
        case "command_result": self = .commandResult(requestID: s("request_id"), result: v["result"], error: s("error"))
        case "command_catalogue": self = .commandCatalogue(v["commands"] ?? .null)
        case "meta": self = .meta(v)
        default: self = .unknown(type: type, payload: v)
        }
    }
}
