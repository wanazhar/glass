import Foundation

/// One entry from `GET /api/commands` (or a `command_catalogue` event).
public struct CommandDescriptor: Sendable, Hashable, Identifiable, Decodable {
    public enum Group: String, Sendable, Hashable, CaseIterable, Decodable {
        case conversations, work, settings
        public var title: String {
            switch self {
            case .conversations: "Conversations"
            case .work: "Work"
            case .settings: "Settings"
            }
        }
    }

    public var name: String
    public var description: String
    public var hint: String
    public var aliases: [String]
    public var keywords: String
    public var primary: Bool
    public var group: Group
    /// The host can run it for a native client.
    public var portable: Bool
    /// Answered by `POST /api/command` without the control channel.
    public var readonly: Bool
    public var form: CommandForm

    public var id: String { name }

    enum CodingKeys: String, CodingKey {
        case name, description, hint, aliases, keywords, primary, group, portable, readonly, form
    }

    public init(name: String, description: String = "", hint: String = "", aliases: [String] = [],
                keywords: String = "", primary: Bool = false, group: Group = .work,
                portable: Bool = true, readonly: Bool = false, form: CommandForm = CommandForm()) {
        self.name = name; self.description = description; self.hint = hint; self.aliases = aliases
        self.keywords = keywords; self.primary = primary; self.group = group
        self.portable = portable; self.readonly = readonly; self.form = form
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        name = try c.decode(String.self, forKey: .name)
        description = (try? c.decodeIfPresent(String.self, forKey: .description))?
            .replacingOccurrences(of: " · local", with: "") ?? ""
        hint = (try? c.decodeIfPresent(String.self, forKey: .hint)) ?? ""
        aliases = (try? c.decodeIfPresent([String].self, forKey: .aliases)) ?? []
        keywords = (try? c.decodeIfPresent(String.self, forKey: .keywords)) ?? ""
        primary = (try? c.decodeIfPresent(Bool.self, forKey: .primary)) ?? false
        group = (try? c.decodeIfPresent(Group.self, forKey: .group)) ?? .work
        portable = (try? c.decodeIfPresent(Bool.self, forKey: .portable)) ?? false
        readonly = (try? c.decodeIfPresent(Bool.self, forKey: .readonly)) ?? false
        form = (try? c.decodeIfPresent(CommandForm.self, forKey: .form)) ?? CommandForm()
    }

    /// Case-insensitive match on name, aliases, description and keywords; a leading `/` is ignored.
    public func matches(_ query: String) -> Bool {
        var q = query.trimmingCharacters(in: .whitespaces).lowercased()
        if q.hasPrefix("/") { q.removeFirst() }
        guard !q.isEmpty else { return true }
        return ([name, description, keywords] + aliases).contains { $0.lowercased().contains(q) }
    }

    /// Opening the command loads its current state first (as the web UI does).
    public var inspectsOnOpen: Bool { readonly || Self.inspectOnOpen.contains(name) }

    /// Running it can be cancelled from the client.
    public var cancellable: Bool { Self.cancellable.contains(name) }

    /// Field keys shown for an `action`, or nil when every field is shown.
    public func visibleKeys(for action: String?) -> [String]? {
        guard let action, let keys = CommandVariants.table[name]?[action] else { return nil }
        return keys
    }

    public func visibleFields(for action: String?) -> [CommandField] {
        guard let keys = visibleKeys(for: action) else { return form.fields }
        return form.fields.filter { $0.key == "action" || keys.contains($0.key) }
    }

    /// The primary button title for the selected action.
    public func runLabel(action: String?) -> String {
        if readonly || ["inspect", "list", "status", "poll"].contains(action ?? "") { return "Refresh" }
        if let action, !action.isEmpty {
            let words = action.replacingOccurrences(of: "-", with: " ")
            return words.prefix(1).uppercased() + words.dropFirst()
        }
        return form.fields.isEmpty ? "Run" : "Apply"
    }

    /// Ask the owner to confirm before a change runs.
    public var needsConfirmation: Bool { form.confirm || name == "todo" }

    /// The `values` object the host expects: strings for every visible, filled field, plus `confirmed`.
    public func values(_ input: [String: String], confirmed: Bool) -> JSONValue {
        let action = input["action"] ?? form.fields.first { $0.key == "action" }?.choices?.first
        var out: [String: JSONValue] = [:]
        for field in visibleFields(for: action) {
            let value = field.key == "action" ? (action ?? "") : (input[field.key] ?? "")
            out[field.key] = .string(value)
        }
        out["confirmed"] = .bool(confirmed)
        return .object(out)
    }

    static let inspectOnOpen: Set<String> = [
        "handoff", "todo", "budget", "pacing", "model", "mods", "jobs", "security", "prewalk",
        "constraints", "loop", "neuralnets", "profiles", "login", "plan", "advisor", "git", "pr",
        "agents", "extensions", "tasks", "queue", "context", "status", "settings-info", "doctor", "quote",
    ]
    static let cancellable: Set<String> = ["compact", "btw", "code-review", "plan", "init", "doctor"]
}

public struct CommandForm: Sendable, Hashable, Decodable {
    public var fields: [CommandField]
    public var confirm: Bool
    /// Running it spends model tokens.
    public var modelRequest: Bool

    enum CodingKeys: String, CodingKey { case fields, confirm, modelRequest = "model_request" }

