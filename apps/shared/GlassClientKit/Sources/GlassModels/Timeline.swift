import Foundation

/// Conversation state built from history windows plus live events, mirroring the web timeline.
public struct Timeline: Sendable, Hashable {
    public struct Notice: Sendable, Hashable {
        public var text: String
        public var isError: Bool
        public init(text: String, isError: Bool) { self.text = text; self.isError = isError }
    }

    public private(set) var entries: [TimelineEntry] = []
    /// Byte offset to pass as `before` for the next older window.
    public private(set) var olderBefore: Int?
    public private(set) var activity: String?
    public private(set) var notice: Notice?
    public private(set) var canWrite = false
    /// A snapshot or control event has said whether this client may write. Until then `canWrite` is a guess.
    public private(set) var controlKnown = false
    public private(set) var isActive = false
    public private(set) var disconnected = false
    public private(set) var largeRecordOmitted = false
    public private(set) var savedOutputs: [String: Int] = [:]
    public private(set) var toolDurations: [String: Duration] = [:]
    /// The decision the turn is paused on, if any.
    public private(set) var interaction: Interaction?

    private var seenRecords: Set<Int> = []
    private var liveSerial = 0
    private var streamID: String?
    private var toolStarted: [String: ContinuousClock.Instant] = [:]
    public private(set) var liveReasoningID: String?

    public init() {}

    public var hasOlder: Bool { (olderBefore ?? 0) > 0 }
    public var isEmpty: Bool { entries.isEmpty }

    /// Whether an older window would add anything visible. Session headers and private records produce no entries.
    public func hasVisibleEntries(_ page: HistoryPage) -> Bool {
        var seen = seenRecords
        var outputs: [String: Int] = [:]
        return !Self.entries(from: page, skipping: &seen, outputs: &outputs).isEmpty
    }

    /// Replaces everything with the newest history window (initial load and reset snapshots).
    public mutating func replaceHistory(_ page: HistoryPage) {
        entries = []
        seenRecords = []
        streamID = nil
        liveReasoningID = nil
        savedOutputs = [:]
        toolStarted = [:]
        entries = Self.entries(from: page, skipping: &seenRecords, outputs: &savedOutputs)
        rememberToolDurations(page)
        let present = Set(entries.map(\.id))
        toolDurations = toolDurations.filter { present.contains($0.key) }
        olderBefore = page.before
        largeRecordOmitted = page.omittedLargeRecord ?? false
    }

    /// Prepends an older window; records already shown (same byte offset) are skipped.
    public mutating func prependOlder(_ page: HistoryPage) {
        var older = Self.entries(from: page, skipping: &seenRecords, outputs: &savedOutputs)
        rememberToolDurations(page)
        // A backward page can reveal the call for an already loaded result.
        let newerTools = Dictionary(entries.compactMap { entry -> (String, TimelineEntry)? in
            if case .tool = entry { return (entry.id, entry) }
            return nil
        }, uniquingKeysWith: { first, _ in first })
        var mergedTools: Set<String> = []
        for index in older.indices {
            guard case .tool(let id, let name, let summary, _, _) = older[index],
                  case .tool(_, _, _, let output, let failed)? = newerTools[id] else { continue }
            older[index] = .tool(id: id, name: name, summary: summary, output: output, failed: failed)
            mergedTools.insert(id)
        }
        entries.removeAll { mergedTools.contains($0.id) }
        if case .reasoning(_, let prefix) = older.last,
           case .reasoning(let id, let suffix) = entries.first, id.hasPrefix("rec-") {
            older.removeLast()
            entries[0] = .reasoning(id: id, text: prefix + suffix)
        }
        entries.insert(contentsOf: older, at: 0)
        olderBefore = page.before
    }

    /// Applies a reset snapshot: fresh history then the in-flight turn's live events.
    public mutating func reset(history: HistoryPage, frame: EventFrame) {
        replaceHistory(history)
        applySnapshot(frame)
    }

