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

/// One named marble on the Prism roster.
///
/// Identity is a shape, a hue, and two eyes; state lives on the same object so a
/// roster can be read at a glance instead of by reading every name.
struct MarblePrismMarble: Decodable, Identifiable, Equatable {
    var id: String
    var name: String
    var role: String
    var glyph: String
    var hue: Int
    var workspace: String
    var state: String
    var conversationId: String
    var taskId: String
    var lastAt: UInt64
    var runs: UInt64
    var lastError: String

    var isWorking: Bool { state == "working" }
    var stateLabel: String { state.isEmpty ? "idle" : state }
}

/// The Prism roster as the host serves it.
struct MarblePrism: Decodable, Equatable {
    var marbles: [MarblePrismMarble]
    var limit: Int?
    var swarms: UInt64?
    var dispatched: UInt64?
    var working: Int?
    var engine: Bool?

    static let empty = MarblePrism(
        marbles: [],
        limit: nil,
        swarms: nil,
        dispatched: nil,
        working: nil,
        engine: nil
    )

    var summary: String {
        let count = marbles.count
        let swarms = swarms ?? 0
        return "\(count) / \(limit ?? 8) marbles · \(swarms) swarm\(swarms == 1 ? "" : "s")"
    }
}

/// A Core-native tool the host has seen the engine use.
struct MarbleObservedTool: Decodable, Identifiable, Equatable {
    var name: String
    var runs: UInt64
    var errors: UInt64
    var lastAt: UInt64

    var id: String { name }
}

/// A `glass.*` integration Marble classifies and executes itself.
struct MarbleGovernedTool: Decodable, Identifiable, Equatable {
    var name: String
    var description: String
    var owner: String
    var effect: String

    var id: String { name }
}

/// Everything this marble can reach for, split by who owns the call.
struct MarbleToolInventory: Decodable, Equatable {
    var profile: String?
    var core: MarbleCoreTools?
    var governed: [MarbleGovernedTool]

    static let empty = MarbleToolInventory(profile: nil, core: nil, governed: [])

    var observed: [MarbleObservedTool] {
        (core?.observed ?? []).sorted { $0.lastAt > $1.lastAt }
    }
}

struct MarbleCoreTools: Decodable, Equatable {
    var nativeTools: Bool
    var observed: [MarbleObservedTool]
    var note: String?
}

struct MarbleState: Decodable, Equatable {
    var revision: Int
    var conversations: [MarbleConversation]
    var tasks: [MarbleTask]
    var approvals: [MarbleApproval]
    var transcript: [MarbleEntry]
    var control: MarbleControl?
    var presence: MarblePresence?
    var memory: MarbleMemory?
    var routines: MarbleRoutines?
    var voice: MarbleVoice?
    var notifications: MarbleNotificationSettings?
    var devices: [MarbleDevice]?
    var transcriptCount: Int?
    var prism: MarblePrism?

    static let empty = MarbleState(
        revision: 0,
        conversations: [],
        tasks: [],
        approvals: [],
        transcript: [],
        control: nil,
        presence: nil,
        memory: nil,
        routines: nil,
        voice: nil,
        notifications: nil,
        devices: nil,
        transcriptCount: 0,
        prism: nil
    )

    /// Hosts from before the control layer answer without one.
    var controlState: MarbleControl { control ?? .empty }
    /// What the assistant is doing, when the host reports it.
    var presenceInfo: MarblePresence { presence ?? .empty }
    var memoryState: MarbleMemory { memory ?? .empty }
    var routineState: MarbleRoutines { routines ?? .empty }
    var voiceSettings: MarbleVoice { voice ?? .empty }
    var notificationState: MarbleNotificationSettings { notifications ?? .empty }
    var deviceList: [MarbleDevice] { devices ?? [] }
    var prismState: MarblePrism { prism ?? .empty }

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

/// One bounded page of durable history for a conversation.
struct MarbleTranscriptPage: Decodable, Equatable {
    var conversationId: String
    var entries: [MarbleEntry]
    var cursor: UInt64
    var omitted: Int?
    var revision: Int?
}

struct MarbleAccepted: Decodable, Equatable {
    var accepted: Bool
    var requestId: String
    var conversationId: String
    var taskId: String
    /// Set when a marble's own engine took the turn instead of the chat engine.
    var marbleName: String?
}

struct MarbleHealth: Decodable, Equatable {
    var ok: Bool
    var revision: Int
    var sequence: UInt64
    var authRequired: Bool
}

/// What the always-on assistant is doing right now.
struct MarblePresence: Decodable, Equatable {
    var status: String
    var summary: String
    var instance: String
    var uptimeSeconds: UInt64
    var lastTurnAt: UInt64
    var turnsStarted: UInt64
    var turnsCompleted: UInt64
    var recoveredTasks: UInt64
    var activeTaskId: String?
    var activePrompt: String?
    var approvalsWaiting: Int
    var queued: Int
    var routinesEnabled: Int
    var memories: Int

