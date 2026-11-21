import Foundation

/// `GET /api/status`.
public struct GatewayStatus: Sendable, Hashable, Codable {
    public var name: String
    public var workspace: String
    public var remote: Bool
    public var profile: String?
    public var connectCommand: String?
    public var secretSubmissionUnlocked: Bool
    public var webWorkflows: Int?
    public var interactiveArtifacts: Int?
    public var protocolVersion: Int?
    public var capabilities: [String]?
    public var security: JSONValue?

    public init(
        name: String = "Glass",
        workspace: String,
        remote: Bool,
        profile: String? = nil,
        connectCommand: String? = nil,
        secretSubmissionUnlocked: Bool = false,
        webWorkflows: Int? = nil,
        interactiveArtifacts: Int? = nil,
        protocolVersion: Int? = nil,
        capabilities: [String]? = nil,
        security: JSONValue? = nil
    ) {
        self.name = name
        self.workspace = workspace
        self.remote = remote
        self.profile = profile
        self.connectCommand = connectCommand
        self.secretSubmissionUnlocked = secretSubmissionUnlocked
        self.webWorkflows = webWorkflows
        self.interactiveArtifacts = interactiveArtifacts
        self.protocolVersion = protocolVersion
        self.capabilities = capabilities
        self.security = security
    }

    enum CodingKeys: String, CodingKey {
        case name, workspace, remote, profile, connectCommand, security, protocolVersion, capabilities
        case secretSubmissionUnlocked = "secret_submission_unlocked"
        case webWorkflows = "web_workflows"
        case interactiveArtifacts = "interactive_artifacts"
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decodeIfPresent(String.self, forKey: .name) ?? "Glass"
        workspace = try c.decodeIfPresent(String.self, forKey: .workspace) ?? ""
        remote = try c.decodeIfPresent(Bool.self, forKey: .remote) ?? false
        profile = try c.decodeIfPresent(String.self, forKey: .profile)
        connectCommand = try c.decodeIfPresent(String.self, forKey: .connectCommand)
        secretSubmissionUnlocked = try c.decodeIfPresent(Bool.self, forKey: .secretSubmissionUnlocked) ?? false
        webWorkflows = try c.decodeIfPresent(Int.self, forKey: .webWorkflows)
        interactiveArtifacts = try c.decodeIfPresent(Int.self, forKey: .interactiveArtifacts)
        protocolVersion = try c.decodeIfPresent(Int.self, forKey: .protocolVersion)
        capabilities = try c.decodeIfPresent([String].self, forKey: .capabilities)
        security = try c.decodeIfPresent(JSONValue.self, forKey: .security)
    }

    /// The last path component of the workspace, as the web header shows it.
    public var workspaceName: String {
        let trimmed = workspace.hasSuffix("/") ? String(workspace.dropLast()) : workspace
        return trimmed.split(separator: "/").last.map(String.init) ?? trimmed
    }
}
