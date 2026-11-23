import Foundation

/// The shared plane's fixed conversation id, the same string the host uses.
let everyoneConversationId = "everyone"

/// `@Name` handling for the Everyone composer and timeline. Pure value code so
/// parsing, completion, and the speaker look-up are testable on Linux.
enum MarbleMentions {
    /// The shapes the host offers for a marble's look, in its order.
    static let glyphs = ["circle", "hex", "shield", "drop", "star", "square"]

    /// The token being typed after an `@` at the end of `text`, if any.
    /// `@` only opens a mention at the start of a word, so `a@b` is plain text.
    static func token(in text: String) -> String? {
        guard let at = text.lastIndex(of: "@") else { return nil }
        if at > text.startIndex {
            let before = text[text.index(before: at)]
            guard before == " " || before == "\n" || before == "\t" else { return nil }
        }
        let query = text[text.index(after: at)...]
        guard !query.contains(" "), !query.contains("\n") else { return nil }
        return String(query)
    }

    /// Roster names completing the token being typed, case-insensitive.
    /// An `@` with no token yet offers every name; `@everyone` is offered too.
    static func completions(in text: String, names: [String]) -> [String] {
        guard token(in: text) != nil else { return [] }
        let query = token(in: text)!.lowercased()
        let candidates = names + ["everyone"]
        return candidates.filter { name in
            !name.isEmpty && name.lowercased().hasPrefix(query)
        }
    }

    /// Replace the open token with `@name `, ready to keep typing.
    static func applying(_ name: String, to text: String) -> String {
        guard token(in: text) != nil,
              let at = text.lastIndex(of: "@") else { return text }
        return String(text[..<at]) + "@\(name) "
    }

    /// Which of `names` the text actually mentions, whole-word and
    /// case-insensitive — the same rule the host applies, so the composer and
    /// the host agree about who answers.
    static func mentions(in text: String, names: [String]) -> [String] {
        let lowered = text.lowercased()
        if mentioned("everyone", in: lowered) {
            return names
        }
        return names.filter { mentioned($0.lowercased(), in: lowered) }
    }

    /// Whether `@word` appears bounded by non-word characters — `@Ada` matches
    /// but `radar` and `@adaptive` do not.
    private static func mentioned(_ word: String, in text: String) -> Bool {
        mentionPosition(word, in: text) != nil
    }

    static func responders(in text: String, roster: [MarblePrismMarble]) -> [String] {
        let text = text.lowercased()
        let mentions = roster.compactMap { marble -> (String, String.Index)? in
            let named = mentionPosition(marble.name.lowercased(), in: text)
            let alias = marble.isDefault ? mentionPosition("marble", in: text) : nil
            return [named, alias].compactMap { $0 }.min().map { (marble.id, $0) }
        }.sorted { $0.1 < $1.1 }.map(\.0)
        var ids = mentions
        if mentioned("everyone", in: text) {
            ids += roster.map(\.id).filter { !mentions.contains($0) }
        }
        return ids.isEmpty ? ["marble"] : ids
    }

    private static func mentionPosition(_ word: String, in text: String) -> String.Index? {
        guard !word.isEmpty else { return nil }
        var rest = text[...]
        let needle = "@\(word)"
        while let at = rest.range(of: needle) {
            let after = rest[at.upperBound...]
            if let next = after.unicodeScalars.first {
                if !CharacterSet.alphanumerics.contains(next) && next != "-" && next != "_" {
                    return at.lowerBound
                }
            } else {
                return at.lowerBound
            }
            rest = after
        }
        return nil
    }

    /// A hue no roster marble uses yet, so a new marble is distinguishable at
    /// a glance: the largest gap on the colour wheel, or gold on an empty one.
    static func nextHue(used: [Int]) -> Int {
        guard !used.isEmpty else { return 40 }
        let sorted = used.map { (($0 % 360) + 360) % 360 }.sorted()
        var bestHue = (sorted.last! + 60) % 360
        var bestGap = 0
        for (index, hue) in sorted.enumerated() {
            let next = index + 1 < sorted.count ? sorted[index + 1] : sorted[0] + 360
            let gap = next - hue
            if gap > bestGap {
                bestGap = gap
                bestHue = (hue + gap / 2) % 360
            }
        }
        return bestHue
    }
}

/// One plain sentence for a handoff row, as it reads in chat.
enum MarbleHandoffLine {
    static func render(_ handoff: MarbleHandoff) -> String {
        switch handoff.status {
        case "asked":
            "\(handoff.fromName) asked \(handoff.toName) to \(handoff.instruction)."
        case "finished":
            "\(handoff.toName) finished: \(handoff.summary)"
        case "refused":
            "\(handoff.toName) couldn't take it: \(handoff.summary)"
        default:
            "\(handoff.toName) couldn't finish: \(handoff.summary)"
        }
    }

