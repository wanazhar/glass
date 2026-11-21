import GlassModels
import Testing

@Suite struct MessageActionTests {
    let location = ActivityLocation(recordOffset: 900, record: 7, turn: 3, revision: "abc")

    @Test func quoteUsesRecordOrdinal() {
        let command = MessageAction.quote.command(at: location)
        #expect(command.name == "quote")
        #expect(command.values["args"] == .string("7"))
        #expect(command.values["source_expected"] == .string("abc"))
        #expect(command.values["before"] == .bool(false))
    }

    @Test func editBranchesBeforeTheTurn() {
        let command = MessageAction.editInBranch.command(at: location)
        #expect(command.name == "branch")
        #expect(command.values["args"] == .string("2"))
        #expect(command.values["before"] == .bool(true))
        #expect(MessageAction.branch.command(at: location).values["args"] == .string("3"))
    }

    @Test func recordOffsetOnlyForSavedEntries() {
        #expect(TimelineEntry.user(id: "rec-42", text: "hi").recordOffset == 42)
        #expect(TimelineEntry.user(id: "live-user-1", text: "hi").recordOffset == nil)
    }

    @Test func readableSplitsOnlyLongProse() {
        let sentence = "Then " + String(repeating: "word ", count: 70) + "end."
        let long = (0..<4).map { _ in sentence }.joined(separator: " ")
        let readable = ReasoningText.readable(long)
        #expect(readable.contains("end.\n\nThen"))
        #expect(ReasoningText.readable("short. Text.") == "short. Text.")
        let list = "- " + long
        #expect(ReasoningText.readable(list) == list)
    }
}