    public init(fields: [CommandField] = [], confirm: Bool = false, modelRequest: Bool = false) {
        self.fields = fields; self.confirm = confirm; self.modelRequest = modelRequest
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        fields = (try? c.decodeIfPresent([CommandField].self, forKey: .fields)) ?? []
        confirm = (try? c.decodeIfPresent(Bool.self, forKey: .confirm)) ?? false
        modelRequest = (try? c.decodeIfPresent(Bool.self, forKey: .modelRequest)) ?? false
    }
}

public struct CommandField: Sendable, Hashable, Identifiable, Decodable {
    public enum Kind: String, Sendable, Hashable, Decodable {
        case text, textarea, number, url, password
        public init(from decoder: any Decoder) throws {
            self = Kind(rawValue: try decoder.singleValueContainer().decode(String.self)) ?? .text
        }
    }

    public var key: String
    public var label: String
    public var kind: Kind
    public var id: String { key }

    public init(key: String, label: String, kind: Kind = .text) {
        self.key = key; self.label = label; self.kind = kind
    }

    /// `action` and `kind` fields list their choices in the label: `"inspect | probe"`.
    public var choices: [String]? {
        guard ["action", "kind"].contains(key), kind == .text, label.contains("|") else { return nil }
        return label.split(separator: "|").compactMap {
            $0.trimmingCharacters(in: .whitespaces).split(separator: " ").first.map(String.init)
        }
    }

    public static func choiceTitle(_ choice: String) -> String {
        let words = choice.replacingOccurrences(of: "-", with: " ").replacingOccurrences(of: "_", with: " ")
        return words.prefix(1).uppercased() + words.dropFirst()
    }
}

public enum CommandCatalogue {
    /// Decodes `{"commands":[…]}`, a bare array, or a `command_catalogue` payload, skipping malformed entries.
    public static func decode(_ value: JSONValue) -> [CommandDescriptor] {
        let list = value["commands"]?.arrayValue ?? value.arrayValue ?? []
        return list.compactMap { item in
            guard let data = try? JSONEncoder().encode(item) else { return nil }
            return try? JSONDecoder().decode(CommandDescriptor.self, from: data)
        }
    }

    /// Non-primary commands matching `query`, grouped in catalogue order.
    public static func grouped(_ commands: [CommandDescriptor], query: String) -> [(CommandDescriptor.Group, [CommandDescriptor])] {
        CommandDescriptor.Group.allCases.compactMap { group in
            let items = commands.filter { $0.group == group && $0.matches(query) }
            return items.isEmpty ? nil : (group, items)
        }
    }
}

/// A command reply: `{"title":"/usage","body":…,"format":"markdown"}`.
public struct CommandOutput: Sendable, Hashable {
    public var title: String?
    public var body: JSONValue
    public var format: String?

    public init(_ value: JSONValue) {
        if case .object(let object) = value, object["body"] != nil {
            title = object["title"]?.stringValue
            body = object["body"] ?? .null
            format = object["format"]?.stringValue
        } else {
            title = nil; body = value; format = nil
        }
    }

    public var isMarkdown: Bool { format == "markdown" }
}

/// Human labels for result keys, matching the web UI.
public enum CommandLabel {
    static let titles: [String: String] = [
        "fs_scan": "File search cache", "env": "Environment", "extensionIsolation": "Extension isolation",
        "expected": "Revision", "cost_usd": "Reported cost (USD)", "cwd": "Working directory",
        "args": "Arguments", "stdout": "Output", "stderr": "Errors", "id": "ID", "url": "URL",
        "api": "API", "input": "Input", "output": "Output",
    ]
    static let unlimited: Set<String> = ["tokenLimit", "token_limit", "remaining_tokens", "round_limit"]
    /// Keys whose string value reads as a block of text.
    public static let literalKeys: Set<String> = ["output", "stdout", "stderr", "patch", "system", "instructions"]
    public static let proseKeys: Set<String> = ["proposal", "draft", "answer", "text", "diff", "preview"]
    /// Keys used to name an element of an array.
    public static let identityKeys = ["name", "title", "label", "command", "path", "model", "session", "id", "record"]

    public static func title(_ key: String) -> String {
        if let title = titles[key] { return title }
        var out = ""
        var previous: Character?
        for ch in key.replacingOccurrences(of: "_", with: " ") {
            if ch.isUppercase, let p = previous, p.isLowercase { out.append(" ") }
            out.append(ch)
            previous = ch
        }
        let sentence = out.lowercased()
        return (sentence.prefix(1).uppercased() + sentence.dropFirst()).replacingOccurrences(of: " usd", with: " USD")
    }

    public static func display(_ value: JSONValue, key: String = "") -> String {
        switch value {
        case .null: unlimited.contains(key) ? "Unlimited" : "Not reported"
        case .bool(let b): b ? "Yes" : "No"
        case .number(let n): n.rounded() == n && abs(n) < 1e15 ? String(Int64(n)) : String(n)
        case .string(let s): s.isEmpty ? "None" : s
        case .array(let a): "\(a.count) items"
        case .object(let o): "\(o.count) fields"
        }
    }

    /// The best name for one element of a result array.
    public static func identity(_ value: JSONValue) -> String? {
        for key in identityKeys {
            if let s = value[key]?.stringValue, !s.isEmpty { return s }
        }
        return nil
    }
}
