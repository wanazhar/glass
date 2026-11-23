import Foundation
import XCTest
@testable import MarbleCore

final class MarbleThreadsTests: XCTestCase {
    func testStableMessageIDAndParentSummaryRoundTripThroughCache() throws {
        let json = """
        {"id":"entry-1","conversationId":"everyone","kind":"user","text":"hello","at":20,
         "thread":{"id":"thread-1","reply_count":2,"last_activity_at":30}}
        """
        let entry = try JSONDecoder().decode(MarbleEntry.self, from: Data(json.utf8))
        XCTAssertEqual(entry.id, "entry-1")
        XCTAssertNil(entry.threadId)
        XCTAssertEqual(entry.thread?.replyLabel, "2 replies")
        let restored = try JSONDecoder().decode(MarbleEntry.self, from: JSONEncoder().encode(entry))
        XCTAssertEqual(restored, entry)
    }

    func testThreadEventAndSnakeCaseMembershipDecode() throws {
        let event = """
        {"sequence":41,"conversationId":"everyone","taskId":"","kind":"thread","data":{
         "thread":{"id":"t","conversation_id":"everyone","parent_entry_id":"p","parent_preview":"hi",
         "parent_sender":"You","reply_count":1,"last_activity_at":30,"created_at":20,"participants":["Ada"]}}}
        """
        let update = try JSONDecoder().decode(MarbleEvent.self, from: Data(event.utf8))
        XCTAssertEqual(update.data.thread?.parentEntryId, "p")
        XCTAssertEqual(update.data.thread?.summary.replyLabel, "1 reply")
        let reply = #"{"id":"a","conversationId":"everyone","kind":"assistant","text":"hello","at":30,"thread_id":"t"}"#
        let entry = try JSONDecoder().decode(MarbleEntry.self, from: Data(reply.utf8))
        XCTAssertEqual(entry.id, "a")
        XCTAssertEqual(entry.threadId, "t")
    }

    func testTurnEventsDecodeOptionalThreadMembership() throws {
        let threadText = #"{"sequence":42,"conversationId":"everyone","taskId":"turn","kind":"text","data":{"text":"thread reply","thread_id":"t"}}"#
        let mainText = #"{"sequence":43,"conversationId":"everyone","taskId":"main","kind":"text","data":{"text":"main reply"}}"#
        let decoder = JSONDecoder()
        XCTAssertEqual(try decoder.decode(MarbleEvent.self, from: Data(threadText.utf8)).data.threadId, "t")
        XCTAssertNil(try decoder.decode(MarbleEvent.self, from: Data(mainText.utf8)).data.threadId)
    }

    func testSearchDecodesThreadAndAppliesUTF8Ranges() throws {
        let json = """
        {"entry_id":"a","conversation_id":"everyone","thread_id":"t","sender":"Ada","created_at":40,
         "snippet":"🧵 café and math","match_ranges":[[5,5],[15,4],[2,1],[0,100],[6,2]]}
        """
        let result = try JSONDecoder().decode(MarbleSearchResult.self, from: Data(json.utf8))
        XCTAssertEqual(result.threadId, "t")
        XCTAssertEqual(result.sender, "Ada")
        XCTAssertEqual(result.highlights.map { String(result.snippet[$0]) }, ["café", "math"])
    }

