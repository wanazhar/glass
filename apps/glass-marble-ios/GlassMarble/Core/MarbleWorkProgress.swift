import Foundation

struct MarbleWorkProgress {
    private struct Turn {
        var phase: MarbleWorkPhase = .thinking
        var tools: Set<String> = []
        var finished = false
    }

    private var turns: [String: Turn] = [:]
    private var recent: [String] = []

    func isFinished(_ id: String) -> Bool { turns[id]?.finished == true }
    func phase(for id: String) -> MarbleWorkPhase { turns[id]?.phase ?? .thinking }

    /// Return true only when the presentation might need an update.
    mutating func apply(_ event: MarbleEvent) -> Bool {
        let id = event.data.taskId ?? event.taskId
        guard !id.isEmpty else { return false }
        let previous = turns[id] ?? Turn()
        var turn = previous
        switch event.kind {
        case "task_retry":
            turn = Turn()
        case "settled" where event.data.cancelled != nil || event.data.taskId != nil,
             "reply", "error",
             "state" where ["completed", "cancelled", "failed", "interrupted"].contains(event.data.status ?? ""):
            turn.finished = true
        case "cancel_requested" where !turn.finished:
            turn.phase = .stopping
        case "tool_start", "tool_end", "replying", "text":
            guard !turn.finished, turn.phase != .stopping else { return false }
            switch event.kind {
            case "tool_start":
                turn.tools.insert(event.data.callId ?? "")
                turn.phase = .tool
            case "tool_end":
                guard turn.tools.remove(event.data.callId ?? "") != nil else { return false }
                turn.phase = turn.tools.isEmpty ? .thinking : .tool
            default:
                if turn.tools.isEmpty { turn.phase = .replying }
            }
        default:
            return false
        }
        if turns[id] == nil {
            if recent.count == 128 { turns.removeValue(forKey: recent.removeFirst()) }
            recent.append(id)
        }
        turns[id] = turn
        return turn.finished != previous.finished || turn.phase != previous.phase
    }
}
