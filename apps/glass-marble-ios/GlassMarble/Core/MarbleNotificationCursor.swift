import Foundation

/// Baseline first launch at the host's current sequence. Persist the high
/// water mark of handled alerts, including suppressed ones, to ignore replay.
struct MarbleNotificationCursor: Codable, Equatable {
    private(set) var lastNotifiedEventID: UInt64
    private var hostPID: Int?
    private var recentKeys: [String] = []

    init(hostSequence: UInt64, hostPID: Int?) {
        lastNotifiedEventID = hostSequence
        self.hostPID = hostPID
    }

    @discardableResult
    mutating func align(hostSequence: UInt64, hostPID: Int?) -> Bool {
        if hostSequence < lastNotifiedEventID || (self.hostPID != nil && hostPID != nil && self.hostPID != hostPID) {
            self = Self(hostSequence: hostSequence, hostPID: hostPID)
            return true
        } else if let hostPID {
            self.hostPID = hostPID
        }
        return false
    }

    mutating func consume(_ event: MarbleEvent) -> MarbleAttention.Alert? {
        guard event.sequence > lastNotifiedEventID, !event.conversationId.isEmpty,
              event.kind == "settled" || event.kind == "approval_pending" else { return nil }
        lastNotifiedEventID = event.sequence
        var keys = Set(recentKeys)
        guard let alert = MarbleAttention.alert(for: event, seen: &keys) else { return nil }
        recentKeys = Array(recentKeys.suffix(63)) + [alert.key]
        return alert
    }

    static func shouldPresent(target: String?, visibleChat: String?) -> Bool {
        guard let target, let visibleChat else { return true }
        return target != visibleChat
    }
}