    func testThreadSendAndSearchAroundUseTheContractKeys() throws {
        let data = try JSONEncoder().encode(MarbleTurnBody(text: "reply", conversationId: "c", threadId: "t"))
        let json = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: String])
        XCTAssertEqual(json, ["text": "reply", "conversationId": "c", "thread_id": "t"])
        let route = MarbleRoute.transcript(conversationId: "c", after: 12, main: true, around: "p")
        XCTAssertEqual(route.query.map(\.name), ["conversationId", "around", "main"])
        XCTAssertEqual(MarbleRoute.messageSearch("reply", threadId: "t", limit: 30).query.last?.value, "t")
    }

    func testThreadCreationPushesOnTheActiveTab() {
        var navigation = MarbleNavigation()
        navigation.notificationTapped("everyone")
        navigation.openThread("t")
        XCTAssertEqual(navigation.everyonePath, [.everyoneChat, .thread("t")])
        navigation.openMarble("ada")
        navigation.openThread("u")
        XCTAssertEqual(navigation.chatPath, [.marbleChat("ada"), .thread("u")])
    }

    func testHistoryPagingRemainsOptionalAndDistinguishesAPrunedCursor() throws {
        let decoder = JSONDecoder()
        let legacy = try decoder.decode(MarbleEveryonePage.self, from: Data(#"{"entries":[]}"#.utf8))
        XCTAssertNil(legacy.hasMore)
        let paged = try decoder.decode(MarbleEveryonePage.self, from: Data(
            #"{"entries":[],"hasMore":true,"nextBefore":"entry-50"}"#.utf8
        ))
        XCTAssertEqual(paged.hasMore, true)
        XCTAssertEqual(paged.nextBefore, "entry-50")
        XCTAssertEqual(MarbleRoute.everyone(after: nil, main: true, before: paged.nextBefore, limit: 50).query,
                       [.init(name: "main", value: "1"), .init(name: "before", value: "entry-50"),
                        .init(name: "limit", value: "50")])
        XCTAssertEqual(MarbleClientError.status(404, body: #"{"error":"unknown_cursor"}"#), .unknownHistoryCursor)
        XCTAssertEqual(MarbleClientError.status(404, body: #"{"error":"unknown_thread"}"#), .notFound)
    }

    func testHistoryWindowPrependsInHostOrderAndRefreshesOverlappingRows() {
        func entry(_ id: String, text: String = "") -> MarbleEntry {
            var row = MarbleEntry(conversationId: "everyone", kind: "assistant", text: text, taskId: "", at: 0)
            row.serverID = id
            return row
        }
        let a = entry("a"), b = entry("b"), c = entry("c"), d = entry("d"), e = entry("e")
        let existing = MarbleHistoryWindow.prepending([a, b, c], to: [c, d])
        XCTAssertEqual(existing.map(\.id), ["a", "b", "c", "d"])
        let (updated, before) = MarbleHistoryWindow.refreshing(
            existing, before: "a", with: [entry("c", text: "Finished"), d, e], nextBefore: "c"
        )
        XCTAssertEqual(updated.map(\.id), ["a", "b", "c", "d", "e"])
        XCTAssertEqual(updated[2].text, "Finished")
        XCTAssertEqual(before, "a")
        XCTAssertEqual(MarbleHistoryWindow.refreshing(updated, before: before, with: [e], nextBefore: nil).0, [e])
        let (cached, cursor) = MarbleHistoryWindow.cached((0..<60).map { entry("row-\($0)") }, before: nil)
        XCTAssertEqual(cached.count, 50)
        XCTAssertEqual(cursor, "row-10")
    }

    func testEveryoneDeletionInvalidatesAnOverlappingOlderPage() throws {
        let event = try JSONDecoder().decode(MarbleEvent.self, from: Data(
            #"{"sequence":2,"conversationId":"","taskId":"","kind":"conversation","data":{"deleted":"removed-chat"}}"#.utf8
        ))
        XCTAssertEqual(event.data.deleted, "removed-chat")
        func entry(_ id: String, task: String) -> MarbleEntry {
            var row = MarbleEntry(conversationId: "everyone", kind: "assistant", text: id, taskId: task, at: 0)
            row.serverID = id
            return row
        }
        let deleted = entry("old-mirror", task: "removed-turn")
        let first = entry("retained", task: "retained-turn")
        let latest = entry("latest", task: "new-turn")
        let (rows, cursor) = MarbleHistoryWindow.refreshing(
            [deleted, first], before: deleted.id, with: [first, latest],
            nextBefore: first.id, invalidated: event.data.deleted != nil
        )
        XCTAssertEqual(rows, [first, latest])
        XCTAssertEqual(cursor, first.id)
    }
}
