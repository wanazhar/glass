import Foundation

/// One `POST /api/send` `message`, encoded exactly as the web UI sends it.
public enum SendMessage: Sendable, Hashable {
    case prompt(text: String, requestID: String)
    /// A correction applied to the running turn.
    case steer(text: String, requestID: String)
    /// Stops the running turn, or declines the pending interaction.
    case cancel
    case cancelCommand(requestID: String)
    case claimControl
    case releaseControl
    case answer(interactionID: String, answer: JSONValue)
    case command(name: String, values: JSONValue, requestID: String)

    public var json: JSONValue {
        switch self {
        case .prompt(let text, let id):
            .object(["t": "prompt", "text": .string(text), "request_id": .string(id), "client": ClientRender.ios.json])
        case .steer(let text, let id):
            .object(["t": "steer", "text": .string(text), "request_id": .string(id), "client": ClientRender.ios.json])
        case .cancel: .object(["t": "cancel"])
        case .cancelCommand(let id): .object(["t": "cancel_command", "request_id": .string(id)])
        case .claimControl: .object(["t": "control", "action": "claim"])
        case .releaseControl: .object(["t": "control", "action": "release"])
        case .answer(let id, let answer): .object(["t": "answer", "id": .string(id), "answer": answer])
        case .command(let name, let values, let id):
            .object(["t": "command", "name": .string(name), "values": values, "request_id": .string(id)])
        }
    }

    /// 32 lowercase hex characters, like the web UI's request IDs.
    public static func newRequestID() -> String {
        UUID().uuidString.replacingOccurrences(of: "-", with: "").lowercased()
    }
}

extension JSONValue: ExpressibleByStringLiteral, ExpressibleByBooleanLiteral {
    public init(stringLiteral value: String) { self = .string(value) }
    public init(booleanLiteral value: Bool) { self = .bool(value) }
}

/// A turn paused for the owner: an approval, questions, or a secret.
public struct Interaction: Sendable, Hashable, Identifiable {
    public struct Option: Sendable, Hashable {
        public var label: String
        public var detail: String?

        public init(label: String, detail: String?) { self.label = label; self.detail = detail }
    }

    public struct Question: Sendable, Hashable {
        public var text: String
        public var multiSelect: Bool
        public var options: [Option]

        public init(text: String, multiSelect: Bool, options: [Option]) {
            self.text = text; self.multiSelect = multiSelect; self.options = options
        }
    }

    public enum Kind: Sendable, Hashable {
        case approval(reason: String, operation: JSONValue)
        case questions([Question])
        case secret(purpose: String, destination: String, secureSubmission: Bool)
        case other(String)
    }

    public var id: String
    public var kind: Kind

    public init?(_ value: JSONValue) {
        guard let id = value["id"]?.stringValue, let request = value["request"] else { return nil }
        self.id = id
        let detail = request["request"]
        switch request["kind"]?.stringValue {
        case "approval":
            kind = .approval(reason: request["reason"]?.stringValue ?? "", operation: request["operation"] ?? .null)
        case "secret":
            kind = .secret(purpose: detail?["purpose"]?.stringValue ?? "",
                           destination: detail?["destination"]?.stringValue ?? "",
                           secureSubmission: detail?["secureSubmission"]?.boolValue ?? false)
        case "question":
            let questions = (detail?["questions"]?.arrayValue ?? []).map { q in
                Question(text: q["question"]?.stringValue ?? "",
                         multiSelect: q["multiSelect"]?.boolValue ?? false,
                         options: (q["options"]?.arrayValue ?? []).compactMap { o in
                             o["label"]?.stringValue.map { Option(label: $0, detail: o["description"]?.stringValue) }
                         })
            }
            kind = .questions(questions)
        case let other:
            kind = .other(other ?? "")
        }
    }

    public static func approval(allow: Bool) -> JSONValue { .object(["allow": .bool(allow)]) }

    /// One list of chosen labels (or a single written answer) per question.
    public static func answers(_ answers: [[String]]) -> JSONValue {
        .object(["status": "answered",
                 "answers": .array(answers.map { .object(["answers": .array($0.map(JSONValue.string))]) })])
    }

    public static let cancelled: JSONValue = .object(["status": "cancelled"])
}

/// `GET /api/context`: attachments active in the thread's model context.
public struct ContextAttachment: Sendable, Hashable, Decodable, Identifiable {
    public var path: String
    public var source: String?
    public var bytes: Int?
    public var mediaType: String?

    public var id: String { path }

    public init(path: String, source: String?, bytes: Int?, mediaType: String?) {
        self.path = path; self.source = source; self.bytes = bytes; self.mediaType = mediaType
    }

    /// The uploaded file name, else the path's last component.
    public var label: String {
        if let source, let range = source.range(of: #"^Uploaded (.*?) ·"#, options: .regularExpression) {
            return String(source[range].dropFirst("Uploaded ".count).dropLast(2))
        }
        return path.split(separator: "/").last.map(String.init) ?? path
    }

