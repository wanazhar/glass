import Foundation
import XCTest
@testable import MarbleCore

final class MarbleTurnPresentationTests: XCTestCase {
    func testRevealPreservesOriginalValidatedTextAndBoundsFrameCount() {
        let text = "  A 👩🏽‍💻 reply.\n\n- Next item"
        XCTAssertEqual(MarbleReplyReveal.frames(for: text), [
            "  A ", "  A 👩🏽‍💻 ", "  A 👩🏽‍💻 reply.\n\n",
            "  A 👩🏽‍💻 reply.\n\n- ", "  A 👩🏽‍💻 reply.\n\n- Next ", text,
        ])
        let long = String(repeating: "word ", count: 131)
        let frames = MarbleReplyReveal.frames(for: long)
        XCTAssertLessThanOrEqual(frames.count, MarbleReplyReveal.maximumFrames)
        XCTAssertEqual(frames.last, long)
        XCTAssertEqual(MarbleReplyReveal.frames(for: "Okay."), ["Okay."])
    }

    func testStoppedPartialReplyKeepsItsTextAndOffersRetry() throws {
        let task = MarbleTask(id: "turn", conversationId: "chat", status: "cancelled",
                              owner: "user", prompt: "Do the work", at: 1)
        let entry = MarbleEntry(conversationId: "chat", kind: "assistant", text: "Partial",
                                taskId: task.id, at: 2)
        XCTAssertTrue(MarbleTurnFailure.isStoppedPartialReply(entry, taskStatus: task.status))
        XCTAssertEqual(MarbleTurnFailure.retryID(entry: entry, task: task), task.id)
        XCTAssertFalse(MarbleTurnFailure.isStoppedPartialReply(entry, taskStatus: "completed"))

        let payload = #"{"sequence":2,"conversationId":"chat","taskId":"turn","kind":"reply","data":{"id":"partial","kind":"assistant","text":"Partial","cancelled":true}}"#
        let event = try JSONDecoder().decode(MarbleEvent.self, from: Data(payload.utf8))
        let saved = try XCTUnwrap(MarbleEveryoneTimeline.settling(event, reserved: nil, at: 2))
        let restored = try JSONDecoder().decode(MarbleEntry.self, from: JSONEncoder().encode(saved))
        XCTAssertEqual(restored.text, "Partial")
        XCTAssertTrue(MarbleTurnFailure.isStoppedPartialReply(restored, taskStatus: nil))
        XCTAssertEqual(MarbleTurnFailure.retryID(entry: restored, task: nil), task.id)
    }

    func testCompletedToolFromBeforePageIsNotAppendedBelowNewestReply() {
        func tool(_ id: String, sequence: UInt64) -> MarbleToolActivity {
            var step = MarbleToolActivity(id: id, name: "bash", status: .completed)
            step.entryIDs["chat"] = id
            step.sequence = sequence
            return step
        }
        let reply = MarbleEntry(conversationId: "chat", kind: "assistant", text: "Latest answer",
                               taskId: "visible-turn", at: 200)
        let updated = MarbleToolActivity.insertingLiveRows(
            ["old-turn": [tool("old-tool", sequence: 1)],
             "visible-turn": [tool("visible-tool", sequence: 2)],
             "active-turn": [tool("active-tool", sequence: 3)]],
            into: [reply], activeTasks: ["active-turn"], conversationId: "chat", threadId: nil
        )
        XCTAssertEqual(updated.map(\.id), ["visible-tool", reply.id, "active-tool"])
    }

    func testNarrationRecognizesTheVisibleRosterChatWithoutSpeakingOtherChats() {
        XCTAssertTrue(MarbleReplyNarration.isVisible("dietrich", selected: nil, rosterChat: "dietrich"))
        XCTAssertFalse(MarbleReplyNarration.isVisible("ada", selected: nil, rosterChat: "dietrich"))
        XCTAssertTrue(MarbleReplyNarration.isVisible("private", selected: "private", rosterChat: "dietrich"))
        XCTAssertFalse(MarbleReplyNarration.isVisible("dietrich", selected: "private", rosterChat: "dietrich"))
        XCTAssertFalse(MarbleReplyNarration.isVisible("dietrich", selected: nil, rosterChat: nil))
    }

