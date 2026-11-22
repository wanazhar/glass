import Foundation

/// Where the event stream picks up again. The host replays buffered events
/// after `since`; its counter starts over when the host restarts, so a saved
/// cursor ahead of the host's own sequence would hide every new event.
enum MarbleResumeCursor {
    struct Resume: Equatable {
        let since: UInt64
        let restarted: Bool
    }

    static func resume(saved: UInt64, hostSequence: UInt64, previousPID: Int?, hostPID: Int?) -> Resume {
        // A saved position from an older app may have no process ID.
        // Replay once so an already advanced replacement host can't hide replies.
        let changedProcess = hostPID != nil && previousPID != hostPID && (previousPID != nil || saved != 0)
        let restarted = changedProcess || hostSequence < saved
        return Resume(since: restarted ? 0 : saved, restarted: restarted)
    }
}

/// The last fetched Everyone timeline and marble chats, kept on this iPhone
/// so a cold launch shows messages before the host answers.
struct MarbleChatCache: Codable, Equatable {
    var everyone: [MarbleEntry]
    var transcripts: [String: [MarbleEntry]]
    var before: [String: String]? = nil
    var threads: [String: MarbleThreadPage]? = nil

    static let empty = MarbleChatCache(everyone: [], transcripts: [:])

    mutating func removeConversation(_ id: String) {
        transcripts.removeValue(forKey: id)
        before?.removeValue(forKey: id)
        threads = threads?.filter { $0.value.thread.conversationId != id }
        everyone.removeAll { $0.conversationId == id }
    }

    /// One file name per host, so switching hosts never shows another
    /// host's messages: the SHA-256 of the normalized host URL, which no two
    /// different hosts share.
    static func fileName(for host: String) -> String {
        let digest = MarbleSHA256.hash(Array(normalized(host).utf8))
        let hex = digest.map { String(format: "%02x", $0) }.joined()
        return "marble-chats-\(hex).json"
    }

    /// Scheme and host are case-insensitive and a trailing slash adds
    /// nothing, so they do not make a different host.
    static func normalized(_ host: String) -> String {
        var value = host.trimmingCharacters(in: .whitespacesAndNewlines)
        while value.hasSuffix("/") { value.removeLast() }
        guard var components = URLComponents(string: value) else { return value }
        components.scheme = components.scheme?.lowercased()
        components.host = components.host?.lowercased()
        return components.string ?? value
    }

    func encoded() throws -> Data {
        try JSONEncoder().encode(self)
    }

    static func decoded(_ data: Data) -> MarbleChatCache? {
        try? JSONDecoder().decode(MarbleChatCache.self, from: data)
    }
}

enum MarbleHistoryWindow {
    static let pageSize = 50

    /// Keep previously loaded rows only when they join the latest page.
    /// The newest page owns overlapping rows, including in-place settlements.
    static func refreshing(_ entries: [MarbleEntry], before: String?,
                           with latest: [MarbleEntry], nextBefore: String?) -> ([MarbleEntry], String?) {
        guard nextBefore != nil, let first = latest.first,
              let overlap = entries.firstIndex(where: { $0.id == first.id }) else {
            return (latest, nextBefore)
        }
        return (Array(entries[..<overlap]) + latest, overlap == 0 ? nextBefore : before)
    }

    static func prepending(_ older: [MarbleEntry], to newer: [MarbleEntry]) -> [MarbleEntry] {
        let ids = Set(newer.map(\.id))
        return older.filter { !ids.contains($0.id) } + newer
    }

    static func cached(_ entries: [MarbleEntry], before: String?) -> ([MarbleEntry], String?) {
        guard entries.count > pageSize else { return (entries, before) }
        let latest = Array(entries.suffix(pageSize))
        return (latest, latest.first?.serverID)
    }
}
