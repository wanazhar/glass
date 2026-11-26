import Foundation
import GlassModels
import Testing

private func decode<T: Decodable>(_ type: T.Type, _ json: String) throws -> T {
    try JSONDecoder().decode(T.self, from: Data(json.utf8))
}

@Suite struct EventDecodingTests {
    @Test func everyKnownEventTypeDecodes() throws {
        let cases: [(String, GlassEvent)] = [
            (#"{"t":"turn_start","request_id":"r1","text":"hi"}"#, .turnStart(requestID: "r1", text: "hi", alreadyRecorded: false)),
            (#"{"t":"text","delta":"He"}"#, .text(delta: "He")),
            (#"{"t":"reasoning","delta":"th"}"#, .reasoning(delta: "th")),
            (#"{"t":"tool","call_id":"c1","name":"bash","command":"ls"}"#, .tool(callID: "c1", name: "bash", command: "ls")),
            (#"{"t":"tool_end","call_id":"c1","output":"a","summary":"ok","truncated":true}"#, .toolEnd(callID: "c1", output: "a", summary: "ok", truncated: true)),
            (#"{"t":"done"}"#, .done),
            (#"{"t":"waiting"}"#, .waiting),
            (#"{"t":"queued","request_id":"r2"}"#, .queued(requestID: "r2")),
            (#"{"t":"duplicate","message":"dup"}"#, .duplicate(message: "dup")),
            (#"{"t":"steering","request_id":"r3"}"#, .steering(requestID: "r3")),
            (#"{"t":"error","message":"boom","settled":true}"#, .error(message: "boom", settled: true, provider: false, requestID: nil)),
            (#"{"t":"provider_error","message":"rate"}"#, .error(message: "rate", settled: false, provider: true, requestID: nil)),
            (#"{"t":"retrying","attempt":2}"#, .notice(message: "Provider cooldown · retry 2")),
            (#"{"t":"provider_notice","message":"slow"}"#, .notice(message: "slow")),
            (#"{"t":"disconnected","message":"gone"}"#, .disconnected(message: "gone")),
            (#"{"t":"control","can_write":true}"#, .control(canWrite: true)),
            (#"{"t":"interaction_done"}"#, .interactionDone),
            (#"{"t":"command_start","name":"compact"}"#, .commandStart(name: "compact")),
            (#"{"t":"command_done"}"#, .commandDone),
        ]
        for (json, expected) in cases { #expect(try decode(GlassEvent.self, json) == expected, "\(json)") }
        if case .interaction = try decode(GlassEvent.self, #"{"t":"interaction","id":"i"}"#) {} else { Issue.record("interaction") }
    }

    @Test func unknownTypesDecodeToUnknown() throws {
        let event = try decode(GlassEvent.self, #"{"t":"future_thing","x":1}"#)
        #expect(event == .unknown(type: "future_thing", payload: .object(["t": .string("future_thing"), "x": .number(1)])))
    }

    @Test func resetAndDeltaFramesDecode() throws {
        let reset = try decode(EventFrame.self, #"{"reset":true,"cursor":9,"active":true,"control":{"can_write":true},"interaction":null,"live":[{"t":"text","delta":"x"}],"liveClipped":false}"#)
        #expect(reset.reset && reset.active && reset.canWrite && reset.cursor == 9 && reset.live == [.text(delta: "x")])
        #expect(reset.interaction == nil)
        let delta = try decode(EventFrame.self, #"{"cursor":10,"events":[{"t":"done"}]}"#)
        #expect(!delta.reset && delta.events == [.done])
    }

    @Test func threadPageDecodes() throws {
        let page = try decode(ThreadPage.self, #"{"threads":[{"session":"a1","title":" ","workspace":"/w/glass","sourceRef":"/d/a1.jsonl","originLabel":"Created in Glass","modified":1700000000,"last_status":null}],"limit":512,"offset":0,"total_threads":600,"projects":[{"workspace":"/w/glass","count":3}],"next_offset":512}"#)
        #expect(page.threads.first?.displayTitle == "Untitled conversation")
        #expect(page.totalThreads == 600 && page.nextOffset == 512)
        #expect(page.projects?.first?.name == "glass")
        #expect(page.threads.first?.modifiedDate == Date(timeIntervalSince1970: 1_700_000_000))
    }
}

@Suite struct TimelineTests {
    static func page(_ records: [(Int, String)], before: Int) throws -> HistoryPage {
        let items = records.map { #"{"index":\#($0.0),"record":\#($0.1)}"# }.joined(separator: ",")
        return try decode(HistoryPage.self, #"{"records":[\#(items)],"before":\#(before),"totalBytes":999}"#)
    }

    @Test func historyRecordsMapToEntriesAndToolResultsMerge() throws {
        var timeline = Timeline()
        timeline.replaceHistory(try Self.page([
            (100, #"{"k":"user","t":"hello"}"#),
            (150, #"{"k":"call","id":"c1","name":"read","args":"{\"path\":\"a.txt\"}"}"#),
            (200, #"{"k":"tool","id":"c1","name":"read","t":"contents"}"#),
            (250, #"{"k":"assistant","t":"**done**"}"#),
            (300, #"{"k":"custom","c":"provider/chat-reasoning","d":"{}"}"#),
        ], before: 100))
        #expect(timeline.entries == [
            .user(id: "rec-100", text: "hello"),
            .tool(id: "c1", name: "read", summary: "a.txt", output: "contents", failed: false),
            .assistant(id: "rec-250", text: "**done**", streaming: false),
        ])
        #expect(timeline.hasOlder)
    }

    @Test func olderPagesPrependWithoutDuplicates() throws {
        var timeline = Timeline()
        timeline.replaceHistory(try Self.page([(100, #"{"k":"user","t":"b"}"#), (200, #"{"k":"assistant","t":"c"}"#)], before: 100))
        timeline.prependOlder(try Self.page([(0, #"{"k":"user","t":"a"}"#), (100, #"{"k":"user","t":"b"}"#)], before: 0))
        #expect(timeline.entries.map(\.id) == ["rec-0", "rec-100", "rec-200"])
        #expect(!timeline.hasOlder)
    }

    @Test func liveEventsStreamAndFinish() {
        var timeline = Timeline()
        timeline.apply(.turnStart(requestID: "r", text: "go", alreadyRecorded: false))
        timeline.apply(.reasoning(delta: "think"))
        timeline.apply(.text(delta: "Hel"))
        timeline.apply(.text(delta: "lo"))
        #expect(timeline.activity == "Writing")
        timeline.apply(.tool(callID: "c9", name: "bash", command: "ls"))
        timeline.apply(.toolEnd(callID: "c9", output: "a b", summary: "exit 1", truncated: false))
        timeline.apply(.done)
        #expect(timeline.entries == [
            .user(id: "live-user-1", text: "go"),
            .reasoning(id: "live-reasoning-2", text: "think"),
            .assistant(id: "live-assistant-3", text: "Hello", streaming: false),
            .tool(id: "c9", name: "bash", summary: "ls", output: "a b", failed: true),
        ])
        #expect(timeline.activity == nil && !timeline.isActive)
    }

    @Test func resetSnapshotReplacesState() throws {
        var timeline = Timeline()
        timeline.apply(.text(delta: "stale"))
        timeline.apply(.error(message: "x", settled: false, provider: false, requestID: nil))
        let frame = try decode(EventFrame.self, #"{"reset":true,"cursor":4,"active":true,"control":{"can_write":true},"live":[{"t":"text","delta":"live"}]}"#)
        timeline.reset(history: try Self.page([(0, #"{"k":"user","t":"q"}"#)], before: 0), frame: frame)
        #expect(timeline.entries.count == 2)
        #expect(timeline.entries.first == .user(id: "rec-0", text: "q"))
        #expect(timeline.notice == nil && timeline.canWrite && timeline.isActive)
    }

    @Test func liveReasoningKeepsItsIDAndSplitsAtTextToolsAndTurns() {
        var timeline = Timeline()
        timeline.apply(.turnStart(requestID: "r", text: "go", alreadyRecorded: false))
        timeline.apply(.text(delta: "Before "))
        timeline.apply(.reasoning(delta: "Check"))
        let firstID = timeline.liveReasoningID
        timeline.apply(.text(delta: ""))
        timeline.apply(.reasoning(delta: " this."))
        #expect(timeline.liveReasoningID == firstID)
        timeline.apply(.tool(callID: "c", name: "read", command: "file"))
        #expect(timeline.liveReasoningID == nil)
        timeline.apply(.reasoning(delta: "Review"))
        timeline.apply(.text(delta: "After"))
        #expect(timeline.liveReasoningID == nil)
        timeline.apply(.reasoning(delta: "Another thought"))
        timeline.apply(.turnStart(requestID: "s", text: nil, alreadyRecorded: true))
        timeline.apply(.reasoning(delta: "New turn"))
        timeline.apply(.done)
        #expect(timeline.liveReasoningID == nil)
        #expect(timeline.entries == [
            .user(id: "live-user-1", text: "go"),
            .assistant(id: "live-assistant-2", text: "Before ", streaming: false),
            .reasoning(id: "live-reasoning-3", text: "Check this."),
            .tool(id: "c", name: "read", summary: "file", output: "", failed: nil),
            .reasoning(id: "live-reasoning-4", text: "Review"),
            .assistant(id: "live-assistant-5", text: "After", streaming: false),
            .reasoning(id: "live-reasoning-6", text: "Another thought"),
            .reasoning(id: "live-reasoning-7", text: "New turn"),
        ])
    }

    @Test func historyCoalescesReasoningWithinAndAcrossPages() throws {
        var timeline = Timeline()
        timeline.replaceHistory(try Self.page([
            (200, #"{"k":"reasoning","t":"the"}"#),
            (220, #"{"k":"reasoning","t":" page."}"#),
            (250, #"{"k":"call","id":"c","name":"read"}"#),
            (270, #"{"k":"reasoning","t":"Review"}"#),
            (290, #"{"k":"reasoning","t":" it."}"#),
            (310, #"{"k":"assistant","t":"Done"}"#),
            (330, #"{"k":"reasoning","t":"Later"}"#),
        ], before: 200))
        timeline.prependOlder(try Self.page([
            (0, #"{"k":"user","t":"go"}"#),
            (150, #"{"k":"reasoning","t":"Inspect "}"#),
            (200, #"{"k":"reasoning","t":"the"}"#),
        ], before: 0))
        #expect(timeline.entries == [
            .user(id: "rec-0", text: "go"),
            .reasoning(id: "rec-200", text: "Inspect the page."),
            .tool(id: "c", name: "read", summary: "", output: "", failed: nil),
            .reasoning(id: "rec-270", text: "Review it."),
            .assistant(id: "rec-310", text: "Done", streaming: false),
            .reasoning(id: "rec-330", text: "Later"),
        ])
        #expect(timeline.liveReasoningID == nil)
    }

    @Test func controlIsUnknownUntilTheHostSaysSo() throws {
        var timeline = Timeline()
        #expect(!timeline.controlKnown && !timeline.canWrite)
        timeline.applySnapshot(try decode(EventFrame.self, #"{"reset":true,"cursor":1,"active":false,"live":[]}"#))
        #expect(!timeline.controlKnown, "a snapshot without control leaves it unknown")
        timeline.applySnapshot(try decode(EventFrame.self, #"{"reset":true,"cursor":2,"active":false,"control":{"can_write":false},"live":[]}"#))
        #expect(timeline.controlKnown && !timeline.canWrite)
        var live = Timeline()
        live.apply(.control(canWrite: true))
        #expect(live.controlKnown && live.canWrite)
    }

    @Test func activeSnapshotReusesTheSavedReasoningPrefix() throws {
        var timeline = Timeline()
        let history = try Self.page([
            (100, #"{"k":"user","t":"go"}"#),
            (200, #"{"k":"reasoning","t":"Check the"}"#),
        ], before: 0)
        let frame = try decode(EventFrame.self, #"{"reset":true,"active":true,"live":[{"t":"reasoning","delta":"Check "},{"t":"reasoning","delta":"the file"}]}"#)
        timeline.reset(history: history, frame: frame)
        timeline.apply(.reasoning(delta: "."))
        #expect(timeline.liveReasoningID == "rec-200")
        #expect(timeline.entries == [
            .user(id: "rec-100", text: "go"),
            .reasoning(id: "rec-200", text: "Check the file."),
        ])
    }

    @Test func olderWindowsWithNothingVisibleAreNotEarlierMessages() throws {
        var timeline = Timeline()
        timeline.replaceHistory(try Self.page([(100, #"{"k":"user","t":"hi"}"#), (200, #"{"k":"assistant","t":"hello"}"#)], before: 100))
        let header = try Self.page([(0, #"{"k":"custom","c":"provider/chat-reasoning","d":"{}"}"#)], before: 0)
        #expect(!timeline.hasVisibleEntries(header))
        let repeated = try Self.page([(100, #"{"k":"user","t":"hi"}"#)], before: 0)
        #expect(!timeline.hasVisibleEntries(repeated), "records already shown don't count")
        let real = try Self.page([(0, #"{"k":"user","t":"earlier"}"#)], before: 0)
        #expect(timeline.hasVisibleEntries(real))
        #expect(timeline.entries.count == 2, "probing doesn't change the timeline")
    }

    @Test func noticesFollowTheWebCopy() {
        var timeline = Timeline()
        timeline.apply(.queued(requestID: nil))
        #expect(timeline.notice?.text == "Follow-up saved on the host. It will run after the current turn.")
        timeline.apply(.error(message: "boom", settled: true, provider: true, requestID: nil))
        #expect(timeline.notice == .init(text: "boom", isError: true))
        timeline.apply(.disconnected(message: nil))
        #expect(timeline.disconnected)
    }

    @Test func originNoteOnlyFlagsTerminalConversations() throws {
        let rows = try JSONDecoder().decode([ThreadSummary].self, from: Data(#"[{"session":"a","origin":"gc"},{"session":"b","origin":"glass"},{"session":"c"}]"#.utf8))
        #expect(rows.map(\.originNote) == ["Started in Terminal", nil, nil])
    }
}