    static let empty = MarblePresence(
        status: "offline",
        summary: "",
        instance: "",
        uptimeSeconds: 0,
        lastTurnAt: 0,
        turnsStarted: 0,
        turnsCompleted: 0,
        recoveredTasks: 0,
        activeTaskId: nil,
        activePrompt: nil,
        approvalsWaiting: 0,
        queued: 0,
        routinesEnabled: 0,
        memories: 0
    )

    var uptimeLabel: String {
        if uptimeSeconds >= 86_400 {
            return "\(uptimeSeconds / 86_400)d \((uptimeSeconds % 86_400) / 3_600)h"
        }
        if uptimeSeconds >= 3_600 {
            return "\(uptimeSeconds / 3_600)h \((uptimeSeconds % 3_600) / 60)m"
        }
        if uptimeSeconds >= 60 { return "\(uptimeSeconds / 60)m" }
        return "\(max(1, uptimeSeconds))s"
    }
}

/// One durable statement Marble carries between conversations.
struct MarbleMemoryFact: Decodable, Identifiable, Equatable {
    var id: String
    var text: String
    var kind: String
    var createdAt: UInt64
    var uses: UInt64
}

/// Something Marble noticed and wants permission to keep.
struct MarbleMemorySuggestion: Decodable, Identifiable, Equatable {
    var id: String
    var text: String
    var kind: String
    var reason: String
}

struct MarbleMemory: Decodable, Equatable {
    var facts: [MarbleMemoryFact]
    var suggestions: [MarbleMemorySuggestion]
    var count: Int?
    var limit: Int?

    static let empty = MarbleMemory(facts: [], suggestions: [], count: 0, limit: nil)
}

/// One saved prompt on a schedule.
struct MarbleRoutine: Decodable, Identifiable, Equatable {
    var id: String
    var title: String
    var prompt: String
    var schedule: String
    var conversationId: String?
    var enabled: Bool
    var createdAt: UInt64
    var lastRunAt: UInt64
    var nextRunAt: UInt64
    var runs: UInt64
    var lastTaskId: String
    var lastStatus: String
    var missedRuns: UInt64?
    var missedLastGap: UInt64?
    var catchUp: Bool?

    var nextRunLabel: String? {
        guard nextRunAt > 0 else { return nil }
        let date = Date(timeIntervalSince1970: Double(nextRunAt) / 1000)
        return "next " + date.formatted(date: .abbreviated, time: .shortened)
    }
}

struct MarbleRoutines: Decodable, Equatable {
    var routines: [MarbleRoutine]
    var count: Int?
    var limit: Int?
    var enabled: Int?
    var offsetMinutes: Int?

    static let empty = MarbleRoutines(routines: [], count: 0, limit: nil, enabled: 0, offsetMinutes: nil)
}

/// Local-only voice preferences.
struct MarbleVoice: Decodable, Equatable {
    var enabled: Bool
    var rate: Double
    var voice: String
    var provider: String?

    static let empty = MarbleVoice(enabled: false, rate: 0.5, voice: "", provider: "on-device")
}

/// APNs credentials the host signs with. The key file stays on the host.
struct MarbleApns: Codable, Equatable {
    var keyFile: String
    var teamId: String
    var keyId: String
    var topic: String
    var environment: String?
}

struct MarbleNotificationSettings: Decodable, Equatable {
    var webhookUrl: String
    var webhookTokenSet: Bool
    var events: [String]
    var availableEvents: [String]
    var apns: MarbleApns?
    var apnsConfigured: Bool
    var webhookConfigured: Bool

    static let empty = MarbleNotificationSettings(
        webhookUrl: "",
        webhookTokenSet: false,
        events: [],
        availableEvents: [],
        apns: nil,
        apnsConfigured: false,
        webhookConfigured: false
    )
}

struct MarbleDevice: Decodable, Identifiable, Equatable {
    var tokenPrefix: String
    var platform: String
    var topic: String
    var lastSeenAt: UInt64

    var id: String { tokenPrefix }
}
