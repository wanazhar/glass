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

struct MarbleConversation: Codable, Identifiable, Equatable {
    var id: String
    var title: String
    var taskIds: [String]
    var updatedAt: UInt64

    var displayTitle: String { title.isEmpty ? "New Chat" : title }
    var isEveryoneSession: Bool { id == everyoneConversationId || id.hasPrefix("everyone-session-") }
}

struct MarbleTask: Decodable, Identifiable, Equatable {
    var id: String
    var conversationId: String
    var status: String
    var owner: String
    var prompt: String
    var at: UInt64
    var marbleId: String? = nil

    var isRunning: Bool { status == "running" }
}

struct MarbleApproval: Decodable, Identifiable, Equatable {
    var requestId: String
    var callId: String
    var tool: String
    var arguments: MarbleJSON
    /// Set when another chat also has work in this call's path.
    var conflict: String?

    var id: String { requestId }

    var argumentsSummary: String { arguments.summary }
}

struct MarbleToolDetails: Codable, Equatable {
    var command: String
    var output: String
    var elapsedMs: UInt64?
}

struct MarbleEntry: Codable, Identifiable, Equatable {
    var serverID: String? = nil
    var clientMessageID: String? = nil
    var presentationID: String? = nil
    var failedSendID: UUID? = nil
    var threadId: String? = nil
    var thread: MarbleThreadSummary? = nil
    var failure: MarbleFailureInfo? = nil
    var cancelled: Bool? = nil
    var toolDetails: MarbleToolDetails? = nil
    var conversationId: String
    var kind: String
    var text: String
    var taskId: String
    var at: UInt64
    /// Who said it: a marble id, "marble" for the default assistant, "user",
    /// or empty on hosts from before the shared plane.
    var speaker: String
    var speakerName: String
    /// One marble asking another for help, when this row records an ask.
    var handoff: MarbleHandoff?

    enum CodingKeys: String, CodingKey {
        case conversationId, kind, text, taskId, at, speaker, speakerName, handoff
        case serverID = "id"
        case clientMessageID = "client_message_id"
        case threadId, thread, failure, cancelled, toolDetails
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let threadFields = try decoder.container(keyedBy: MarbleThreadKey.self)
        let id = try container.decodeIfPresent(String.self, forKey: .serverID)
        serverID = id?.isEmpty == false ? id : nil
        clientMessageID = try threadFields.optional("client_message_id", "clientMessageId")
        let membership: String? = try threadFields.optional("thread_id", "threadId")
        threadId = membership?.isEmpty == false ? membership : nil
        thread = try container.decodeIfPresent(MarbleThreadSummary.self, forKey: .thread)
        failure = try container.decodeIfPresent(MarbleFailureInfo.self, forKey: .failure)
        cancelled = try container.decodeIfPresent(Bool.self, forKey: .cancelled)
        toolDetails = try container.decodeIfPresent(MarbleToolDetails.self, forKey: .toolDetails)
        conversationId = try container.decode(String.self, forKey: .conversationId)
        kind = try container.decode(String.self, forKey: .kind)
        text = try container.decode(String.self, forKey: .text)
        taskId = try container.decodeIfPresent(String.self, forKey: .taskId) ?? ""
        at = try container.decode(UInt64.self, forKey: .at)
        speaker = try container.decodeIfPresent(String.self, forKey: .speaker) ?? ""
        speakerName = try container.decodeIfPresent(String.self, forKey: .speakerName) ?? ""
        handoff = try container.decodeIfPresent(MarbleHandoff.self, forKey: .handoff)
    }

    init(
        conversationId: String, kind: String, text: String, taskId: String,
        at: UInt64, speaker: String = "", speakerName: String = "",
        handoff: MarbleHandoff? = nil
    ) {
        self.conversationId = conversationId
        self.kind = kind
        self.text = text
        self.taskId = taskId
        self.at = at
        self.speaker = speaker
        self.speakerName = speakerName
        self.handoff = handoff
    }

    var id: String { serverID ?? presentationID ?? "\(conversationId)|\(taskId)|\(kind)|\(at)|\(text.count)|\(text.hashValue)" }
    var rowID: String { presentationID ?? clientMessageID.map { "outgoing-\($0.lowercased())" } ?? id }

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

/// One executable a crate ships, and whether the host can run it.
struct MarbleCrateBinary: Decodable, Identifiable, Equatable {
    var name: String
    var onPath: String?

