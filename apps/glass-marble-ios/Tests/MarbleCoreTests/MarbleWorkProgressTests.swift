import Foundation
import XCTest
@testable import MarbleCore

final class MarbleWorkProgressTests: XCTestCase {
    func testOverlappingToolsAndReplyingAreScopedToOneTurn() throws {
        var work = MarbleWorkProgress()
        XCTAssertTrue(work.apply(try event("tool_start", data: ["callId": "read"])))
        XCTAssertFalse(work.apply(try event("tool_start", data: ["callId": "list"])))
        XCTAssertFalse(work.apply(try event("tool_end", data: ["callId": "read"])))
        XCTAssertEqual(work.phase(for: "turn"), .tool)
        XCTAssertEqual(work.phase(for: "other"), .thinking)
        XCTAssertTrue(work.apply(try event("tool_end", data: ["callId": "list"])))
        XCTAssertTrue(work.apply(try event("replying")))
        XCTAssertFalse(work.apply(try event("text")))
        XCTAssertEqual(work.phase(for: "turn"), .replying)
    }

    func testCancellationAndSettlementIgnoreLateRuntimeUpdates() throws {
        var work = MarbleWorkProgress()
        XCTAssertTrue(work.apply(try event("cancel_requested", data: ["taskId": "turn"])))
        XCTAssertFalse(work.apply(try event("replying")))
        XCTAssertEqual(work.phase(for: "turn"), .stopping)
        XCTAssertFalse(work.apply(try event("settled", data: ["reason": "completed"])))
        XCTAssertFalse(work.isFinished("turn"))
        XCTAssertTrue(work.apply(try event("settled", data: ["cancelled": true])))
        XCTAssertTrue(work.isFinished("turn"))
        XCTAssertFalse(work.apply(try event("text")))
        XCTAssertTrue(work.apply(try event("task_retry")))
        XCTAssertFalse(work.isFinished("turn"))
        XCTAssertEqual(work.phase(for: "turn"), .thinking)
    }

    private func event(_ kind: String, data: [String: Any] = [:]) throws -> MarbleEvent {
        let payload = try JSONSerialization.data(withJSONObject: [
            "sequence": 1, "taskId": "turn", "conversationId": "chat", "kind": kind, "data": data
        ])
        return try JSONDecoder().decode(MarbleEvent.self, from: payload)
    }
}
