import Foundation

/// `GET /api/browse`: one page of a workspace directory.
public struct BrowsePage: Sendable, Hashable, Decodable {
    public struct Entry: Sendable, Hashable, Decodable, Identifiable {
        public var name: String
        public var kind: String
        public var size: Int?
        public var bytes: Int?
        public var id: String { name }
        public var isDirectory: Bool { kind == "directory" }
        public var isOpenable: Bool { kind != "special" && kind != "directory" }
        public var isHidden: Bool { name.hasPrefix(".") }
        public init(name: String, kind: String, size: Int? = nil, bytes: Int? = nil) {
            self.name = name; self.kind = kind; self.size = size; self.bytes = bytes
        }
    }

    public var path: String
    public var parent: String?
    public var entries: [Entry]
    public var next: Int?
    public var limited: Bool?

    public init(path: String, parent: String? = nil, entries: [Entry], next: Int? = nil, limited: Bool? = nil) {
        self.path = path; self.parent = parent; self.entries = entries; self.next = next; self.limited = limited
    }

    public func child(_ entry: Entry) -> String {
        (path.hasSuffix("/") ? String(path.dropLast()) : path) + "/" + entry.name
    }
}

/// `GET /api/usage`: allocated bytes under a directory.
public struct DiskUsage: Sendable, Hashable, Decodable {
    public var path: String
    public var bytes: Int
    public var entries: Int
    public var partial: Bool
}

/// `GET /api/edit`: a small text file opened for editing, with the revision a save must match.
public struct EditableFile: Sendable, Hashable, Decodable {
    public var path: String
    public var text: String
    public var revision: String
    public var maxBytes: Int?
    public init(path: String, text: String, revision: String, maxBytes: Int? = nil) {
        self.path = path; self.text = text; self.revision = revision; self.maxBytes = maxBytes
    }
}

/// `GET /api/changes`: Git status and the unstaged patch for the thread workspace.
public struct WorkspaceChanges: Sendable, Hashable, Decodable {
    public struct File: Sendable, Hashable, Decodable, Identifiable {
        public var path: String
        public var index: String
        public var worktree: String
        public var oldPath: String?
        public var id: String { path }
        public var isStaged: Bool { !index.trimmingCharacters(in: .whitespaces).isEmpty && index != "?" }
        public var isUntracked: Bool { index == "?" }
        /// A plain word for the change, as Git's two-letter code is not readable.
        public var summary: String {
            let code = index == " " ? worktree : index
            return switch code {
            case "M": "Modified"
            case "A": "Added"
            case "D": "Deleted"
            case "R": "Renamed"
            case "C": "Copied"
            case "?": "New file"
            case "U": "Conflict"
            default: "Changed"
            }
        }
    }

    public var files: [File]
    public var diff: String
    public var branch: String?
    /// Status fingerprint the host checks before staging or discarding.
    public var expected: String?

    enum CodingKeys: String, CodingKey { case files, diff, branch, expected }

    public init(files: [File], diff: String, branch: String? = nil) {
        self.files = files; self.diff = diff; self.branch = branch
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        files = try c.decodeIfPresent([File].self, forKey: .files) ?? []
        diff = try c.decodeIfPresent(String.self, forKey: .diff) ?? ""
        branch = try? c.decodeIfPresent(String.self, forKey: .branch)
        expected = try? c.decodeIfPresent(String.self, forKey: .expected)
    }

    /// Hunks of the unstaged patch for one file, each with the file header it needs to apply alone.
    public func hunks(for path: String) -> [DiffHunk] { DiffHunk.parse(diff).filter { $0.path == path } }

    /// The unstaged diff changes this file's mode, with or without line changes.
    public func changesMode(for path: String) -> Bool {
        var current = ""
        for line in diff.split(separator: "\n") {
            if line.hasPrefix("diff --git ") {
                current = line.components(separatedBy: " b/").last ?? ""
            } else if current == path, line.hasPrefix("old mode ") || line.hasPrefix("new mode ") {
                return true
            }
        }
        return false
    }
}

/// One `@@` hunk of a unified diff, carrying its file header so it can be staged or reverted alone.
public struct DiffHunk: Sendable, Hashable, Identifiable {
    public var path: String
    public var header: String
    public var lines: [String]
    public var id: String { path + "\u{0}" + (lines.first ?? "") + "\(lines.count)" }

    /// The file's header changes its mode, so applying any one hunk would change the mode too.
    /// Like the web client, line actions are withheld for such files; whole-file actions remain.
    public var changesMode: Bool {
        header.split(separator: "\n").contains { $0.hasPrefix("old mode ") || $0.hasPrefix("new mode ") }
    }

