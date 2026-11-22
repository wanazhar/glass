import Foundation
import XCTest
@testable import MarbleCore

final class MarbleTurnPresentationTests: XCTestCase {
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
}