    var id: String { name }
}

/// One crate in the workspace the host was started inside.
struct MarbleWorkspaceCrate: Decodable, Identifiable, Equatable {
    var name: String
    var crateName: String
    var version: String
    var description: String
    var path: String
    var binaries: [MarbleCrateBinary]
    var dependsOn: [String]
    /// `library` when the host links the crate, `binary` when it ships an
    /// executable on `PATH`. Empty means another crate owns the route.
    var reach: [String]

    var id: String { name }

    var reachLabel: String {
        reach.isEmpty ? "no direct reach" : reach.joined(separator: " · ")
    }
}

/// The workspace crate map, served with the control document.
struct MarbleCrateMap: Decodable, Equatable {
    struct Workspace: Decodable, Equatable {
        var version: String
        var members: Int
        var marble: String?
    }

    var workspace: Workspace
    var crates: [MarbleWorkspaceCrate]
    var truncated: Bool

    static let empty = MarbleCrateMap(
        workspace: Workspace(version: "", members: 0, marble: nil),
        crates: [],
        truncated: false
    )
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
    // The crate map is fetched from `/api/crates` when the sheet that shows it
    // opens: it is a workspace scan, not a fact every state poll should carry.

    enum CodingKeys: String, CodingKey {
        case profile, paused, grants, deniedPaths, ledger, queue, profiles
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        profile = try container.decode(String.self, forKey: .profile)
        paused = try container.decode(Bool.self, forKey: .paused)
        grants = try container.decodeIfPresent([MarbleGrant].self, forKey: .grants) ?? []
        deniedPaths = try container.decodeIfPresent([String].self, forKey: .deniedPaths) ?? []
        ledger = try container.decodeIfPresent([MarbleControlEntry].self, forKey: .ledger) ?? []
        queue = try container.decodeIfPresent([MarbleQueuedTurn].self, forKey: .queue) ?? []
        profiles = try container.decodeIfPresent([MarbleProfileOption].self, forKey: .profiles) ?? []
    }

    init(
        profile: String, paused: Bool, grants: [MarbleGrant], deniedPaths: [String],
        ledger: [MarbleControlEntry], queue: [MarbleQueuedTurn], profiles: [MarbleProfileOption]
    ) {
        self.profile = profile
        self.paused = paused
        self.grants = grants
        self.deniedPaths = deniedPaths
        self.ledger = ledger
        self.queue = queue
        self.profiles = profiles
    }

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

/// One named marble in the roster.
///
/// Identity is a shape, a hue, and a name; state lives on the same object so a
/// roster can be read at a glance instead of by reading every name.
struct MarbleAgentCard: Codable, Equatable {
    enum Speaking: String, Codable, CaseIterable {
        case mentioned, relevant, always

        var label: String {
            switch self {
            case .mentioned: "Mentioned only"
            case .relevant: "Relevant"
            case .always: "Always"
            }
        }
    }

    struct Tools: Codable, Equatable {
        var native = false
        var allowed: [String] = []
    }

    var voice = ""
    var model = ""
    var whenToSpeak = Speaking.relevant
    var tools = Tools()
    var inEveryone = false
    var toolCallLimit: Int?
    var turnTimeoutSecs: Int?
}

struct MarblePrismMarble: Codable, Identifiable, Equatable {
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
    /// Who the marble is, in its own words — editable from its info screen.
    var persona: String
    /// The built-in assistant: fixed shape, its chats are the chats list.
    var isDefault: Bool
    var card: MarbleAgentCard?
    var reportedWorking: Bool? = nil
    var reportedActiveTaskIds: [String]? = nil

    enum CodingKeys: String, CodingKey {
        case id, name, role, glyph, hue, workspace, state, conversationId
        case taskId, lastAt, runs, lastError, persona, isDefault, card
        case reportedWorking = "isWorking"
        case reportedActiveTaskIds = "activeTaskIds"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        name = try container.decode(String.self, forKey: .name)
        role = try container.decodeIfPresent(String.self, forKey: .role) ?? ""
        glyph = try container.decodeIfPresent(String.self, forKey: .glyph) ?? "circle"
        hue = try container.decodeIfPresent(Int.self, forKey: .hue) ?? 0
        workspace = try container.decodeIfPresent(String.self, forKey: .workspace) ?? ""
        state = try container.decodeIfPresent(String.self, forKey: .state) ?? "idle"
        conversationId = try container.decodeIfPresent(String.self, forKey: .conversationId) ?? ""
        taskId = try container.decodeIfPresent(String.self, forKey: .taskId) ?? ""
        lastAt = try container.decodeIfPresent(UInt64.self, forKey: .lastAt) ?? 0
        runs = try container.decodeIfPresent(UInt64.self, forKey: .runs) ?? 0
        lastError = try container.decodeIfPresent(String.self, forKey: .lastError) ?? ""
        persona = try container.decodeIfPresent(String.self, forKey: .persona) ?? ""
        isDefault = try container.decodeIfPresent(Bool.self, forKey: .isDefault) ?? false
        card = try container.decodeIfPresent(MarbleAgentCard.self, forKey: .card)
        reportedWorking = try container.decodeIfPresent(Bool.self, forKey: .reportedWorking)
        reportedActiveTaskIds = try container.decodeIfPresent([String].self, forKey: .reportedActiveTaskIds)
    }