    /// The patch text the `git` command expects for `stage-patch`, `unstage-patch` and `revert-patch`.
    public var patch: String { header + lines.joined(separator: "\n") + "\n" }

    public static func parse(_ diff: String) -> [DiffHunk] {
        var hunks: [DiffHunk] = []
        var header: [String] = []
        var path = ""
        var current: [String]?
        func flush() {
            if let lines = current { hunks.append(DiffHunk(path: path, header: header.joined(separator: "\n") + "\n", lines: lines)) }
            current = nil
        }
        for line in diff.split(separator: "\n", omittingEmptySubsequences: false).map(String.init) {
            if line.hasPrefix("diff --git ") {
                flush()
                header = [line]
                path = line.components(separatedBy: " b/").last ?? ""
            } else if current == nil, !line.hasPrefix("@@") {
                if !line.isEmpty { header.append(line) }
                if line.hasPrefix("+++ b/") { path = String(line.dropFirst(6)) }
            } else if line.hasPrefix("@@") {
                flush()
                current = [line]
            } else {
                current?.append(line)
            }
        }
        flush()
        return hunks.map { hunk in
            var hunk = hunk
            while hunk.lines.last == "" { hunk.lines.removeLast() }
            return hunk
        }
    }
}

/// One teammate (sub-agent) from `GET /api/agents`.
public struct Teammate: Sendable, Hashable, Decodable, Identifiable {
    public var id: String
    public var name: String?
    public var status: String?
    public var activity: String?
    public var parentID: String?
    public var depth: Int?
    public var task: String?
    public var model: String?
    public var terminalEvents: Int?
    public var pendingActivation: Bool?
    public var offline: Bool?

    enum CodingKeys: String, CodingKey {
        case id, name, status, activity, depth, task, prompt, model, offline
        case parentID = "parent_id", terminalEvents = "terminal_events", pendingActivation = "pending_activation"
    }

    public init(id: String, name: String? = nil, status: String? = nil, activity: String? = nil, parentID: String? = nil,
                depth: Int? = nil, task: String? = nil, terminalEvents: Int? = nil, pendingActivation: Bool? = nil) {
        self.id = id; self.name = name; self.status = status; self.activity = activity; self.parentID = parentID
        self.depth = depth; self.task = task; self.terminalEvents = terminalEvents; self.pendingActivation = pendingActivation
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        name = try? c.decodeIfPresent(String.self, forKey: .name)
        status = try? c.decodeIfPresent(String.self, forKey: .status)
        activity = try? c.decodeIfPresent(String.self, forKey: .activity)
        parentID = try? c.decodeIfPresent(String.self, forKey: .parentID)
        depth = try? c.decodeIfPresent(Int.self, forKey: .depth)
        task = (try? c.decodeIfPresent(String.self, forKey: .task)) ?? (try? c.decodeIfPresent(String.self, forKey: .prompt))
        model = try? c.decodeIfPresent(String.self, forKey: .model)
        terminalEvents = try? c.decodeIfPresent(Int.self, forKey: .terminalEvents)
        pendingActivation = try? c.decodeIfPresent(Bool.self, forKey: .pendingActivation)
        offline = try? c.decodeIfPresent(Bool.self, forKey: .offline)
    }

    public var title: String { name ?? id }

    /// The same plain status words the web UI shows.
    public var statusTitle: String {
        switch status {
        case "working": activity?.localizedCaseInsensitiveContains("starting") == true ? "Starting" : "Working"
        case "idle": pendingActivation == true ? "Waiting" : (terminalEvents ?? 0) > 0 ? "Finished" : "Idle"
        case "parked": "Paused"
        case "blocked": "Blocked"
        case let other?: other.prefix(1).uppercased() + other.dropFirst()
        case nil: "Unknown"
        }
    }

    public static func decodeList(_ value: JSONValue) -> [Teammate] {
        let rows: JSONValue = if case .object(let object) = value, let agents = object["agents"] { agents } else { value }
        guard let data = try? JSONEncoder().encode(rows),
              let list = try? JSONDecoder().decode([FailableTeammate].self, from: data) else { return [] }
        return Array(list.compactMap(\.value).prefix(64))
    }

    private struct FailableTeammate: Decodable {
        var value: Teammate?
        init(from decoder: any Decoder) throws { value = try? Teammate(from: decoder) }
    }
}

/// `GET /api/agent`: a window of a teammate's transcript.
public struct TeammateTranscript: Sendable, Hashable, Decodable {
    public var text: String
    public var before: Int
    public var hasMore: Bool
    enum CodingKeys: String, CodingKey { case text, before, hasMore = "has_more" }
}