    public var isImage: Bool { mediaType?.hasPrefix("image/") == true }
}

public struct ContextPage: Sendable, Hashable, Decodable {
    public var attachments: [ContextAttachment]
}

/// `POST /api/upload` reply. The token is attached with the `attach` command.
public struct UploadReceipt: Sendable, Hashable, Decodable {
    public var token: String
    public var name: String
    public var bytes: Int
    public var kind: String
}

/// What the host accepts before an upload leaves the device.
public enum UploadPolicy {
    public static let imageLimit = 8 * 1024 * 1024
    public static let textLimit = 32 * 1024
    public static let filesPerBatch = 8

    public enum Kind: Sendable, Hashable { case image, text }

    public enum Rejection: Error, Sendable, Hashable, LocalizedError {
        case empty(String)
        case imageTooLarge(String)
        case notText(String)

        public var errorDescription: String? {
            switch self {
            case .empty(let name): "\(name) is empty."
            case .imageTooLarge(let name): "\(name) exceeds 8 MiB."
            case .notText(let name): "\(name) must be a PNG, JPEG, GIF or WebP image, or a UTF-8 text file of at most 32 KiB."
            }
        }
    }

    public static func imageType(_ data: Data) -> String? {
        let b = [UInt8](data.prefix(12))
        if b.starts(with: [0x89, 0x50, 0x4E, 0x47]) { return "image/png" }
        if b.starts(with: [0xFF, 0xD8, 0xFF]) { return "image/jpeg" }
        if b.starts(with: [0x47, 0x49, 0x46, 0x38]) { return "image/gif" }
        if b.count == 12, b.starts(with: [0x52, 0x49, 0x46, 0x46]), Array(b[8..<12]) == [0x57, 0x45, 0x42, 0x50] { return "image/webp" }
        return nil
    }

    public static func check(_ data: Data, name: String) throws(Rejection) -> Kind {
        guard !data.isEmpty else { throw .empty(name) }
        if imageType(data) != nil {
            guard data.count <= imageLimit else { throw .imageTooLarge(name) }
            return .image
        }
        guard data.count <= textLimit, !data.contains(0), String(data: data, encoding: .utf8) != nil else { throw .notText(name) }
        return .text
    }
}

/// A task from the `todo` command's board.
public struct ChecklistTask: Sendable, Hashable, Identifiable {
    public var id: String
    public var title: String
    public var status: String

    public var isDone: Bool { status == "completed" }
    public var isAbandoned: Bool { status == "abandoned" }

    /// Reads `body.tasks` from a `todo` command result; nil when the board is hidden or absent.
    public static func board(from result: JSONValue) -> [ChecklistTask]? {
        let body = result["body"] ?? result
        guard let tasks = body["tasks"]?.arrayValue else { return nil }
        if body["hidden"]?.boolValue == true { return [] }
        return tasks.enumerated().compactMap { index, task in
            guard let title = task["title"]?.stringValue ?? task["content"]?.stringValue else { return nil }
            return ChecklistTask(id: task["id"]?.stringValue ?? "\(index)-\(title)", title: title,
                                 status: task["status"]?.stringValue ?? "pending")
        }
    }
}

/// Bounded device-local drafts, one per thread: 24 drafts, 30 days, 60,000 characters.
public struct DraftStore: Sendable, Hashable, Codable {
    public static let maxDrafts = 24
    public static let maxAge: TimeInterval = 30 * 86_400
    public static let maxCharacters = 60_000

    public struct Draft: Sendable, Hashable, Codable {
        public var text: String
        public var at: Date
    }

    public private(set) var drafts: [String: Draft] = [:]

    public init() {}

    public func read(_ thread: String) -> String { drafts[thread].map { String($0.text.prefix(Self.maxCharacters)) } ?? "" }

    public mutating func write(_ thread: String, _ text: String, now: Date = .now) {
        if text.isEmpty { drafts[thread] = nil } else { drafts[thread] = Draft(text: String(text.prefix(Self.maxCharacters)), at: now) }
        prune(now: now)
    }

    public mutating func prune(now: Date = .now) {
        let kept = drafts.filter { now.timeIntervalSince($0.value.at) < Self.maxAge }
            .sorted { $0.value.at > $1.value.at }
            .prefix(Self.maxDrafts)
        drafts = Dictionary(uniqueKeysWithValues: kept.map { ($0.key, Draft(text: String($0.value.text.prefix(Self.maxCharacters)), at: $0.value.at)) })
    }

    /// Reads a saved store, dropping anything out of bounds. A damaged file starts empty.
    public static func load(from url: URL, now: Date = .now) -> DraftStore {
        guard let data = try? Data(contentsOf: url), var store = try? JSONDecoder().decode(DraftStore.self, from: data) else { return DraftStore() }
        store.prune(now: now)
        return store
    }

    public func save(to url: URL) throws {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try JSONEncoder().encode(self).write(to: url, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
    }
}