    init(
        id: String, name: String, role: String, glyph: String, hue: Int,
        workspace: String, state: String, conversationId: String, taskId: String,
        lastAt: UInt64, runs: UInt64, lastError: String,
        persona: String = "", isDefault: Bool = false
    ) {
        self.id = id
        self.name = name
        self.role = role
        self.glyph = glyph
        self.hue = hue
        self.workspace = workspace
        self.state = state
        self.conversationId = conversationId
        self.taskId = taskId
        self.lastAt = lastAt
        self.runs = runs
        self.lastError = lastError
        self.persona = persona
        self.isDefault = isDefault
    }

    func isWorking(presence: MarbleAgentPresence? = nil) -> Bool {
        if let reportedWorking { return reportedWorking }
        if let reportedActiveTaskIds { return !reportedActiveTaskIds.isEmpty }
        if let presence { return presence.status == "working" || !presence.activeTaskIds.isEmpty }
        return state == "working"
    }

    func activeTaskIds(presence: MarbleAgentPresence?) -> [String] {
        if let reportedActiveTaskIds { return reportedActiveTaskIds }
        if let presence { return presence.activeTaskIds }
        return taskId.isEmpty ? [] : [taskId]
    }

    /// A person-facing state word: what the row should say under the name.
    func displayState(presence: MarbleAgentPresence?) -> String {
        if isWorking(presence: presence) { return "Working…" }
        switch state {
        case "blocked": return "Needs you"
        case "done": return "Finished"
        default: return "Idle"
        }
    }
    /// The one-line personality shown under the name: its persona, else its
    /// original role, else nothing.
    var tagline: String { persona.isEmpty ? role : persona }
    var isEveryoneMember: Bool { isDefault || (card?.inEveryone ?? true) }
}

/// The Prism roster as the host serves it.
struct MarblePrism: Codable, Equatable {
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
    var transcriptCount: Int?
    var prism: MarblePrism?
    var model: String?

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
        transcriptCount: 0,
        prism: nil,
        model: nil
    )

    /// Hosts from before the control layer answer without one.
    var controlState: MarbleControl { control ?? .empty }
    /// What the assistant is doing, when the host reports it.
    var presenceInfo: MarblePresence { presence ?? .empty }
    var memoryState: MarbleMemory { memory ?? .empty }
    var routineState: MarbleRoutines { routines ?? .empty }
    var voiceSettings: MarbleVoice { voice ?? .empty }
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

    /// The running turn in one conversation. Several chats run at once, so a
    /// surface showing one chat asks about that chat rather than about the host.
    func runningTask(for conversationId: String?) -> MarbleTask? {
        guard let conversationId else { return runningTask }
        return tasks.first { $0.isRunning && $0.conversationId == conversationId }
    }
}

struct MarbleEvent: Decodable, Equatable {
    var sequence: UInt64
    var conversationId: String
    var taskId: String
    var kind: String
    var data: MarbleEventData
    var entryUpdate: MarbleEntry? = nil
    var userEntry: MarbleEntry? {
        guard entryUpdate?.kind == "user", entryUpdate?.clientMessageID != nil else { return nil }
        return entryUpdate
    }
}

extension MarbleEvent {
    private enum CodingKeys: String, CodingKey { case sequence, conversationId, taskId, kind, data }

