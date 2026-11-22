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
    /// host's messages.
    static func fileName(for host: String) -> String {
        let safe = host.unicodeScalars.map { scalar -> String in
            CharacterSet.alphanumerics.contains(scalar) ? String(scalar) : "_"
        }.joined()
        return "marble-chats-\(safe).json"
    }

    func encoded() throws -> Data {
        try JSONEncoder().encode(self)
    }

    static func decoded(_ data: Data) -> MarbleChatCache? {
        try? JSONDecoder().decode(MarbleChatCache.self, from: data)
    }
}
