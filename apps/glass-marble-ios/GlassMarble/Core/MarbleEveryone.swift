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
        var rest = text[...]
        let needle = "@\(word)"
        while let at = rest.range(of: needle) {
            let after = rest[at.upperBound...]
            guard let next = after.first else { return true }
            if !next.isLetter && !next.isNumber && next != "-" && next != "_" {
                return true
            }
            rest = after
        }
        return false
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

    /// A finished ask expands to the whole reply it brought back.
    static func isExpandable(_ handoff: MarbleHandoff) -> Bool {
        handoff.status == "finished" && !handoff.summary.isEmpty
    }
}

/// Who said a line on the shared plane, for its label and colour dot.
struct MarbleSpeakerStyle: Equatable, Sendable {
    var name: String
    var hue: Int?
    /// The default assistant's dot, matching its roster hue.
    static let defaultHue = 210

    init(name: String, hue: Int?) {
        self.name = name
        self.hue = hue
    }

    /// Resolve a transcript row's speaker against the roster.
    static func `for`(
        entry: MarbleEntry,
        marbles: [MarblePrismMarble]
    ) -> MarbleSpeakerStyle? {
        guard entry.kind == "assistant" || entry.kind == "handoff",
              !entry.speaker.isEmpty
        else { return nil }
        if let marble = marbles.first(where: { $0.id == entry.speaker }) {
            return MarbleSpeakerStyle(
                name: entry.speakerName.isEmpty ? marble.name : entry.speakerName,
                hue: marble.hue
            )
        }
        return MarbleSpeakerStyle(
            name: entry.speakerName.isEmpty ? entry.speaker : entry.speakerName,
            hue: entry.speaker == "marble" ? MarbleSpeakerStyle.defaultHue : nil
        )
    }
}