/// `GET /api/activity-text`: one page of a large saved activity record.
public struct ActivityTextPage: Sendable, Hashable, Decodable {
    public var recordOffset: Int
    public var offset: Int
    public var text: String
    public var nextOffset: Int
    public var eof: Bool
    public var totalBytes: Int
    public var kind: String?
    public var revision: String?
}

/// `POST /api/artifact`: a short-lived token for an isolated interactive HTML document.
public struct ArtifactReceipt: Sendable, Hashable, Decodable {
    public var token: String
    public var url: String
    public var expiresIn: Int
}

/// A file's bytes with the type the host reported, for previews.
public struct FileContent: Sendable, Hashable {
    public var data: Data
    public var contentType: String
    public init(data: Data, contentType: String) { self.data = data; self.contentType = contentType }
    public var isPDF: Bool { contentType.hasPrefix("application/pdf") }
    public var isImage: Bool { contentType.hasPrefix("image/") }
    public var isHTML: Bool { contentType.hasPrefix("text/html") }
    public var text: String? { isPDF || isImage ? nil : String(data: data, encoding: .utf8) }
}

/// `GET /api/agent-events` frames: a new roster revision, or the main session going offline.
public enum TeammateSignal: Sendable, Hashable {
    case revision(Int)
    case offline
}

/// `GET /api/activity-location`: where a saved message sits, for quoting and branching.
public struct ActivityLocation: Sendable, Hashable, Decodable {
    public var recordOffset: Int
    public var record: Int
    public var turn: Int
    public var revision: String?

    public init(recordOffset: Int, record: Int, turn: Int, revision: String? = nil) {
        self.recordOffset = recordOffset
        self.record = record
        self.turn = turn
        self.revision = revision
    }
}

/// Actions on a saved message, matching the web conversation's message buttons.
public enum MessageAction: String, Sendable, CaseIterable {
    case quote, branch, editInBranch

    public var title: String {
        switch self {
        case .quote: "Quote"
        case .branch: "Branch from here"
        case .editInBranch: "Edit in new branch"
        }
    }

    /// The host command and its values for a message at `location`.
    public func command(at location: ActivityLocation) -> (name: String, values: [String: JSONValue]) {
        let args = switch self {
        case .quote: location.record
        case .branch: location.turn
        case .editInBranch: max(0, location.turn - 1)
        }
        var values: [String: JSONValue] = [
            "args": .string(String(args)),
            "source_record": .number(Double(location.record)),
            "before": .bool(self == .editInBranch),
        ]
        if let revision = location.revision { values["source_expected"] = .string(revision) }
        return (self == .quote ? "quote" : "branch", values)
    }
}

extension TimelineEntry {
    /// Byte offset of the saved history record behind this entry; nil until the host has saved it.
    public var recordOffset: Int? {
        guard id.hasPrefix("rec-") else { return nil }
        return Int(id.dropFirst(4))
    }
}

/// Reasoning text for the full reader.
public enum ReasoningText {
    /// Splits very long prose paragraphs at sentence ends so they read comfortably; code, lists and headings stay as written.
    public static func readable(_ source: String) -> String {
        var fenced = false
        var output = ""
        var rest = Substring(source)
        while !rest.isEmpty {
            let gap = rest.range(of: #"\n\s*\n"#, options: .regularExpression)
            let paragraph = gap.map { rest[..<$0.lowerBound] } ?? rest
            let separator = gap.map { String(rest[$0]) } ?? ""
            rest = gap.map { rest[$0.upperBound...] } ?? ""
            output += reflow(String(paragraph), fenced: &fenced) + separator
        }
        return output
    }

    private static func reflow(_ paragraph: String, fenced: inout Bool) -> String {
        if paragraph.contains("```") {
            if paragraph.components(separatedBy: "```").count % 2 == 0 { fenced.toggle() }
            return paragraph
        }
        if fenced || paragraph.count < 520 || paragraph.range(of: #"^\s*(#|[-*>]|\d+\.)"#, options: .regularExpression) != nil {
            return paragraph
        }
        var result = ""
        var sinceBreak = 0
        var characters = Array(paragraph)
        var index = 0
        while index < characters.count {
            let character = characters[index]
            result.append(character)
            sinceBreak += 1
            if ".!?".contains(character), sinceBreak >= 300 {
                var end = index + 1
                while end < characters.count, characters[end] == " " || characters[end] == "\t" { end += 1 }
                if end > index + 1, end < characters.count, characters[end].isUppercase {
                    result += "\n\n"
                    sinceBreak = 0
                    index = end
                    continue
                }
            }
            index += 1
        }
        characters.removeAll()
        return result
    }
}