    /// Applies a snapshot frame's control state over the history already shown.
    public mutating func applySnapshot(_ frame: EventFrame) {
        canWrite = frame.canWrite
        controlKnown = frame.control?["can_write"]?.boolValue != nil
        isActive = frame.active
        disconnected = false
        notice = nil
        interaction = frame.interaction.flatMap(Interaction.init)
        var live = frame.live[...]
        if frame.active,
           case .reasoning(let id, let saved) = entries.last,
           id.hasPrefix("rec-") {
            // The host appends reasoning to the log before publishing the live delta.
            // A reset can include some of the same text in both history and its live prefix.
            var prefix = ""
            var remaining = live
            while case .reasoning(let delta)? = remaining.first {
                prefix += delta
                remaining.removeFirst()
            }
            if prefix.hasPrefix(saved) || (frame.liveClipped && !prefix.isEmpty && saved.hasSuffix(prefix)) {
                liveReasoningID = id
                activity = "Reasoning"
                if prefix.hasPrefix(saved) {
                    apply(.reasoning(delta: String(prefix.dropFirst(saved.count))))
                }
                live = remaining
            }
        }
        for event in live { apply(event) }
        activity = frame.active ? (activity ?? "Working") : nil
    }

    public mutating func apply(_ frame: EventFrame, now: ContinuousClock.Instant = .now) {
        for event in frame.events { apply(event, now: now) }
    }

    public mutating func dismissNotice() { notice = nil }

    public mutating func apply(_ event: GlassEvent, now: ContinuousClock.Instant = .now) {
        switch event {
        case .turnStart(_, let text, let alreadyRecorded):
            finishStream()
            isActive = true
            if let text, !text.isEmpty, !alreadyRecorded { entries.append(.user(id: nextID("user"), text: text)) }
            activity = "Working"
        case .text(let delta):
            guard !delta.isEmpty else { break }
            notice = nil
            liveReasoningID = nil
            if let streamID, let index = entries.lastIndex(where: { $0.id == streamID }),
               case .assistant(let id, let text, _) = entries[index] {
                entries[index] = .assistant(id: id, text: text + delta, streaming: true)
            } else {
                let id = nextID("assistant")
                streamID = id
                entries.append(.assistant(id: id, text: delta, streaming: true))
            }
            activity = "Writing"
        case .reasoning(let delta):
            guard !delta.isEmpty else { break }
            finishStream(clearReasoning: false)
            if case .reasoning(let id, let text) = entries.last, id == liveReasoningID {
                entries[entries.count - 1] = .reasoning(id: id, text: text + delta)
            } else {
                let id = nextID("reasoning")
                liveReasoningID = id
                entries.append(.reasoning(id: id, text: delta))
            }
            activity = "Reasoning"
        case .tool(let callID, let name, let command):
            finishStream()
            let id = callID ?? nextID("tool")
            if !entries.contains(where: { $0.id == id }) {
                entries.append(.tool(id: id, name: name, summary: command, output: "", failed: nil))
                if callID != nil { toolStarted[id] = now }
            }
            activity = name == "bash" ? (command.isEmpty ? "Running a command…" : "Running `\(command)`…") : "Using \(name)…"
        case .toolEnd(let callID, let output, let summary, let truncated):
            if let callID, let started = toolStarted.removeValue(forKey: callID), started < now {
                toolDurations[callID] = started.duration(to: now)
            }
            let body = (output.isEmpty ? summary : output) + (truncated ? "\nPreview capped · full output remains on the host." : "")
            let failed = summary.range(of: "failed|error:|exit [1-9]", options: [.regularExpression, .caseInsensitive]) != nil
            if let callID, let index = entries.lastIndex(where: { $0.id == callID }),
               case .tool(let id, let name, let command, _, _) = entries[index] {
                entries[index] = .tool(id: id, name: name, summary: command, output: body, failed: failed)
            } else {
                finishStream()
                entries.append(.tool(id: callID ?? nextID("tool"), name: "tool", summary: "", output: body, failed: failed))
            }
        case .done:
            toolStarted.removeAll(keepingCapacity: true)
            finishStream()
            isActive = false
            notice = nil
            activity = nil
        case .waiting:
            activity = "Waiting for the model"
        case .queued:
            notice = Notice(text: "Follow-up saved on the host. It will run after the current turn.", isError: false)
        case .duplicate(let message):
            notice = Notice(text: message ?? "This request is already recorded; it was not run twice.", isError: false)
        case .steering:
            notice = Notice(text: "Applying your correction before further actions.", isError: false)
        case .error(let message, let settled, _, _):
            if settled {
                interaction = nil
                finishStream()
                isActive = false
                activity = nil
            }
            notice = Notice(text: message, isError: true)
        case .notice(let message):
            notice = Notice(text: message, isError: false)
        case .disconnected(let message):
            disconnected = true
            notice = Notice(text: message ?? "The host disconnected.", isError: true)
        case .control(let canWrite):
            self.canWrite = canWrite
            controlKnown = true
        case .commandStart(let name):
            activity = name.map { "Working · /\($0)" } ?? "Working"
        case .commandDone:
            activity = nil
        case .meta(let value):
            if let elapsed = value["turn_seconds"]?.doubleValue, elapsed >= 0, elapsed < Double(Int.max) {
                entries.append(.turnTiming(id: nextID("timing"), seconds: Int(elapsed)))
            }
            if let text = value["t"]?.stringValue, text.range(of: "error:|failed:|stopped:", options: .regularExpression) != nil {
                finishStream()
                entries.append(.notice(id: nextID("meta"), text: text, isError: true))
            }
        case .interaction(let value):
            interaction = Interaction(value)
            activity = "Waiting for your decision"
        case .interactionDone:
            interaction = nil
            if activity == "Waiting for your decision" { activity = isActive ? "Working" : nil }
        case .commandResult, .commandCatalogue, .commandQueued, .unknown:
            break
        }
    }

