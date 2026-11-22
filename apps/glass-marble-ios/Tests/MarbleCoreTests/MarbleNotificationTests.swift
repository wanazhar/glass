import Foundation
import XCTest
@testable import MarbleCore

final class MarbleNotificationTests: XCTestCase {
    private func reply(
        _ sequence: UInt64, id: String = "entry", kind: String = "assistant",
        eventKind: String = "reply", conversation: String = "chat"
    ) -> MarbleEvent {
        MarbleEvent(sequence: sequence, conversationId: conversation, taskId: "turn", kind: eventKind,
                    data: .init(entryId: id, entryKind: kind, text: "Example"))
    }

    func testReplayRecordsLatestReplyWithoutNotifyingBeforeOrAtBaseline() {
        var cursor = MarbleNotificationCursor()
        XCTAssertNil(cursor.consume(reply(9, id: "first"), liveAfter: nil))
        XCTAssertNil(cursor.consume(reply(10, id: "second"), liveAfter: 10))
        XCTAssertEqual(cursor.lastSeenMessageID, "second")
        XCTAssertNil(cursor.lastNotifiedMessageID)
        XCTAssertEqual(cursor.consume(reply(11, id: "live"), liveAfter: 10)?.entry.id, "live")
    }

    func testStableEntryIDIsDedupedAcrossConnectionsAndLaunches() throws {
        var cursor = MarbleNotificationCursor()
        let entry = try XCTUnwrap(cursor.consume(reply(11), liveAfter: 10)?.entry)
        cursor.notified(entry)
        let data = try JSONEncoder().encode(cursor)
        var restored = try JSONDecoder().decode(MarbleNotificationCursor.self, from: data)
        XCTAssertEqual(restored.lastNotifiedMessageID, "entry")
        XCTAssertNil(restored.consume(reply(12), liveAfter: 11))
        XCTAssertNotNil(restored.consume(reply(13, id: "another"), liveAfter: 11))
    }

    func testReplyDeduplicationIsScopedToTheVisibleChat() {
        var cursor = MarbleNotificationCursor()
        XCTAssertNotNil(cursor.consume(reply(11, conversation: "everyone"), liveAfter: 10))
        XCTAssertNotNil(cursor.consume(reply(12, conversation: "private"), liveAfter: 10))
        XCTAssertNil(cursor.consume(reply(13, conversation: "everyone"), liveAfter: 10))
        XCTAssertNil(cursor.consume(reply(14, conversation: "private"), liveAfter: 10))
    }

    func testOnlyPersistedAssistantAndFailureRepliesQualify() {
        var cursor = MarbleNotificationCursor()
        for kind in ["user", "tool", "reasoning"] {
            XCTAssertNil(cursor.consume(reply(11, kind: kind), liveAfter: 10))
        }
        XCTAssertNil(cursor.consume(reply(12, eventKind: "settled"), liveAfter: 10))
        XCTAssertNil(cursor.lastSeenMessageID)
        XCTAssertEqual(cursor.consume(reply(13, kind: "failure"), liveAfter: 10)?.title, "Marble hit a problem")
    }

    func testReplyEventDecodesItsStableEntryIDAndThread() throws {
        let payload = #"{"sequence":11,"conversationId":"chat","taskId":"turn","kind":"reply","data":{"id":"entry","kind":"assistant","text":"Example","thread_id":"thread"}}"#
        let event = try JSONDecoder().decode(MarbleEvent.self, from: Data(payload.utf8))
        var cursor = MarbleNotificationCursor()
        XCTAssertEqual(event.data.threadId, "thread")
        XCTAssertEqual(cursor.consume(event, liveAfter: 10)?.entry.id, "entry")
    }

    func testOnlyVisibleChatIsSuppressed() {
        var navigation = MarbleNavigation()
        XCTAssertNil(navigation.visibleChat { _ in nil })
        navigation.notificationTapped("everyone")
        XCTAssertFalse(MarbleNotificationCursor.shouldPresent(
            target: "everyone", visibleChat: navigation.visibleChat { _ in nil }
        ))
        navigation.openMarble("ada")
        let namedChat = navigation.visibleChat { _ in "ada-chat" }
        XCTAssertFalse(MarbleNotificationCursor.shouldPresent(target: "ada-chat", visibleChat: namedChat))
        XCTAssertTrue(MarbleNotificationCursor.shouldPresent(target: "other-chat", visibleChat: namedChat))
        navigation.selectTab(.settings)
        XCTAssertTrue(MarbleNotificationCursor.shouldPresent(
            target: "ada-chat", visibleChat: navigation.visibleChat { _ in "ada-chat" }
        ))
    }

    func testThreadNotificationIsMutedOnlyWhenThatThreadIsOnScreen() {
        var navigation = MarbleNavigation()
        navigation.notificationTapped("everyone")
        XCTAssertTrue(MarbleNotificationCursor.shouldPresent(
            target: "everyone", visibleChat: "everyone", threadId: "t",
            visibleThreadId: navigation.visibleThreadId
        ))
        navigation.openThread("t")
        XCTAssertEqual(navigation.visibleThreadId, "t")
        XCTAssertFalse(MarbleNotificationCursor.shouldPresent(
            target: "everyone", visibleChat: nil, threadId: "t",
            visibleThreadId: navigation.visibleThreadId
        ))
        XCTAssertTrue(MarbleNotificationCursor.shouldPresent(
            target: "everyone", visibleChat: nil, threadId: "other",
            visibleThreadId: navigation.visibleThreadId
        ))
        navigation.everyonePath = [.searchHit(conversationId: "everyone", threadId: "t", entryId: "reply")]
        XCTAssertEqual(navigation.visibleThreadId, "t")
        XCTAssertFalse(MarbleNotificationCursor.shouldPresent(
            target: "everyone", visibleChat: nil, threadId: "t", visibleThreadId: navigation.visibleThreadId
        ))
        XCTAssertTrue(MarbleNotificationCursor.shouldPresent(
            target: "everyone", visibleChat: nil, threadId: "other", visibleThreadId: navigation.visibleThreadId
        ))
        navigation.selectTab(.settings)
        XCTAssertNil(navigation.visibleThreadId)
    }
}
