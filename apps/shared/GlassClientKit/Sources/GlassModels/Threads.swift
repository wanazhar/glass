import Foundation

/// One row of `GET /api/threads`.
public struct ThreadSummary: Sendable, Hashable, Codable, Identifiable {
    public var session: String
    public var title: String?
    public var workspace: String?
    public var sourceRef: String?
    public var origin: String?
    public var originLabel: String?

    /// Plain-language origin, shown only when the conversation began outside Glass.
    public var originNote: String? { origin == "gc" ? "Started in Terminal" : nil }

    public var writerActive: Bool?
    public var modified: Double?
    public var model: String?
    public var branch: String?
    public var lastStatus: String?
    public var currentWorkspace: Bool?

    public var id: String { session }
    public var displayTitle: String {
        let trimmed = title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return trimmed.isEmpty ? "Untitled conversation" : trimmed
    }
    public var modifiedDate: Date? {
        guard let modified else { return nil }
        // The gateway reports seconds; tolerate milliseconds.
        return Date(timeIntervalSince1970: modified > 10_000_000_000 ? modified / 1000 : modified)
    }

    public init(session: String, title: String? = nil, workspace: String? = nil, sourceRef: String? = nil, modified: Double? = nil) {
        self.session = session
        self.title = title
        self.workspace = workspace
        self.sourceRef = sourceRef
        self.modified = modified
    }

    enum CodingKeys: String, CodingKey {
        case session, title, workspace, sourceRef, origin, originLabel, writerActive, modified, model, branch
        case lastStatus = "last_status"
        case currentWorkspace = "current_workspace"
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        session = try c.decode(String.self, forKey: .session)
        title = try? c.decodeIfPresent(String.self, forKey: .title)
        workspace = try? c.decodeIfPresent(String.self, forKey: .workspace)
        sourceRef = try? c.decodeIfPresent(String.self, forKey: .sourceRef)
        origin = try? c.decodeIfPresent(String.self, forKey: .origin)
        originLabel = try? c.decodeIfPresent(String.self, forKey: .originLabel)
        writerActive = try? c.decodeIfPresent(Bool.self, forKey: .writerActive)
        modified = try? c.decodeIfPresent(Double.self, forKey: .modified)
        model = try? c.decodeIfPresent(String.self, forKey: .model)
        branch = try? c.decodeIfPresent(String.self, forKey: .branch)
        lastStatus = (try? c.decodeIfPresent(String.self, forKey: .lastStatus)) ?? nil
        currentWorkspace = try? c.decodeIfPresent(Bool.self, forKey: .currentWorkspace)
    }
}

public struct ProjectCount: Sendable, Hashable, Codable, Identifiable {
    public var workspace: String
    public var count: Int
    public var id: String { workspace }
    public var name: String { (workspace as NSString).lastPathComponent }
}

/// `GET /api/threads` page (512 rows per page, `next_offset` when more remain).
public struct ThreadPage: Sendable, Hashable, Codable {
    public var threads: [ThreadSummary]
    public var offset: Int
    public var totalThreads: Int?
    public var projects: [ProjectCount]?
    public var nextOffset: Int?

    enum CodingKeys: String, CodingKey {
        case threads, offset, projects
        case totalThreads = "total_threads"
        case nextOffset = "next_offset"
    }
}

/// `POST /api/open` and `POST /api/close` replies.
public struct ThreadConnection: Sendable, Hashable, Codable {
    public var session: String
    public var sourceRef: String?
    public var origin: String?
    public var originLabel: String?
    public var activeOwner: String?
}

public struct NewThread: Sendable, Hashable, Codable {
    public var id: String
}