    private mutating func rememberToolDurations(_ page: HistoryPage) {
        for item in page.records {
            guard let raw = item.record["duration_ms"]?.doubleValue,
                  let milliseconds = Int64(exactly: raw), milliseconds >= 0,
                  case .tool(let id, _, _, _, _)? = item.entry else { continue }
            toolDurations[id] = .milliseconds(milliseconds)
        }
    }

    private mutating func finishStream(clearReasoning: Bool = true) {
        if clearReasoning { liveReasoningID = nil }
        guard let streamID, let index = entries.lastIndex(where: { $0.id == streamID }),
              case .assistant(let id, let text, _) = entries[index] else {
            self.streamID = nil
            return
        }
        entries[index] = .assistant(id: id, text: text, streaming: false)
        self.streamID = nil
    }

    private mutating func nextID(_ kind: String) -> String {
        liveSerial += 1
        return "live-\(kind)-\(liveSerial)"
    }

    private static func entries(from page: HistoryPage, skipping seen: inout Set<Int>,
                                outputs: inout [String: Int]) -> [TimelineEntry] {
        var ordered: [(Int, TimelineEntry)] = []
        for omitted in page.omitted ?? [] where seen.insert(omitted.recordOffset).inserted {
            ordered.append((omitted.recordOffset, .largeRecord(id: "rec-\(omitted.recordOffset)", offset: omitted.recordOffset, bytes: omitted.bytes)))
        }
        for item in page.records where seen.insert(item.index).inserted {
            if let entry = item.entry {
                ordered.append((item.index, entry))
                if case .tool = entry, item.previewTruncated == true {
                    outputs[entry.id] = item.index
                }
            }
        }
        var result: [TimelineEntry] = []
        var tools: [String: Int] = [:]
        for (_, entry) in ordered.sorted(by: { $0.0 < $1.0 }) {
            if case .tool = entry, let index = tools[entry.id] {
                // A tool result record completes the matching call record.
                if case .tool(let id, _, _, let output, let failed) = entry, failed != nil,
                   case .tool(_, let name, let summary, _, _) = result[index] {
                    result[index] = .tool(id: id, name: name, summary: summary, output: output, failed: failed)
                }
                continue
            }
            if case .reasoning(let id, let prefix) = result.last,
               case .reasoning(_, let suffix) = entry {
                result[result.count - 1] = .reasoning(id: id, text: prefix + suffix)
            } else {
                if case .tool = entry { tools[entry.id] = result.count }
                result.append(entry)
            }
        }
        return result
    }
}