    func testNarrationUsesFinalReplyAndResetInvalidatesQueuedSpeechByTask() throws {
        var narration = MarbleReplyNarration()
        var entry = MarbleEntry(conversationId: "chat", kind: "assistant", text: "Final answer.",
                                taskId: "one", at: 2)
        XCTAssertNil(narration.finalReply(entry))
        narration.awaitFinal(taskId: "one", conversationId: "chat")
        narration.awaitFinal(taskId: "two", conversationId: "other")
        entry.kind = "reasoning"
        entry.text = "Planning."
        XCTAssertNil(narration.finalReply(entry))
        entry.kind = "assistant"
        entry.text = "Final answer."
        let first = try XCTUnwrap(narration.finalReply(entry))
        XCTAssertNil(narration.finalReply(entry), "SSE and the transcript can report the same reply")

        narration.cancel(taskId: "one")
        XCTAssertFalse(narration.isQueued(first))
        XCTAssertFalse(narration.take(first))
        narration.awaitFinal(taskId: "one", conversationId: "chat")
        entry.text = "Replacement answer."
        let replacement = try XCTUnwrap(narration.finalReply(entry))
        XCTAssertFalse(narration.take(first), "A delayed speech request cannot consume its replacement")
        XCTAssertEqual(replacement.text, "Replacement answer.")
        XCTAssertTrue(narration.take(replacement))

        entry.taskId = "two"
        entry.conversationId = "other"
        let other = try XCTUnwrap(narration.finalReply(entry))
        XCTAssertTrue(narration.take(other))
    }

    func testTextResetClearsOnlyMatchingTaskAndKeepsItsRow() throws {
        func event(_ kind: String, task: String, text: String) throws -> MarbleEvent {
            let json = """
            {"sequence":1,"conversationId":"private-chat","taskId":"\(task)","kind":"\(kind)","data":{"text":"\(text)"}}
            """
            return try JSONDecoder().decode(MarbleEvent.self, from: Data(json.utf8))
        }
        var row = MarbleEntry(conversationId: "everyone", kind: "pending", text: "Planning.",
                              taskId: "ada-turn", at: 42, speaker: "ada", speakerName: "Ada")
        row.serverID = "reserved-row"
        let original = row
        let otherReset = try event("text_reset", task: "marble-turn", text: "")
        XCTAssertNil(MarbleStreamingText.updating(row.text, for: row.taskId, from: otherReset))
        let reset = try event("text_reset", task: row.taskId, text: "")
        row.text = try XCTUnwrap(MarbleStreamingText.updating(row.text, for: row.taskId, from: reset))
        XCTAssertEqual(row.text, "")
        let reasoning = try event("reasoning", task: row.taskId, text: "Planning.")
        XCTAssertNil(MarbleStreamingText.updating(row.text, for: row.taskId, from: reasoning))
        for delta in ["Public ", "answer."] {
            let update = try event("text", task: row.taskId, text: delta)
            row.text = try XCTUnwrap(MarbleStreamingText.updating(row.text, for: row.taskId, from: update))
        }
        XCTAssertEqual(row.text, "Public answer.")
        XCTAssertEqual(row.rowID, original.rowID)
        row.text = original.text
        XCTAssertEqual(row, original)
    }