    init(from decoder: Decoder) throws {
        let fields = try decoder.container(keyedBy: CodingKeys.self)
        sequence = try fields.decode(UInt64.self, forKey: .sequence)
        conversationId = try fields.decode(String.self, forKey: .conversationId)
        taskId = try fields.decode(String.self, forKey: .taskId)
        kind = try fields.decode(String.self, forKey: .kind)
        data = try fields.decode(MarbleEventData.self, forKey: .data)
        if kind == "entry", var entry = try? fields.decode(MarbleEntry.self, forKey: .data),
           entry.serverID != nil {
            entry.conversationId = conversationId
            entryUpdate = entry
        }
    }
}

struct MarbleEventData: Decodable, Equatable {
    var thread: MarbleThread? = nil
    var threadId: String? = nil
    var entryId: String? = nil
    var toolEntryId: String? = nil
    var sender: String? = nil
    var entryKind: String? = nil
    var text: String?
    var tool: String?
    var name: String? = nil
    var title: String? = nil
    var callId: String? = nil
    var arguments: MarbleJSON? = nil
    var isError: Bool?
    var message: String?
    var status: String?
    var revision: Int?
    var taskId: String?
    var reason: String?
    var cancelled: Bool?
    var requestId: String?
    var approved: Bool?
    var failure: MarbleFailureInfo? = nil
    /// On an `everyone` event, how many entries a clear removed.
    var cleared: Int? = nil
    /// Conversation ID on a successful deletion event.
    var deleted: String? = nil

    private enum CodingKeys: String, CodingKey {
        case thread, text, tool, name, title, callId, arguments, isError, message, status, revision
        case taskId, reason, cancelled, requestId, approved, cleared, deleted, sender, failure
        case threadId = "thread_id"
        case entryId = "id"
        case toolEntryId = "entry_id"
        case entryKind = "kind"
    }
}

/// One marble-to-marble ask, kept on the transcript row that tells it.
struct MarbleHandoff: Codable, Equatable {
    var from: String
    var fromName: String
    var to: String
    var toName: String
    var instruction: String
    /// `asked`, `finished`, `refused`, or `failed`.
    var status: String
    var summary: String

    enum CodingKeys: String, CodingKey {
        case from, fromName, to, toName, instruction, status, summary
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        from = try container.decodeIfPresent(String.self, forKey: .from) ?? ""
        fromName = try container.decodeIfPresent(String.self, forKey: .fromName) ?? ""
        to = try container.decodeIfPresent(String.self, forKey: .to) ?? ""
        toName = try container.decodeIfPresent(String.self, forKey: .toName) ?? ""
        instruction = try container.decodeIfPresent(String.self, forKey: .instruction) ?? ""
        status = try container.decodeIfPresent(String.self, forKey: .status) ?? ""
        summary = try container.decodeIfPresent(String.self, forKey: .summary) ?? ""
    }

    init(
        from: String, fromName: String, to: String, toName: String,
        instruction: String, status: String, summary: String
    ) {
        self.from = from
        self.fromName = fromName
        self.to = to
        self.toName = toName
        self.instruction = instruction
        self.status = status
        self.summary = summary
    }
}

/// What `POST /api/everyone` answers: who is answering and who is busy when
/// marbles were named, or the default turn's task when none were.
struct MarbleEveryoneReply: Decodable, Equatable {
    var ok: Bool
    var answering: [String]
    var busy: [String]
    /// Set when the default Marble took the message as an ordinary turn.
    var taskId: String?
    var entryId: String? = nil
    var clientMessageID: String? = nil

    enum CodingKeys: String, CodingKey {
        case ok, answering, busy, taskId, accepted
    }

    init(ok: Bool, answering: [String], busy: [String], taskId: String? = nil) {
        self.ok = ok
        self.answering = answering
        self.busy = busy
        self.taskId = taskId
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let accepted = try container.decodeIfPresent(Bool.self, forKey: .accepted)
        ok = try container.decodeIfPresent(Bool.self, forKey: .ok) ?? accepted ?? true
        answering = try container.decodeIfPresent([String].self, forKey: .answering) ?? []
        busy = try container.decodeIfPresent([String].self, forKey: .busy) ?? []
        taskId = try container.decodeIfPresent(String.self, forKey: .taskId)
        let fields = try decoder.container(keyedBy: MarbleThreadKey.self)
        entryId = try fields.optional("entry_id", "entryId")
        clientMessageID = try fields.optional("client_message_id", "clientMessageId")
    }
}

/// One transcript-search hit, newest first.
struct MarbleSearchHit: Decodable, Identifiable, Equatable {
    var conversationId: String
    var title: String
    var kind: String
    var text: String
    var at: UInt64
    var taskId: String

