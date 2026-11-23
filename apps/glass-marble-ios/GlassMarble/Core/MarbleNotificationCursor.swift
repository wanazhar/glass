import Foundation

/// Tracks notification eligibility for one running stream task, including
/// automatic retries while background keep-alive remains active.
struct MarbleReplyDeliveryWindow {
    private(set) var liveAfter: UInt64?

    mutating func opened(at cursor: UInt64, keepingAlive: Bool) {
        if !keepingAlive || liveAfter == nil { liveAfter = cursor }
    }

    mutating func interrupted(keepingAlive: Bool) {
        if !keepingAlive { liveAfter = nil }
    }

    mutating func hostRestarted(keepingAlive: Bool) {
        // The host's new event buffer starts empty. Durable history loaded
        // into the new process never generates reply events.
        if keepingAlive, liveAfter != nil {
            liveAfter = 0
        } else {
            liveAfter = nil
        }
    }
}

/// The event stream's hello cursor separates replay from live replies.
/// Stable entry IDs deduplicate replies within and across connections.
struct MarbleNotificationCursor: Codable, Equatable {
    struct EntryKey: Codable, Equatable {
        let conversationId: String
        let id: String
    }

    private var lastNotifiedEntry: EntryKey?
    private var recentEntries: [EntryKey] = []
    var lastSeenMessageID: String? { recentEntries.last?.id }
    var lastNotifiedMessageID: String? { lastNotifiedEntry?.id }

    struct Reply: Equatable {
        let entry: EntryKey
        let title: String
        let text: String
    }

    mutating func consume(_ event: MarbleEvent, liveAfter: UInt64?) -> Reply? {
        guard event.kind == "reply",
              let id = event.data.entryId, !id.isEmpty,
              !event.conversationId.isEmpty,
              let text = event.data.text, !text.isEmpty,
              let kind = event.data.entryKind, kind == "assistant" || kind == "failure" else { return nil }
        let entry = EntryKey(conversationId: event.conversationId, id: id)
        guard entry != lastNotifiedEntry, !recentEntries.contains(entry) else { return nil }
        recentEntries.append(entry)
        if recentEntries.count > 128 { recentEntries.removeFirst() }
        guard let liveAfter, event.sequence > liveAfter else { return nil }
        return Reply(entry: entry, title: kind == "failure" ? "Marble hit a problem" : "New reply from Marble", text: text)
    }

    mutating func notified(_ entry: EntryKey) {
        lastNotifiedEntry = entry
    }

    static func shouldPresent(
        target: String?,
        visibleChat: String?,
        threadId: String? = nil,
        visibleThreadId: String? = nil,
        mirroredInEveryone: Bool = false
    ) -> Bool {
        if let threadId { return threadId != visibleThreadId }
        if mirroredInEveryone && visibleChat == everyoneConversationId { return false }
        guard let target, let visibleChat else { return true }
        return target != visibleChat
    }

    static func isMirroredInEveryone(_ reply: MarbleEvent, entries: [MarbleEntry]) -> Bool {
        let taskId = reply.data.taskId ?? reply.taskId
        guard reply.conversationId != everyoneConversationId, reply.data.threadId == nil,
              !taskId.isEmpty else { return false }
        return entries.contains {
            $0.conversationId == everyoneConversationId && $0.threadId == nil && $0.taskId == taskId
                && ["pending", "assistant", "failure"].contains($0.kind)
        }
    }
}
