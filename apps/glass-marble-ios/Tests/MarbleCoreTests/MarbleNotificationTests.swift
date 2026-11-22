import Foundation
import XCTest
@testable import MarbleCore

final class MarbleNotificationTests: XCTestCase {
    private func settled(_ sequence: UInt64, task: String = "turn") -> MarbleEvent {
        MarbleEvent(sequence: sequence, conversationId: "chat", taskId: task, kind: "settled", data: .init())
    }

    func testInitialBaselineSkipsHistoryAndPersistsAcrossLaunch() throws {
        var cursor = MarbleNotificationCursor(hostSequence: 10, hostPID: 1)
        XCTAssertNil(cursor.consume(settled(10)))
        XCTAssertNotNil(cursor.consume(settled(11)))
        let data = try JSONEncoder().encode(cursor)
        var restored = try JSONDecoder().decode(MarbleNotificationCursor.self, from: data)
        XCTAssertNil(restored.consume(settled(11)))
        XCTAssertNil(restored.consume(settled(12)), "The host publishes two settled frames per turn")
        XCTAssertNotNil(restored.consume(settled(13, task: "new-turn")))
    }

    func testReconnectPreservesCursorUnlessHostRestarts() {
        var cursor = MarbleNotificationCursor(hostSequence: 10, hostPID: 1)
        cursor.align(hostSequence: 14, hostPID: 1)
        XCTAssertNotNil(cursor.consume(settled(11)))
        cursor.align(hostSequence: 3, hostPID: 2)
        XCTAssertNil(cursor.consume(settled(3)))
        XCTAssertNotNil(cursor.consume(settled(4)))
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
        navigation.selectTab(.settings)
        XCTAssertNil(navigation.visibleThreadId)
    }
}
