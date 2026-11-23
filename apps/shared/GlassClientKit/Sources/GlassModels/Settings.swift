import Foundation

/// `GET /api/settings`: global defaults for new conversations, with a revision for safe saves.
public struct SettingsSnapshot: Sendable, Hashable, Decodable {
    public struct Defaults: Sendable, Hashable, Decodable {
        public var provider: String
        public var baseURL: String
        public var model: String
        public var maxTokens: Int
        public var reasoningBudget: Int?
        public var costLimit: Double?
        public var adaptivePacing: Bool
        public var imageInput: Bool
        public var systemPrompt: String
        public var skillsDir: String?
        public var permissionProfile: String
        public var sandbox: String
        /// The host administrator fixed the sandbox; it can't be changed here.
        public var sandboxManaged: Bool
        public var mods: [String]

        enum CodingKeys: String, CodingKey {
            case provider, model, sandbox, mods
            case baseURL = "base_url", maxTokens = "max_tokens", reasoningBudget = "reasoning_budget"
            case costLimit = "cost_limit", adaptivePacing = "adaptive_pacing", imageInput = "image_input"
            case systemPrompt = "system_prompt", skillsDir = "skills_dir"
            case permissionProfile = "permission_profile", sandboxManaged = "sandbox_managed"
        }

        public init(from decoder: any Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            provider = (try? c.decodeIfPresent(String.self, forKey: .provider)) ?? ""
            baseURL = (try? c.decodeIfPresent(String.self, forKey: .baseURL)) ?? ""
            model = (try? c.decodeIfPresent(String.self, forKey: .model)) ?? ""
            maxTokens = (try? c.decodeIfPresent(Int.self, forKey: .maxTokens)) ?? 0
            reasoningBudget = try? c.decodeIfPresent(Int.self, forKey: .reasoningBudget)
            costLimit = try? c.decodeIfPresent(Double.self, forKey: .costLimit)
            adaptivePacing = (try? c.decodeIfPresent(Bool.self, forKey: .adaptivePacing)) ?? false
            imageInput = (try? c.decodeIfPresent(Bool.self, forKey: .imageInput)) ?? false
            systemPrompt = (try? c.decodeIfPresent(String.self, forKey: .systemPrompt)) ?? ""
            skillsDir = try? c.decodeIfPresent(String.self, forKey: .skillsDir)
            permissionProfile = (try? c.decodeIfPresent(String.self, forKey: .permissionProfile)) ?? "unrestricted"
            sandbox = (try? c.decodeIfPresent(String.self, forKey: .sandbox)) ?? "off"
            sandboxManaged = (try? c.decodeIfPresent(Bool.self, forKey: .sandboxManaged)) ?? false
            mods = (try? c.decodeIfPresent([String].self, forKey: .mods)) ?? []
        }
    }

    public struct Connection: Sendable, Hashable, Identifiable, Decodable {
        public var id: String
        public var name: String
        public var provider: String
        public var endpoint: String
        public var model: String
        public var hasCredential: Bool
        enum CodingKeys: String, CodingKey { case id, name, provider, endpoint, model, hasCredential = "has_credential" }
        public init(from decoder: any Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            id = try c.decode(String.self, forKey: .id)
            name = (try? c.decodeIfPresent(String.self, forKey: .name)) ?? id
            provider = (try? c.decodeIfPresent(String.self, forKey: .provider)) ?? ""
            endpoint = (try? c.decodeIfPresent(String.self, forKey: .endpoint)) ?? ""
            model = (try? c.decodeIfPresent(String.self, forKey: .model)) ?? ""
            hasCredential = (try? c.decodeIfPresent(Bool.self, forKey: .hasCredential)) ?? false
        }
    }

    public struct Extension: Sendable, Hashable, Identifiable, Decodable {
        public var id: String
        public var name: String
        public var description: String
        public var enabled: Bool
    }

    public struct Profile: Sendable, Hashable, Identifiable, Decodable {
        public var name: String
        public var model: String
        public var instructions: String
        public var adaptivePacing: Bool
        public var workspaces: [String]
        public var id: String { name }
        enum CodingKeys: String, CodingKey { case name, model, instructions, workspaces, adaptivePacing = "adaptive_pacing" }
        public init(from decoder: any Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            name = try c.decode(String.self, forKey: .name)
            model = (try? c.decodeIfPresent(String.self, forKey: .model)) ?? ""
            instructions = (try? c.decodeIfPresent(String.self, forKey: .instructions)) ?? ""
            adaptivePacing = (try? c.decodeIfPresent(Bool.self, forKey: .adaptivePacing)) ?? false
            workspaces = (try? c.decodeIfPresent([String].self, forKey: .workspaces)) ?? []
        }
    }

    public struct Skill: Sendable, Hashable, Identifiable, Decodable {
        public var name: String
        public var description: String
        public var id: String { name }
    }

    /// Returned after starting a ChatGPT sign-in: open `authorizeURL`, then paste the callback URL.
    public struct SignIn: Sendable, Hashable, Decodable {
        public var authorizeURL: String
        public var attempt: String?
        enum CodingKeys: String, CodingKey { case authorizeURL = "authorize_url", attempt }
    }