    func testGroupReservationRekeysAndSettlesInItsOriginalSlot() throws {
        func event(_ kind: String, task: String, data: String) throws -> MarbleEvent {
            let json = """
            {"sequence":1,"conversationId":"everyone","taskId":"\(task)","kind":"\(kind)","data":\(data)}
            """
            return try JSONDecoder().decode(MarbleEvent.self, from: Data(json.utf8))
        }
        let initiallyReserved = try event("entry", task: "queued-ada", data: """
            {"id":"ada-row","kind":"pending","conversationId":"everyone","text":"","at":32,
             "taskId":"queued-ada","speaker":"ada","speakerName":"Ada","thread_id":"thread"}
            """)
        let rekeyed = try event("entry", task: "turn-ada", data: """
            {"id":"ada-row","kind":"pending","conversationId":"everyone","text":"","at":32,
             "taskId":"turn-ada","speaker":"ada","speakerName":"Ada","thread_id":"thread"}
            """)
        let reservation = try XCTUnwrap(initiallyReserved.entryUpdate)
        let update = try XCTUnwrap(rekeyed.entryUpdate)
        var other = reservation
        other.serverID = "ben-row"
        let rows = MarbleEveryoneTimeline.applying([update], to: [reservation, other])
        XCTAssertEqual(rows.map(\.rowID), ["ada-row", "ben-row"])
        XCTAssertEqual(rows.first?.taskId, "turn-ada")
        let failure = try event("reply", task: "turn-ada", data: """
            {"id":"ada-row","kind":"failure","text":"Couldn’t finish the reply.","sender":"Ada",
             "failure":{"category":"transient","retryable":true,"diagnostics":"Timed out"}}
            """)
        let settled = try XCTUnwrap(MarbleEveryoneTimeline.settling(failure, reserved: rows.first, at: 999))
        let finished = MarbleEveryoneTimeline.applying([settled], to: rows)
        XCTAssertEqual(finished.map(\.rowID), rows.map(\.rowID))
        XCTAssertEqual(finished.first?.at, 32)
        XCTAssertEqual(finished.first?.threadId, "thread")
        XCTAssertEqual(finished.first?.speaker, "ada")
        XCTAssertEqual(MarbleTurnFailure.retryID(entry: settled, task: nil), "turn-ada")
        XCTAssertEqual(MarbleEveryoneTimeline.applying([update], to: finished), finished)
    }

    private func toolEvent(_ kind: String, data: String) throws -> MarbleEvent {
        let json = """
        {"sequence":1,"conversationId":"chat","taskId":"turn","kind":"\(kind)","data":\(data)}
        """
        return try JSONDecoder().decode(MarbleEvent.self, from: Data(json.utf8))
    }

