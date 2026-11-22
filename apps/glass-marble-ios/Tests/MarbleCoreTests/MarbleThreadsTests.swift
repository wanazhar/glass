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
}