    public var revision: String
    public var scope: String
    public var applies: String
    public var defaults: Defaults
    public var activeConnection: String?
    public var connections: [Connection]
    public var availableMods: [String]
    public var extensions: [Extension]
    public var profiles: [Profile]
    public var skills: [Skill]
    public var signIn: SignIn?

    enum CodingKeys: String, CodingKey {
        case revision, scope, applies, defaults, connections, extensions, profiles, skills
        case activeConnection = "active_connection", availableMods = "available_mods", signIn = "sign_in"
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        revision = try c.decode(String.self, forKey: .revision)
        scope = (try? c.decodeIfPresent(String.self, forKey: .scope)) ?? "global"
        applies = (try? c.decodeIfPresent(String.self, forKey: .applies)) ?? "new_conversations"
        defaults = try c.decode(Defaults.self, forKey: .defaults)
        activeConnection = (try? c.decodeIfPresent(String.self, forKey: .activeConnection)).flatMap { $0 }
        connections = (try? c.decodeIfPresent([Connection].self, forKey: .connections)) ?? []
        availableMods = (try? c.decodeIfPresent([String].self, forKey: .availableMods)) ?? []
        extensions = (try? c.decodeIfPresent([Extension].self, forKey: .extensions)) ?? []
        profiles = ((try? c.decodeIfPresent([Profile].self, forKey: .profiles)) ?? []).sorted { $0.name < $1.name }
        skills = (try? c.decodeIfPresent([Skill].self, forKey: .skills)) ?? []
        signIn = try? c.decodeIfPresent(SignIn.self, forKey: .signIn)
    }
}

/// One `POST /api/settings` change. The body always carries the revision it was based on.
public enum SettingsChange: Sendable, Hashable {
    case preference(key: String, value: JSONValue)
    case extensionEnabled(id: String, enabled: Bool)
    case useConnection(id: String)
    case connectionModel(id: String, model: String)
    case removeConnection(id: String)
    case addConnection(label: String, provider: String, endpoint: String, model: String, secret: String)
    case startSignIn
    case completeSignIn(attempt: String?, callbackURL: String, model: String, label: String)
    case updateProfile(name: String, model: String? = nil, instructions: String? = nil)
    case createProfile(name: String)
    case deleteProfile(name: String)
    case assignProfile(name: String, workspace: String)

    /// Carries a credential, so it is only sent over HTTPS.
    public var carriesSecret: Bool {
        switch self {
        case .addConnection, .completeSignIn: true
        default: false
        }
    }

    public func body(revision: String) -> JSONValue {
        var o: [String: JSONValue] = ["revision": .string(revision)]
        switch self {
        case .preference(let key, let value):
            o["action"] = "preference"; o["key"] = .string(key); o["value"] = value
        case .extensionEnabled(let id, let enabled):
            o["action"] = "extension"; o["id"] = .string(id); o["enabled"] = .bool(enabled)
        case .useConnection(let id):
            o["action"] = "connection-use"; o["id"] = .string(id)
        case .connectionModel(let id, let model):
            o["action"] = "connection-model"; o["id"] = .string(id); o["model"] = .string(model)
        case .removeConnection(let id):
            o["action"] = "connection-remove"; o["id"] = .string(id); o["confirmed"] = true
        case .addConnection(let label, let provider, let endpoint, let model, let secret):
            o["action"] = "connection-add"; o["label"] = .string(label); o["provider"] = .string(provider)
            o["endpoint"] = .string(endpoint); o["model"] = .string(model); o["secret"] = .string(secret)
        case .startSignIn:
            o["action"] = "oauth"
        case .completeSignIn(let attempt, let callbackURL, let model, let label):
            o["action"] = "callback"; o["args"] = attempt.map(JSONValue.string) ?? .null
            o["model"] = .string(model); o["label"] = .string(label); o["secret"] = .string(callbackURL)
        case .updateProfile(let name, let model, let instructions):
            o["action"] = "profile"; o["name"] = .string(name)
            if let model { o["model"] = .string(model) }
            if let instructions { o["instructions"] = .string(instructions) }
        case .createProfile(let name):
            o["action"] = "profile-create"; o["name"] = .string(name)
        case .deleteProfile(let name):
            o["action"] = "profile-delete"; o["name"] = .string(name); o["confirmed"] = true
        case .assignProfile(let name, let workspace):
            o["action"] = "profile-assign"; o["name"] = .string(name); o["workspace"] = .string(workspace)
        }
        return .object(o)
    }

    /// The host refused the save because settings changed since the snapshot was read.
    public static func isConflict(_ message: String) -> Bool {
        message.contains("changed elsewhere")
    }

    /// Default endpoint for a provider, as the web UI fills it in.
    public static func defaultEndpoint(provider: String) -> String {
        provider == "anthropic" ? "https://api.anthropic.com" : "https://api.openai.com/v1"
    }

    public static let providers: [(id: String, title: String)] = [
        ("openai", "OpenAI compatible"), ("openai-responses", "OpenAI Responses"), ("anthropic", "Anthropic"),
    ]
    public static let permissionProfiles: [(id: String, title: String)] = [
        ("unrestricted", "Unrestricted"), ("workspace-edit", "Workspace edit"), ("read-only", "Read-only"),
    ]
    public static let sandboxes: [(id: String, title: String)] = [
        ("off", "Off"), ("workspace", "Workspace"), ("strict", "Strict"),
    ]
}