    func testTwoIdenticalToolsFinishByRequestIDAndKeepTheirQuery() throws {
        var calls: [MarbleToolActivity] = []
        for (id, query) in [("one", "other"), ("two", "riemann hypothesis")] {
            let start = try toolEvent("tool_start", data: """
                {"requestId":"\(id)","tool":"web_search","arguments":{"query":"\(query)"}}
                """)
            MarbleToolActivity.apply(start, to: &calls)
            MarbleToolActivity.apply(start, to: &calls)
        }
        let end = try toolEvent("tool_end", data: #"{"requestId":"two","tool":"web_search","isError":false}"#)
        MarbleToolActivity.apply(end, to: &calls)
        XCTAssertEqual(calls.map(\.status), [.running, .completed])
        XCTAssertEqual(calls.last?.title, "Searched the web")
        XCTAssertEqual(calls.last?.query, "riemann hypothesis")
    }

    func testFailedToolDoesNotUseRawProviderErrorAsSummary() throws {
        let end = try toolEvent("tool_end", data: #"{"tool":"web_search","isError":true,"text":"raw provider error"}"#)
        var calls: [MarbleToolActivity] = []
        MarbleToolActivity.apply(end, to: &calls)
        XCTAssertEqual(calls.first?.status, .failed)
        XCTAssertNil(calls.first?.query)
    }

    func testToolDetailsStayWithTheReplyAndTheLabelDoesNotChange() throws {
        var calls: [MarbleToolActivity] = []
        let start = try toolEvent("tool_start", data: #"{"callId":"shell","name":"bash","title":"printf hello"}"#)
        MarbleToolActivity.apply(start, to: &calls)
        let label = calls.first?.title
        let end = try toolEvent("tool_end", data: #"{"callId":"shell","text":"hello","isError":true}"#)
        MarbleToolActivity.apply(end, to: &calls)
        let tool = try XCTUnwrap(calls.first)
        XCTAssertEqual(label, "Ran printf")
        XCTAssertEqual(tool.title, label)
        XCTAssertEqual(tool.details?.command, "printf hello")
        XCTAssertEqual(tool.details?.output, "hello")
        XCTAssertEqual(tool.status, .failed)
        var row = MarbleEntry(conversationId: "chat", kind: "tool", text: tool.transcriptText, taskId: "turn", at: 1)
        row.toolDetails = tool.details
        let reply = MarbleEntry(conversationId: "chat", kind: "assistant", text: "Reply", taskId: "turn", at: 2)
        let later = MarbleEntry(conversationId: "chat", kind: "user", text: "Next", taskId: "next", at: 3)
        let running = MarbleReasoningAssociation([row])
        let timeline = MarbleReasoningAssociation([row, reply, later])
        XCTAssertEqual(timeline.standalone, [row, reply, later])
        XCTAssertEqual(timeline.tools, running.tools)
        XCTAssertEqual(timeline.tools[row.id], [row])
        XCTAssertEqual(MarbleToolActivity(id: "list", name: "bash", query: "ls -l appendix.pdf",
                                          status: .running).title, "Listed appendix.pdf")
    }

    func testTimeLabelsSeparateDaysAndGapsOverFiveMinutes() {
        let base = UInt64(Calendar.current.startOfDay(for: Date()).timeIntervalSince1970 * 1000)
        let rows = [1_000, 301_000, 602_000, 172_800_000].map { offset in
            MarbleEntry(conversationId: "chat", kind: "user", text: String(offset),
                        taskId: String(offset), at: base + UInt64(offset))
        }
        let labels = MarbleReasoningAssociation(rows).timestamps
        XCTAssertNotNil(labels[rows[0].id])
        XCTAssertNil(labels[rows[1].id])
        XCTAssertNotNil(labels[rows[2].id])
        XCTAssertNotNil(labels[rows[3].id])
    }

    func testNamelessEndUpdatesMatchingCallAndKeepsEveryoneAttribution() throws {
        var calls: [MarbleToolActivity] = []
        let start = try toolEvent("tool_start", data: #"{"callId":"call-7","tool":"bash","sender":"Ada","entry_id":"tool-7"}"#)
        let end = try toolEvent("tool_end", data: #"{"callId":"call-7","requestId":"approval","approved":false,"isError":true}"#)
        MarbleToolActivity.apply(start, to: &calls)
        MarbleToolActivity.apply(end, to: &calls)
        XCTAssertEqual(calls.count, 1)
        XCTAssertEqual(calls.first?.name, "bash")
        XCTAssertEqual(calls.first?.sender, "Ada")
        XCTAssertEqual(calls.first?.entryIDs["chat"], "tool-7")
        XCTAssertEqual(calls.first?.status, .denied)
    }

    func testSettledFinalizesOnlyRunningCalls() throws {
        var calls: [MarbleToolActivity] = []
        for id in ["one", "two"] {
            let start = try toolEvent("tool_start", data: #"{"callId":"\#(id)","tool":"bash"}"#)
            MarbleToolActivity.apply(start, to: &calls)
        }
        MarbleToolActivity.apply(try toolEvent("tool_end", data: #"{"callId":"two","isError":true}"#), to: &calls)
        MarbleToolActivity.finishRunning(&calls)
        XCTAssertEqual(calls.map(\.status), [.finished, .failed])
        MarbleToolActivity.apply(try toolEvent("tool_end", data: #"{"callId":"one","isError":false}"#), to: &calls)
        XCTAssertEqual(calls.map(\.status), [.completed, .failed])
    }

    func testFailurePresentationLeavesNormalAnswersAlone() {
        let failure = MarbleEntry.preview(kind: "assistant", text: "[Generation failed]\nReason: provider error")
        XCTAssertTrue(MarbleTurnFailure.representsFailure(failure, status: nil))
        let quoted = MarbleEntry.preview(kind: "user", text: "[Generation failed] I saw this yesterday")
        XCTAssertFalse(MarbleTurnFailure.representsFailure(quoted, status: nil))
        let structured = MarbleEntry.preview(kind: "system", text: "raw provider error")
        XCTAssertTrue(MarbleTurnFailure.representsFailure(structured, status: "failed"))
        XCTAssertNil(MarbleTurnFailure.message(for: "completed"))
    }

    func testDurableToolRowUsesSamePlainTitleAndStatusAsLiveTool() {
        let entry = MarbleEntry.preview(kind: "tool", text: "web_search · riemann · hypothesis · ok")
        let tool = MarbleToolActivity.fromTranscript(entry)
        XCTAssertEqual(tool?.title, "Searched the web")
        XCTAssertEqual(tool?.status, .completed)
        XCTAssertEqual(tool?.query, "riemann · hypothesis")
    }

    func testStructuredFailureUsesTheHostRetryFlagWithoutATaskSnapshot() throws {
        let json = """
        {"id":"entry","kind":"failure","conversationId":"everyone","at":32,
         "taskId":"failed-agent-turn","text":"Couldn't get a reply from the model. Try again.",
         "failure":{"category":"provider_rejected","retryable":true,"diagnostics":"Sanitized details"}}
        """
        var failure = try JSONDecoder().decode(MarbleEntry.self, from: Data(json.utf8))
        XCTAssertTrue(MarbleTurnFailure.representsFailure(failure, status: nil))
        XCTAssertEqual(failure.failure?.diagnostics, "Sanitized details")
        XCTAssertEqual(MarbleTurnFailure.retryID(entry: failure, task: nil), "failed-agent-turn")
        failure.failure = MarbleFailureInfo(category: "unknown", retryable: false, diagnostics: "")
        let task = MarbleTask(id: "failed-agent-turn", conversationId: "everyone", status: "failed",
                              owner: "agent", prompt: "retryable on old hosts", at: 31)
        XCTAssertNil(MarbleTurnFailure.retryID(entry: failure, task: task))
    }

    func testReasoningAttachesToItsOwnReplyWhenTurnsInterleave() {
        let a = MarbleEntry(conversationId: "everyone", kind: "reasoning", text: "Ada thought", taskId: "a", at: 1)
        let b = MarbleEntry(conversationId: "everyone", kind: "reasoning", text: "Ben thought", taskId: "b", at: 2)
        let reply = MarbleEntry(conversationId: "everyone", kind: "assistant", text: "Ben replied", taskId: "b", at: 3)
        let failure = MarbleEntry(conversationId: "everyone", kind: "failure", text: "Ada failed", taskId: "a", at: 4)
        let rows = MarbleReasoningAssociation([a, b, reply, failure])
        XCTAssertEqual(rows.standalone, [reply, failure])
        XCTAssertEqual(rows.attached, [reply.id: "Ben thought", failure.id: "Ada thought"])
    }

    func testPersistedEveryoneReasoningSharesTheLiveReplySlotAcrossRefreshAndRelaunch() throws {
        func row(_ id: String, kind: String, text: String, agent: String) -> MarbleEntry {
            var entry = MarbleEntry(conversationId: "everyone", kind: kind, text: text,
                                    taskId: "\(agent)-turn", at: 1, speakerName: agent)
            entry.serverID = id
            return entry
        }
        let user = row("group-message", kind: "user", text: "@everyone", agent: "Member")
        let marble = row("marble-reply", kind: "pending", text: "", agent: "Marble")
        let dietrich = row("dietrich-reply", kind: "pending", text: "", agent: "Dietrich")
        let older = row("older-reply", kind: "assistant", text: "Earlier", agent: "Ada")
        let marbleReasoning = row("private-marble-reasoning-everyone", kind: "reasoning",
                                  text: "Marble's plan", agent: "Marble")
        let dietrichReasoning = row("private-dietrich-reasoning-everyone", kind: "reasoning",
                                    text: "Dietrich's plan", agent: "Dietrich")
        var marbleReply = marble
        marbleReply.kind = "assistant"
        marbleReply.text = "Marble's answer"
        var dietrichReply = dietrich
        dietrichReply.kind = "assistant"
        dietrichReply.text = "Dietrich's answer"
        let current = [older, user, marble, dietrich]
        let live = [marbleReply, dietrichReply]
        let page = [user, marbleReasoning, marbleReply, dietrichReasoning, dietrichReply]
        let (refreshed, _) = MarbleHistoryWindow.refreshing(
            current, before: older.id, with: page, nextBefore: user.id
        )
        let withLiveUpdates = MarbleEveryoneTimeline.applying(live, to: refreshed)
        XCTAssertEqual(withLiveUpdates.map(\.id), ([older] + page).map(\.id))
        let relaunched = try JSONDecoder().decode([MarbleEntry].self, from: JSONEncoder().encode(withLiveUpdates))
        let transcript = MarbleReasoningAssociation(relaunched)
        XCTAssertEqual(transcript.standalone, [older, user, marbleReply, dietrichReply])
        XCTAssertEqual(transcript.attached, [
            marble.id: marbleReasoning.text,
            dietrich.id: dietrichReasoning.text,
        ])
    }
}
