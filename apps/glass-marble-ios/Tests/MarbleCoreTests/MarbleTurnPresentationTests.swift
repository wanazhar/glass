import Foundation
import XCTest
@testable import MarbleCore

final class MarbleTurnPresentationTests: XCTestCase {
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
}
