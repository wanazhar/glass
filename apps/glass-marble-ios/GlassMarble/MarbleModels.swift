import Foundation

/// Arbitrary JSON carried by tool arguments.
enum MarbleJSON: Decodable, Hashable {
    case string(String)
    case number(Double)
    case bool(Bool)
    case object([String: MarbleJSON])
    case array([MarbleJSON])
    case null

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() {
            self = .null
        } else if let value = try? container.decode(Bool.self) {
            self = .bool(value)
        } else if let value = try? container.decode(Double.self) {
            self = .number(value)
        } else if let value = try? container.decode(String.self) {
            self = .string(value)
        } else if let value = try? container.decode([String: MarbleJSON].self) {
            self = .object(value)
        } else if let value = try? container.decode([MarbleJSON].self) {
            self = .array(value)
        } else {
            throw DecodingError.dataCorruptedError(
                in: container,
                debugDescription: "Unsupported JSON value in tool arguments"
            )
        }
    }

    /// Compact one-line rendering used by approval cards.
    var summary: String {
        switch self {
        case .string(let value):
            return value
        case .number(let value):
            return value.rounded() == value ? String(Int(value)) : String(value)
        case .bool(let value):
            return value ? "true" : "false"
        case .null:
            return "null"
        case .array(let values):
            return "[" + values.map(\.summary).joined(separator: ", ") + "]"
        case .object(let values):
            return values.keys.sorted()
                .map { key in "\(key): \(values[key]?.summary ?? "")" }
                .joined(separator: ", ")
        }
    }
}

struct MarbleConversation: Decodable, Identifiable, Equatable {
    var id: String
    var title: String
    var taskIds: [String]
    var updatedAt: UInt64
}

struct MarbleTask: Decodable, Identifiable, Equatable {
    var id: String
    var conversationId: String
    var status: String
    var owner: String
    var prompt: String
    var at: UInt64

    var isRunning: Bool { status == "running" }
}

struct MarbleApproval: Decodable, Identifiable, Equatable {
    var requestId: String
    var callId: String
    var tool: String
    var arguments: MarbleJSON

    var id: String { requestId }

    var argumentsSummary: String { arguments.summary }
}

struct MarbleEntry: Decodable, Identifiable, Equatable {
    var conversationId: String
    var kind: String
    var text: String
    var taskId: String
    var at: UInt64

    var id: String { "\(conversationId)|\(taskId)|\(kind)|\(at)|\(text.count)|\(text.hashValue)" }

    /// A streamed entry that has not reached the durable transcript yet.
    static func preview(kind: String, text: String) -> MarbleEntry {
        MarbleEntry(conversationId: "live", kind: kind, text: text, taskId: "", at: 0)
    }
}

/// One selectable posture offered by the host's control layer.
struct MarbleProfileOption: Decodable, Identifiable, Equatable {
    var id: String
    var label: String
    var description: String
    var allowsWrites: Bool
    var prompts: Bool
}

/// A standing answer the host remembers until it is revoked.
struct MarbleGrant: Decodable, Identifiable, Equatable {
    var id: String
    var tool: String
    var pathPrefix: String
    var createdAt: UInt64
    var uses: UInt64

    var scopeLabel: String { pathPrefix.isEmpty ? "whole workspace" : pathPrefix }
}

/// One journalled control decision.
struct MarbleControlEntry: Decodable, Identifiable, Equatable {
    var at: UInt64
    var conversationId: String
    var taskId: String
    var tool: String
    var path: String
    var decision: String
    var reason: String

    var id: String { "\(at)|\(tool)|\(path)|\(decision)|\(reason)" }
}

/// A message the host is holding while admission is paused.
struct MarbleQueuedTurn: Decodable, Identifiable, Equatable {
    var id: String
    var conversationId: String?
    var text: String
    var at: UInt64
}

/// Marble's own layer of control as served by the host.
struct MarbleControl: Decodable, Equatable {
    var profile: String
    var paused: Bool
    var grants: [MarbleGrant]
    var deniedPaths: [String]
    var ledger: [MarbleControlEntry]
    var queue: [MarbleQueuedTurn]
    var profiles: [MarbleProfileOption]

    static let empty = MarbleControl(
        profile: "workspace",
        paused: false,
        grants: [],
        deniedPaths: [],
        ledger: [],
        queue: [],
        profiles: []
    )

    func label(for profile: String) -> String {
        profiles.first { $0.id == profile }?.label ?? profile
    }
}

struct MarbleState: Decodable, Equatable {
    var revision: Int
    var conversations: [MarbleConversation]
    var tasks: [MarbleTask]
    var approvals: [MarbleApproval]
    var transcript: [MarbleEntry]
    var control: MarbleControl?

    static let empty = MarbleState(
        revision: 0,
        conversations: [],
        tasks: [],
        approvals: [],
        transcript: [],
        control: nil
    )

    /// Hosts from before the control layer answer without one.
    var controlState: MarbleControl { control ?? .empty }

    func conversation(_ id: String?) -> MarbleConversation? {
        guard let id else { return nil }
        return conversations.first { $0.id == id }
    }

    func transcript(for id: String?) -> [MarbleEntry] {
        guard let id else { return transcript }
        return transcript.filter { $0.conversationId == id }
    }

    var runningTask: MarbleTask? { tasks.first(where: \.isRunning) }
}

struct MarbleEvent: Decodable, Equatable {
    var sequence: UInt64
    var conversationId: String
    var taskId: String
    var kind: String
    var data: MarbleEventData
}

struct MarbleEventData: Decodable, Equatable {
    var text: String?
    var tool: String?
    var isError: Bool?
    var message: String?
    var status: String?
    var revision: Int?
    var taskId: String?
}

struct MarbleAccepted: Decodable, Equatable {
    var accepted: Bool
    var requestId: String
    var conversationId: String
    var taskId: String
}

struct MarbleHealth: Decodable, Equatable {
    var ok: Bool
    var revision: Int
    var sequence: UInt64
    var authRequired: Bool
}