    var id: String { "\(conversationId)|\(taskId)|\(at)" }
}

/// One bounded page of durable history for a conversation.
struct MarbleTranscriptPage: Decodable, Equatable {
    var conversationId: String
    var entries: [MarbleEntry]
    var cursor: UInt64
    var omitted: Int?
    var revision: Int?
    var hasMore: Bool?
    var nextBefore: String?
}

struct MarbleEveryonePage: Decodable {
    var entries: [MarbleEntry]
    var hasMore: Bool?
    var nextBefore: String?
}

struct MarbleAccepted: Decodable, Equatable {
    var accepted: Bool
    var requestId: String
    var conversationId: String
    var taskId: String
    /// Set when a marble's own engine took the turn instead of the chat engine.
    var marbleName: String?
    var entry_id: String? = nil
    var client_message_id: String? = nil
}

struct MarbleHealth: Decodable, Equatable {
    var ok: Bool
    var version: String?
    var revision: Int
    var sequence: UInt64
    var authRequired: Bool
    var status: String?
    var runtime: RuntimeHealth?
    var pid: Int? = nil
    var build: Build? = nil

    struct Build: Decodable, Equatable {
        var version: String
        var gitCommit: String
        var gitCommitShort: String
    }

    struct RuntimeHealth: Decodable, Equatable {
        var health: String
        var lastError: String?
    }
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
    var agents: [String: MarbleAgentPresence]? = nil

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

struct MarbleAgentPresence: Decodable, Equatable {
    let status: String
    let activeTaskIds: [String]
}

/// One durable statement Marble carries between conversations.
struct MarbleMemoryFact: Decodable, Identifiable, Equatable {
    var id: String
    var text: String
    var kind: String
    var createdAt: UInt64
    var uses: UInt64
    /// Empty means shared with every marble; otherwise the owning marble's id.
    var owner: String

    enum CodingKeys: String, CodingKey {
        case id, text, kind, createdAt, uses, owner
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        id = try container.decode(String.self, forKey: .id)
        text = try container.decode(String.self, forKey: .text)
        kind = try container.decodeIfPresent(String.self, forKey: .kind) ?? "fact"
        createdAt = try container.decode(UInt64.self, forKey: .createdAt)
        uses = try container.decodeIfPresent(UInt64.self, forKey: .uses) ?? 0
        owner = try container.decodeIfPresent(String.self, forKey: .owner) ?? ""
    }

    init(
        id: String, text: String, kind: String, createdAt: UInt64,
        uses: UInt64, owner: String = ""
    ) {
        self.id = id
        self.text = text
        self.kind = kind
        self.createdAt = createdAt
        self.uses = uses
        self.owner = owner
    }
}

/// Something Marble noticed and wants permission to keep.
struct MarbleMemorySuggestion: Decodable, Identifiable, Equatable {
    var id: String
    var text: String
    var kind: String
    var reason: String
    var conversationId: String? = nil
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

/// A quiet-hours window, in host minutes past midnight with an offset to the
/// client's timezone.
struct MarbleQuietHours: Codable, Equatable {
    var enabled: Bool
    var startMinute: Int
    var endMinute: Int
    var offsetMinutes: Int

    static let off = MarbleQuietHours(enabled: false, startMinute: 1320, endMinute: 420, offsetMinutes: 0)

    private func clock(_ minute: Int) -> String {
        String(format: "%02d:%02d", minute / 60 % 24, minute % 60)
    }

    var label: String {
        enabled ? "\(clock(startMinute))–\(clock(endMinute))" : "off"
    }
}

struct MarbleRoutines: Decodable, Equatable {
    var routines: [MarbleRoutine]
    var count: Int?
    var limit: Int?
    var enabled: Int?
    var offsetMinutes: Int?
    var quietHours: MarbleQuietHours?

    static let empty = MarbleRoutines(routines: [], count: 0, limit: nil, enabled: 0, offsetMinutes: nil, quietHours: nil)
}

/// Local-only voice preferences.
struct MarbleVoice: Decodable, Equatable {
    var enabled: Bool
    var rate: Double
    var voice: String
    var provider: String?

    static let empty = MarbleVoice(enabled: false, rate: 0.5, voice: "", provider: "on-device")
}

/// The /api/turns request body: exactly the composer text and the conversation
/// the person is looking at — nothing else, so a stale prompt can never ride
/// along inside a later send.
public struct MarbleTurnBody: Encodable, Equatable, Sendable {
    public var text: String
    public var conversationId: String?
    public var threadId: String?
    public var clientMessageId: UUID?

    enum CodingKeys: String, CodingKey {
        case text, conversationId
        case threadId = "thread_id"
        case clientMessageId = "client_message_id"
    }

    public init(text: String, conversationId: String?, threadId: String? = nil, clientMessageId: UUID? = nil) {
        self.text = text
        self.conversationId = conversationId
        self.threadId = threadId
        self.clientMessageId = clientMessageId
    }
}
