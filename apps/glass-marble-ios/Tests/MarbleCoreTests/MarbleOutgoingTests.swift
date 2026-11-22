import Foundation
import XCTest
@testable import MarbleCore

final class MarbleOutgoingTests: XCTestCase {
    func testOnlyOneUnresolvedDraftSendCanStart() {
        let draft = MarbleSendTarget.conversation(nil)
        let resolved = MarbleSendTarget.conversation("created-chat")
        var first = MarbleOutgoing(target: draft, text: "first message", responders: ["marble"], entries: [])
        XCTAssertTrue(MarbleOutgoing.canStart(draft, alongside: []))
        XCTAssertFalse(MarbleOutgoing.canStart(draft, alongside: [first]))
        first.delivery = .pending
        XCTAssertFalse(MarbleOutgoing.canStart(draft, alongside: [first]))
        XCTAssertTrue(MarbleOutgoing.canStart(resolved, alongside: [first]))
        first.delivery = .failed
        XCTAssertTrue(MarbleOutgoing.canStart(draft, alongside: [first]))
        first.target = resolved
        XCTAssertTrue(MarbleOutgoing.canStart(draft, alongside: [first]))
    }

    func testBubbleAndRespondersAreProjectedFromTheSameSend() {
        let send = MarbleOutgoing(target: .everyone(), text: "@Ada @Ben hi", responders: ["ada", "ben"], entries: [])
        let draft = MarbleOutgoingPresentation(durable: [], outgoing: [send], conversationId: "everyone")
        XCTAssertEqual(draft.entries.map(\.text), [send.text])
        XCTAssertEqual(draft.responders, ["ada", "ben"])
    }

    func testEchoedIDReconcilesEvenIfHistoryArrivesBeforePostReturns() {
        let send = MarbleOutgoing(target: .everyone(), text: "same message", responders: ["ada"], entries: [])
        let optimistic = MarbleOutgoingPresentation(durable: [], outgoing: [send], conversationId: "everyone")
        var previous = MarbleEntry.preview(kind: "user", text: send.text)
        previous.serverID = "older"
        var current = previous
        current.serverID = "new"
        current.clientMessageID = send.id.uuidString.lowercased()
        let confirmed = MarbleOutgoingPresentation(durable: [previous, current], outgoing: [send], conversationId: "everyone")
        XCTAssertEqual(confirmed.entries.map(\.id), ["older", "new"])
        XCTAssertEqual(confirmed.entries.last?.rowID, optimistic.entries.first?.rowID)
    }

    func testPersistedEntryEventReconcilesEvenWhenPostResponseIsLost() throws {
        var send = MarbleOutgoing(target: .everyone(thread: "thread"), text: "hello",
                                  responders: ["ada"], entries: [])
        let payload = """
        {
          "sequence":123,"conversationId":"everyone","taskId":"turn","kind":"entry",
          "data":{"id":"entry-1","client_message_id":"\(send.id)","conversationId":"private",
                  "kind":"user","text":"hello","taskId":"turn","at":1770000000000,"thread_id":"thread"}
        }
        """
        let event = try JSONDecoder().decode(MarbleEvent.self, from: Data(payload.utf8))
        let entry = try XCTUnwrap(event.userEntry)
        XCTAssertEqual(entry.conversationId, "everyone")
        XCTAssertEqual(entry.threadId, send.target.threadId)
        send.confirm(entry)
        send.recordFailure(.timeout)
        XCTAssertEqual(send.delivery, .accepted(taskId: "turn", entryId: "entry-1"))
        let result = MarbleOutgoingPresentation(durable: [entry], outgoing: [send], conversationId: "everyone")
        XCTAssertEqual(result.entries.map(\.rowID), [send.rowID])
        XCTAssertEqual(result.responders, ["ada"])
    }

