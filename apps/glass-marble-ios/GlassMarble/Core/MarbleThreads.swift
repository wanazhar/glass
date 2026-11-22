import Foundation

/// New thread/search routes use snake_case. Accept camelCase replies too,
/// matching the rest of the host's transcript format.
struct MarbleThreadKey: CodingKey {
    var stringValue: String
    var intValue: Int? { nil }

    init(_ value: String) { stringValue = value }
    init?(stringValue: String) { self.init(stringValue) }
    init?(intValue: Int) { return nil }
}

extension KeyedDecodingContainer<MarbleThreadKey> {
    func optional<T: Decodable>(_ key: String, _ alias: String? = nil) throws -> T? {
        if let value = try decodeIfPresent(T.self, forKey: .init(key)) { return value }
        guard let alias else { return nil }
        return try decodeIfPresent(T.self, forKey: .init(alias))
    }

    func required<T: Decodable>(_ key: String, _ alias: String? = nil) throws -> T {
        if let value: T = try optional(key, alias) { return value }
        return try decode(T.self, forKey: .init(key))
    }
}

struct MarbleThreadSummary: Codable, Hashable, Sendable {
    let id: String
    let replyCount: Int
    let lastActivityAt: UInt64

    init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: MarbleThreadKey.self)
        id = try values.required("id")
        replyCount = try values.required("reply_count", "replyCount")
        lastActivityAt = try values.required("last_activity_at", "lastActivityAt")
    }

    var replyLabel: String { replyCount == 1 ? "1 reply" : "\(replyCount) replies" }
}

struct MarbleThread: Decodable, Identifiable, Hashable, Sendable {
    let summary: MarbleThreadSummary
    let conversationId: String
    let parentEntryId: String
    let parentPreview: String
    let parentSender: String
    let createdAt: UInt64
    let participants: [String]

    var id: String { summary.id }

    init(from decoder: Decoder) throws {
        summary = try MarbleThreadSummary(from: decoder)
        let values = try decoder.container(keyedBy: MarbleThreadKey.self)
        conversationId = try values.required("conversation_id", "conversationId")
        parentEntryId = try values.required("parent_entry_id", "parentEntryId")
        parentPreview = try values.optional("parent_preview", "parentPreview") ?? ""
        parentSender = try values.optional("parent_sender", "parentSender") ?? ""
        createdAt = try values.required("created_at", "createdAt")
        participants = try values.optional("participants") ?? []
    }
}

struct MarbleThreadPage: Decodable {
    let thread: MarbleThread
    let entries: [MarbleEntry]
}

struct MarbleSearchResult: Decodable, Identifiable, Hashable, Sendable {
    let id: String
    let entryId: String?
    let conversationId: String
    let threadId: String?
    let sender: String
    let createdAt: UInt64
    let snippet: String
    let matchRanges: [[Int]]

    init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: MarbleThreadKey.self)
        let entryId: String = try values.required("entry_id", "entryId")
        self.id = entryId
        self.entryId = entryId
        conversationId = try values.required("conversation_id", "conversationId")
        threadId = try values.optional("thread_id", "threadId")
        sender = try values.optional("sender") ?? ""
        createdAt = try values.required("created_at", "createdAt")
        snippet = try values.required("snippet")
        matchRanges = try values.optional("match_ranges", "matchRanges") ?? []
    }

    init(legacy hit: MarbleSearchHit) {
        id = hit.id
        entryId = nil
        conversationId = hit.conversationId
        threadId = nil
        sender = hit.kind == "user" ? "You" : hit.title
        createdAt = hit.at
        snippet = hit.text
        matchRanges = []
    }

    /// Offsets from the host are UTF-8 byte offsets in the returned snippet.
    /// Ignore malformed or overlapping ranges without losing the snippet.
    var highlights: [Range<String.Index>] {
        var ranges: [Range<String.Index>] = []
        for pair in matchRanges.sorted(by: { ($0.first ?? 0) < ($1.first ?? 0) }) {
            guard pair.count == 2, pair[0] >= 0, pair[1] > 0,
                  let start = snippet.utf8.index(snippet.utf8.startIndex, offsetBy: pair[0], limitedBy: snippet.utf8.endIndex),
                  let end = snippet.utf8.index(start, offsetBy: pair[1], limitedBy: snippet.utf8.endIndex),
                  let lower = String.Index(start, within: snippet), let upper = String.Index(end, within: snippet),
                  ranges.last.map({ $0.upperBound <= lower }) ?? true else { continue }
            ranges.append(lower..<upper)
        }
        return ranges
    }
}
