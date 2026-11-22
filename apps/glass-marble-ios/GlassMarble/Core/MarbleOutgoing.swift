import Foundation

enum MarbleSendTarget: Hashable {
    case conversation(String?, thread: String? = nil)
    case everyone(thread: String? = nil)
    case marble(String)

    var conversationId: String? {
        guard case .conversation(let id, _) = self else { return nil }
        return id
    }

    var threadId: String? {
        switch self {
        case let .conversation(_, thread), let .everyone(thread): return thread
        case .marble: return nil
        }
    }
}

struct MarbleOutgoing: Identifiable, Equatable {
    let id: UUID
    var target: MarbleSendTarget
    let text: String
    let previousUserIDs: Set<String>
    let previousOutcomeIDs: Set<String>
    var responders: [String]
    var delivery: Delivery = .sending
    var confirmedEntryID: String?

    enum Delivery: Equatable {
        case sending
        case pending
        case accepted(taskId: String?, entryId: String?)
        case failed
    }

    var rowID: String { "outgoing-\(id.uuidString.lowercased())" }

    static func canStart(_ target: MarbleSendTarget, alongside outgoing: [Self]) -> Bool {
        guard target == .conversation(nil) else { return true }
        return !outgoing.contains { $0.target == target && $0.delivery != .failed }
    }

    init(id: UUID = UUID(), target: MarbleSendTarget, text: String, responders: [String], entries: [MarbleEntry]) {
        self.id = id
        self.target = target
        self.text = text
        self.responders = responders
        previousUserIDs = Set(entries.lazy.filter { $0.kind == "user" && $0.text == text }.map(\.id))
        previousOutcomeIDs = Set(entries.lazy.filter(\.isReplyOutcome).map(\.id))
    }

    func presentationEntry(in conversationId: String) -> MarbleEntry {
        var entry = MarbleEntry(conversationId: conversationId, kind: "user", text: text, taskId: "", at: 0)
        entry.threadId = target.threadId
        entry.presentationID = rowID
        if delivery == .failed { entry.failedSendID = id }
        return entry
    }

    func match(in entries: [MarbleEntry]) -> MarbleEntry? {
        if let echoed = entries.first(where: {
            $0.kind == "user" && $0.clientMessageID?.lowercased() == id.uuidString.lowercased()
        }) { return echoed }
        if let confirmedEntryID { return entries.first { $0.id == confirmedEntryID } }
        guard case let .accepted(taskId, entryId) = delivery else { return nil }
        if let entryId { return entries.first { $0.serverID == entryId } }
        // Older hosts omit client_message_id. Use a unique new matching user
        // entry, plus the task ID wherever the transcript carries one.
        return legacyMatch(in: entries, taskId: taskId)
    }

    mutating func recordFailure(_ error: MarbleClientError) {
        guard confirmedEntryID == nil else { return }
        delivery = error == .conflict("client_message_pending") ? .pending : .failed
    }

    mutating func confirm(_ entry: MarbleEntry) {
        confirmedEntryID = entry.id
        let taskId = entry.taskId.isEmpty ? nil : entry.taskId
        if case let .accepted(previousTaskId, _) = delivery {
            delivery = .accepted(taskId: previousTaskId ?? taskId, entryId: entry.id)
        } else {
            delivery = .accepted(taskId: taskId, entryId: entry.id)
        }
    }

    func legacyMatch(in entries: [MarbleEntry], taskId: String? = nil) -> MarbleEntry? {
        var candidates = entries.filter {
            $0.kind == "user" && $0.clientMessageID == nil && $0.text == text
                && $0.threadId == target.threadId
                && !previousUserIDs.contains($0.id)
                && (taskId == nil || $0.taskId.isEmpty || $0.taskId == taskId)
        }
        if let taskId, candidates.contains(where: { $0.taskId == taskId }) {
            candidates.removeAll { $0.taskId != taskId }
        }
        return candidates.count == 1 ? candidates.first : nil
    }
}

struct MarbleOutgoingPresentation {
    var entries: [MarbleEntry]
    var responders: [String] = []

    init(durable: [MarbleEntry], outgoing: [MarbleOutgoing], conversationId: String) {
        entries = durable
        var claimed = Set<String>()
        var matches: [UUID: MarbleEntry] = [:]
        for send in outgoing {
            if let match = send.match(in: durable), claimed.insert(match.id).inserted {
                matches[send.id] = match
            }
        }
        // On older hosts the transcript may arrive first. Reuse the optimistic
        // row while the POST is pending, but only when the match is unambiguous.
        // Leave permanent identity mapping to the echoed ID or acknowledgment.
        let unmatched = Dictionary(grouping: outgoing.filter {
            matches[$0.id] == nil && $0.confirmedEntryID == nil
        }, by: \.text)
        for send in outgoing where send.delivery == .sending && unmatched[send.text]?.count == 1 {
            if let match = send.legacyMatch(in: durable.filter { !claimed.contains($0.id) }) {
                claimed.insert(match.id)
                matches[send.id] = match
            }
        }
        for send in outgoing {
            let match = matches[send.id]
            if let match,
               let index = entries.firstIndex(where: { $0.id == match.id }) {
                entries[index].presentationID = send.rowID
            } else if send.confirmedEntryID == nil {
                let row = send.presentationEntry(in: conversationId)
                if case let .accepted(taskId?, _) = send.delivery,
                   let index = entries.firstIndex(where: { $0.taskId == taskId && $0.isReplyOutcome }) {
                    entries.insert(row, at: index)
                } else {
                    entries.append(row)
                }
            }
            if match == nil && send.confirmedEntryID != nil { continue }
            guard send.delivery != .failed || match != nil else { continue }
            let outcomes = durable.filter { $0.isReplyOutcome && !send.previousOutcomeIDs.contains($0.id) }
            let settledTask: Bool
            if case let .accepted(taskID?, _) = send.delivery {
                settledTask = outcomes.contains { $0.taskId == taskID }
            } else {
                settledTask = false
            }
            if !settledTask {
                for responder in send.responders where !responders.contains(responder) && !outcomes.contains(where: {
                    $0.speaker == responder || (
                        send.responders.count == 1 && $0.speaker.isEmpty && conversationId != everyoneConversationId
                    )
                }) {
                    responders.append(responder)
                }
            }
        }
    }
}

private extension MarbleEntry {
    var isReplyOutcome: Bool { ["assistant", "failure", "error"].contains(kind) }
}