    func testInterruptedAdmissionKeepsWaitingUntilPersistedEntryArrives() {
        var send = MarbleOutgoing(target: .everyone(), text: "hello", responders: ["ada"], entries: [])
        let error = MarbleClientError.status(409, body: #"{"error":"client_message_pending"}"#)
        send.recordFailure(error)
        XCTAssertEqual(send.delivery, .pending)
        let waiting = MarbleOutgoingPresentation(durable: [], outgoing: [send], conversationId: "everyone")
        XCTAssertEqual(waiting.entries.map(\.rowID), [send.rowID])
        XCTAssertNil(waiting.entries.first?.failedSendID)
        XCTAssertEqual(waiting.responders, ["ada"])
        var entry = MarbleEntry.preview(kind: "user", text: send.text)
        entry.serverID = "entry"
        entry.clientMessageID = send.id.uuidString
        XCTAssertEqual(send.match(in: [entry])?.id, entry.id)
        send.confirm(entry)
        XCTAssertEqual(send.delivery, .accepted(taskId: nil, entryId: "entry"))
    }

    func testSendReceiptsDecodeCanonicalEntryAndClientID() throws {
        let payload = """
        {"ok":true,"accepted":true,"requestId":"request","conversationId":"everyone",
         "taskId":"task","entry_id":"entry","client_message_id":"ios-123","answering":["Ada"],"busy":[]}
        """
        let data = Data(payload.utf8)
        let everyone = try JSONDecoder().decode(MarbleEveryoneReply.self, from: data)
        XCTAssertEqual(everyone.entryId, "entry")
        XCTAssertEqual(everyone.clientMessageID, "ios-123")
        let accepted = try JSONDecoder().decode(MarbleAccepted.self, from: data)
        XCTAssertEqual(accepted.entry_id, "entry")
        XCTAssertEqual(accepted.client_message_id, "ios-123")
    }

    func testLegacyMatchingUsesNewEntryAndTaskID() {
        var previous = MarbleEntry.preview(kind: "user", text: "hello")
        previous.serverID = "older"
        var send = MarbleOutgoing(target: .conversation("chat"), text: previous.text,
                                  responders: ["marble"], entries: [previous])
        send.delivery = .accepted(taskId: "task", entryId: nil)
        var other = previous
        other.serverID = "other"
        other.taskId = "other-task"
        var accepted = previous
        accepted.serverID = "accepted"
        accepted.taskId = "task"
        let result = MarbleOutgoingPresentation(durable: [previous, other, accepted], outgoing: [send],
                                                conversationId: "chat")
        XCTAssertEqual(result.entries.map(\.rowID), ["older", "other", send.rowID])
    }

    func testLegacyTranscriptBeforePostAcknowledgmentKeepsOneStableBubble() {
        var previous = MarbleEntry.preview(kind: "user", text: "same message")
        previous.serverID = "older"
        var send = MarbleOutgoing(target: .everyone(), text: previous.text,
                                  responders: ["ada"], entries: [previous])
        var current = previous
        current.serverID = "new"
        let pending = MarbleOutgoingPresentation(durable: [previous, current], outgoing: [send],
                                                 conversationId: "everyone")
        XCTAssertEqual(pending.entries.map(\.rowID), ["older", send.rowID])
        XCTAssertEqual(pending.responders, ["ada"])
        send.delivery = .accepted(taskId: nil, entryId: nil)
        let accepted = MarbleOutgoingPresentation(durable: [previous, current], outgoing: [send],
                                                  conversationId: "everyone")
        XCTAssertEqual(accepted.entries.map(\.rowID), pending.entries.map(\.rowID))
    }

    func testLegacyTranscriptDoesNotGuessBetweenIdenticalPendingSends() {
        let first = MarbleOutgoing(target: .everyone(), text: "same message",
                                   responders: ["ada"], entries: [])
        let second = MarbleOutgoing(target: .everyone(), text: first.text,
                                    responders: ["ada"], entries: [])
        var durable = MarbleEntry.preview(kind: "user", text: first.text)
        durable.serverID = "new"
        let ambiguous = MarbleOutgoingPresentation(durable: [durable], outgoing: [first, second],
                                                   conversationId: "everyone")
        XCTAssertEqual(ambiguous.entries.map(\.rowID), ["new", first.rowID, second.rowID])

        var accepted = first
        accepted.delivery = .accepted(taskId: nil, entryId: durable.id)
        let resolved = MarbleOutgoingPresentation(durable: [durable], outgoing: [accepted, second],
                                                  conversationId: "everyone")
        XCTAssertEqual(resolved.entries.map(\.rowID), [first.rowID, second.rowID])
    }

    func testFailureKeepsBubbleAndRetryIdentityAndDropsResponders() {
        var send = MarbleOutgoing(target: .everyone(thread: "thread"), text: "hello", responders: ["ada"], entries: [])
        send.delivery = .failed
        let failed = MarbleOutgoingPresentation(durable: [], outgoing: [send], conversationId: "everyone")
        XCTAssertEqual(failed.entries.first?.failedSendID, send.id)
        XCTAssertEqual(failed.entries.first?.rowID, send.rowID)
        XCTAssertEqual(failed.entries.first?.threadId, "thread")
        XCTAssertTrue(failed.responders.isEmpty)
        send.delivery = .sending
        let retry = MarbleOutgoingPresentation(durable: [], outgoing: [send], conversationId: "everyone")
        XCTAssertEqual(retry.entries.first?.rowID, failed.entries.first?.rowID)
        XCTAssertEqual(retry.responders, ["ada"])
    }

    func testEachResponderHandsOffToTheirOwnReply() {
        let send = MarbleOutgoing(target: .everyone(), text: "hello", responders: ["ada", "ben"], entries: [])
        var reply = MarbleEntry.preview(kind: "assistant", text: "hi")
        reply.speaker = "ada"
        let result = MarbleOutgoingPresentation(durable: [reply], outgoing: [send], conversationId: "everyone")
        XCTAssertEqual(result.responders, ["ben"])
    }
}