    /// The quiet row's two parts: who did what, then the instruction or
    /// the reply it brought back.
    static func title(_ handoff: MarbleHandoff) -> String {
        switch handoff.status {
        case "asked": "\(handoff.fromName) asked \(handoff.toName)"
        case "finished": "\(handoff.toName) replied to \(handoff.fromName)"
        case "refused": "\(handoff.toName) couldn't take it"
        default: "\(handoff.toName) couldn't finish"
        }
    }

    static func detail(_ handoff: MarbleHandoff) -> String {
        handoff.status == "asked" ? handoff.instruction : handoff.summary
    }

    /// A finished ask expands to the whole reply it brought back.
    static func isExpandable(_ handoff: MarbleHandoff) -> Bool {
        handoff.status == "finished" && !handoff.summary.isEmpty
    }
}

/// Who said a line on the shared plane, for its label and colour dot.
struct MarbleSpeakerStyle: Equatable, Sendable {
    var name: String
    var hue: Int?
    /// The marble's avatar shape, when the roster knows it.
    var glyph: String = "circle"
    /// The default assistant's dot, matching its roster hue.
    static let defaultHue = 210

    init(name: String, hue: Int?, glyph: String = "circle") {
        self.name = name
        self.hue = hue
        self.glyph = glyph
    }

    /// Resolve a transcript row's speaker against the roster.
    static func `for`(
        entry: MarbleEntry,
        marbles: [MarblePrismMarble]
    ) -> MarbleSpeakerStyle? {
        guard ["assistant", "pending", "handoff", "failure", "tool"].contains(entry.kind),
              !entry.speaker.isEmpty || !entry.speakerName.isEmpty
        else { return nil }
        if let marble = marbles.first(where: { $0.id == entry.speaker || $0.name == entry.speakerName }) {
            return MarbleSpeakerStyle(
                name: entry.speakerName.isEmpty ? marble.name : entry.speakerName,
                hue: marble.hue,
                glyph: marble.glyph
            )
        }
        return MarbleSpeakerStyle(
            name: entry.speakerName.isEmpty ? entry.speaker : entry.speakerName,
            hue: entry.speaker == "marble" ? MarbleSpeakerStyle.defaultHue : nil
        )
    }
}

/// How a host `everyone` event changes a fetched Everyone timeline.
enum MarbleEveryoneTimeline {
    static func settling(_ event: MarbleEvent, reserved: MarbleEntry?, at: UInt64) -> MarbleEntry? {
        guard let id = event.data.entryId, let kind = event.data.entryKind,
              ["assistant", "failure"].contains(kind), let text = event.data.text else { return nil }
        var entry = reserved ?? MarbleEntry(
            conversationId: event.conversationId, kind: kind, text: text, taskId: event.taskId, at: at
        )
        entry.serverID = id
        entry.threadId = event.data.threadId ?? entry.threadId
        entry.kind = kind
        entry.text = text
        entry.taskId = event.taskId
        if let sender = event.data.sender, !sender.isEmpty { entry.speakerName = sender }
        entry.failure = event.data.failure
        entry.cancelled = event.data.cancelled
        return entry
    }

    static func applying(_ updates: [MarbleEntry], to entries: [MarbleEntry]) -> [MarbleEntry] {
        var rows = entries
        for update in updates {
            if let index = rows.firstIndex(where: { $0.id == update.id }) {
                if update.kind != "pending" || rows[index].kind == "pending" {
                    rows[index] = update
                }
            } else {
                let index = update.kind == "user" ? rows.firstIndex {
                    $0.kind == "pending" && $0.at >= update.at
                } : nil
                rows.insert(update, at: index ?? rows.endIndex)
            }
        }
        return rows
    }

    /// The state and transcript are fetched separately. Keep a bubble through
    /// that transition, then let the visible outcome replace it.
    static func showsThinking(
        working: Bool, activeTaskIds: [String], previousTaskIds: [String], visibleOutcomes: Set<String>
    ) -> Bool {
        if working && activeTaskIds.isEmpty { return true }
        let pending = working ? activeTaskIds : previousTaskIds
        return pending.contains { !visibleOutcomes.contains($0) }
    }

    /// Whether the event says the shared timeline changed, so a fetched copy
    /// is stale and should be fetched again.
    static func isChange(_ event: MarbleEvent) -> Bool {
        event.kind == "everyone"
    }

    /// A clear empties the timeline at once; any other change keeps it until
    /// the refetch replaces it.
    static func applying(_ event: MarbleEvent, to timeline: [MarbleEntry]) -> [MarbleEntry] {
        isChange(event) && event.data.cleared != nil ? [] : timeline
    }
}
