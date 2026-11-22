import Foundation

/// Where the event stream picks up again. The host replays buffered events
/// after `since`; its counter starts over when the host restarts, so a saved
/// cursor ahead of the host's own sequence would hide every new event.
enum MarbleResumeCursor {
    static func resume(saved: UInt64, hostSequence: UInt64) -> UInt64 {
        saved > hostSequence ? hostSequence : saved
    }
}

/// The last fetched Everyone timeline and marble chats, kept on this iPhone
/// so a cold launch shows messages before the host answers.
struct MarbleChatCache: Codable, Equatable {
    var everyone: [MarbleEntry]
    var transcripts: [String: [MarbleEntry]]

    static let empty = MarbleChatCache(everyone: [], transcripts: [:])

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
