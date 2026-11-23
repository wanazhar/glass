import Foundation

/// Glass reply components, schema v1: a fenced `glass-ui` block holding one
/// JSON object `{type, v, fallback, example?, data}`. Clients draw the types
/// they know and show the Markdown `fallback` for everything else.
public struct ReplyComponent: Sendable, Hashable {
    public static let schemaVersion = 1
    public static let fence = "glass-ui"
    public static let maxBytes = 128 * 1024
    public static let types = [
        "badges", "icon_list", "entities", "kpi", "buttons", "counter", "copy",
        "choices", "form", "table", "chart", "image", "gallery", "carousel", "map",
        "disclosure", "checklist", "progress", "timeline", "html", "product",
        "business", "reservation", "video", "sources", "file", "writing", "weather",
        "sports", "currency", "flights", "hotels", "jobs", "news", "flow",
    ]

    public var type: String
    public var version: Int
    public var fallback: String?
    public var example: Bool
    public var data: JSONValue

    public init(type: String, version: Int = Self.schemaVersion, fallback: String?, example: Bool = false, data: JSONValue) {
        self.type = type; self.version = version; self.fallback = fallback; self.example = example; self.data = data
    }

    /// Parses a block body. Streaming may split long lines; JSON never needs raw newlines.
    public init?(source: String) {
        guard source.utf8.count <= Self.maxBytes else { return nil }
        let joined = source.replacingOccurrences(of: "\r", with: "").replacingOccurrences(of: "\n", with: "")
        guard let bytes = joined.trimmingCharacters(in: .whitespaces).data(using: .utf8),
              let value = try? JSONDecoder().decode(JSONValue.self, from: bytes),
              let type = value["type"]?.stringValue else { return nil }
        self.type = type
        version = value["v"]?.doubleValue.map { Int($0) } ?? Self.schemaVersion
        fallback = value["fallback"]?.stringValue
        example = value["example"]?.boolValue ?? false
        data = value["data"] ?? .object([:])
    }

    /// Whether this client draws the component natively.
    public func isDrawable(supported: [String] = Self.types) -> Bool {
        version == Self.schemaVersion && supported.contains(type)
    }

    /// Markdown shown when the component isn't drawn.
    public var fallbackText: String {
        let text = (fallback ?? "").trimmingCharacters(in: .whitespacesAndNewlines)
        let body = text.isEmpty ? "(\(type.replacingOccurrences(of: "_", with: " ")))" : text
        return example && !body.hasPrefix("_Example") ? "_Example data, not live._\n\n" + body : body
    }

    /// Text a form, choice group or button puts into the message box.
    public static func answer(title: String?, _ entries: [(String, String)]) -> String {
        ([title.map { "\($0):" }].compactMap { $0 } + entries.map { "- \($0.0): \($0.1)" }).joined(separator: "\n")
    }
}

/// What a client tells Core it can render, sent with every prompt.
public struct ClientRender: Sendable, Hashable {
    public var name: String
    public var components: [String]

    public init(name: String, components: [String]) { self.name = name; self.components = components }

    public static let ios = ClientRender(name: "ios", components: ReplyComponent.types)

    public var json: JSONValue {
        .object([
            "name": .string(name),
            "schema": .number(Double(ReplyComponent.schemaVersion)),
            "components": .array(components.map(JSONValue.string)),
        ])
    }
}

extension JSONValue {
    /// A field as display text: strings as-is, numbers without a trailing `.0`.
    public var text: String? {
        switch self {
        case .string(let value): value
        case .number(let value): value.rounded() == value && abs(value) < 1e15 ? String(Int64(value)) : String(value)
        case .bool(let value): value ? "Yes" : "No"
        default: nil
        }
    }

    public var number: Double? {
        switch self {
        case .number(let value): value
        case .string(let value): Double(value)
        default: nil
        }
    }

    /// `data[key]` as an array, empty when missing.
    public func list(_ key: String) -> [JSONValue] { self[key]?.arrayValue ?? [] }
}
