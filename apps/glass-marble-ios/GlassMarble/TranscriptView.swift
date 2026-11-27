import SwiftUI

struct TranscriptView: View {
    @EnvironmentObject private var store: MarbleStore
    var conversationId: String?

    var body: some View {
        let target = MarbleSendTarget.conversation(conversationId)
        let durable = store.transcript(for: conversationId)
        let presentation = store.outgoingPresentation(durable, to: target, conversationId: conversationId)
        let entries = store.withLiveToolRows(presentation.entries, conversationId: conversationId)
        ScrollView {
                LazyVStack(alignment: .leading, spacing: 12) {
                    if entries.isEmpty && (!store.loaded || store.transcriptHistoryLoading(conversationId)) {
                        placeholderRows
                    } else if (store.connection == .offline || store.historyErrors.contains(conversationId ?? ""))
                        && entries.isEmpty
                        && store.liveText.isEmpty
                        && store.liveReasoning.isEmpty
                        && store.liveTools.isEmpty {
                        // Load failed: say so and offer a retry, never a silent
                        // empty conversation that looks settled.
                        ContentUnavailableView {
                            Label("Could not reach the host", systemImage: "network.slash")
                        } description: {
                            Text(store.errorMessage ?? "")
                        } actions: {
                            Button("Retry") { Task { await store.refresh() } }
                                .buttonStyle(.borderedProminent)
                        }
                    } else if entries.isEmpty
                        && store.liveText.isEmpty
                        && store.liveReasoning.isEmpty
                        && store.liveTools.isEmpty {
                        ContentUnavailableView(
                            "No messages yet",
                            systemImage: "text.bubble",
                            description: Text("Say hello, ask a question, or share what’s on your mind.")
                        )
                    }
                    let transcript = MarbleReasoningAssociation(entries)
                    ForEach(transcript.standalone, id: \.rowID) { entry in
                        MarbleEntryView(entry: entry, reasoning: transcript.attached[entry.id],
                                        toolRows: transcript.tools[entry.id] ?? [], timestamp: transcript.timestamps[entry.id])
                            .transition(.opacity.combined(with: .move(edge: .bottom)))
                    }
                    if !store.liveReasoning.isEmpty && !entries.contains(where: {
                        $0.taskId == store.activeTaskId && ["reasoning", "assistant"].contains($0.kind)
                    }) {
                        MarbleEntryView(entry: .preview(kind: "reasoning", text: store.liveReasoning))
                    }
                    let working = store.isWorking && !entries.contains {
                        $0.kind == "assistant" && $0.taskId == store.activeTaskId
                    }
                    let hasReservedReply = entries.contains { $0.kind == "pending" }
                    if !hasReservedReply && store.liveText.isEmpty && (!presentation.responders.isEmpty || working
                        || store.isAwaitingReply(in: conversationId, entries: entries)) {
                        MarbleThinkingBubble(status: store.activeTaskId.flatMap { store.workingLines[$0] })
                    }
                    if !store.liveText.isEmpty && store.activeTaskId == nil {
                        MarbleEntryView(entry: .preview(kind: "assistant", text: store.liveText))
                    }
                    MarbleTurnTail(task: store.latestTurn(in: conversationId), entries: entries)
                    MarbleTranscriptEnd(bottomPadding: 14)
                }
                .marbleHistoryScrollTargets()
                .padding(.horizontal, 16)
                .padding(.top, 14)
                // Cap the measure on iPad and Mac so a reply never becomes a
                // single 1,000-character line.
                .frame(maxWidth: 820, alignment: .leading)
                .frame(maxWidth: .infinity)
        }
        .background(MarbleTheme.paper)
        .scrollDismissesKeyboard(.interactively)
        .marbleFollowsLatest(hasMessages: !entries.isEmpty, outgoing: store.lastOutgoing(to: target)) { beforePrepend in
            if let conversationId { await store.loadOlderHistory(conversationId, beforePrepend: beforePrepend) }
        }
    }

    /// Shaped rows keep the first load from looking like an empty
    /// conversation; system redaction does the shimmer-free placeholder.
    private var placeholderRows: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Placeholder user message")
                .frame(maxWidth: .infinity, alignment: .trailing)
            Text("A placeholder assistant reply that runs a little longer.")
            Text("Another placeholder")
        }
        .font(.body)
        .redacted(reason: .placeholder)
        .accessibilityHidden(true)
    }

}

struct MarbleHistoryUnavailable: View {
    let retry: () async -> Void

    var body: some View {
        ContentUnavailableView {
            Label("Couldn’t load messages", systemImage: "network.slash")
        } actions: {
            Button("Retry") { Task { await retry() } }
        }
    }
}

/// One transcript entry. The person's own messages sit in a blue bubble on
/// the right; replies read as full-width Markdown under a clear speaker line
/// with the marble's avatar and name in its colour.
struct MarbleEntryView: View {
    @EnvironmentObject private var store: MarbleStore
    @EnvironmentObject private var navigation: MarbleNavigationModel
    @Environment(\.colorScheme) private var colorScheme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.marbleMeasuresHistory) private var measuresHistory
    @State private var creatingThread = false
    @State private var threadError = false
    @State private var showsMessageActions = false
    var entry: MarbleEntry
    /// Who said it, when the row is on a shared timeline. Nil on one-to-one
    /// chats, which need no label.
    var speaker: MarbleSpeakerStyle? = nil
    var replyHue: Int = MarbleSpeakerStyle.defaultHue
    var reasoning: String? = nil
    var toolRows: [MarbleEntry] = []
    var timestamp: String? = nil
    var allowsThreads = true
    var usesThreadActionsDialog = false
    var chooseMemory: ((MarbleMemoryConsent) -> Void)? = nil

    private var isUser: Bool { entry.kind == "user" }
    private var exactTime: String {
        guard entry.at > 0 else { return "" }
        let date = Date(timeIntervalSince1970: Double(entry.at) / 1_000)
        return date.formatted(date: .abbreviated, time: .standard)
    }
    private var task: MarbleTask? {
        guard !entry.taskId.isEmpty else { return nil }
        return store.state.tasks.first { $0.id == entry.taskId }
    }

    var body: some View {
        let failure = MarbleTurnFailure.representsFailure(entry, status: task?.status)
        VStack(alignment: .leading, spacing: 6) {
            if let timestamp {
                Text(timestamp)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity)
            }
            if entry.kind == "assistant" || entry.kind == "pending" || failure {
                // Persisted reasoning replaces the live disclosure for this task.
                let thought = reasoning ?? store.reasoning(for: entry.taskId)
                let author = speaker?.name ?? store.agentName(for: entry.taskId) ?? "Marble"
                if !thought.isEmpty { MarbleReasoningRow(text: thought, speaker: author) }
                let tools = toolRows.compactMap(store.toolActivity(for:))
                if !tools.isEmpty {
                    MarbleToolTimeline(tools: tools)
                } else if entry.kind != "pending", entry.threadId == nil,
                          !store.hasTranscriptToolRows(in: entry.conversationId, taskId: entry.taskId),
                          !store.tools(for: entry.taskId).isEmpty {
                    MarbleToolTimeline(tools: store.tools(for: entry.taskId))
                }
            }
            entryContent(isFailure: failure)
            if entry.kind == "assistant" && !failure {
                MarbleKnowledgeCards(entry: entry, chooseMemory: chooseMemory)
            }
            if !failure && MarbleTurnFailure.isStoppedPartialReply(entry, taskStatus: task?.status) {
                MarbleFailedTurnRow(task: task, entry: entry, sender: speaker?.name)
            }
        }
        .background {
            if measuresHistory {
                GeometryReader { geometry in
                    Color.clear.preference(
                        key: MarbleHistoryRowPositions.self,
                        value: [entry.rowID: geometry.frame(in: .named(MarbleHistoryRowPositions.space)).minY]
                    )
                }
            }
        }
    }

    @ViewBuilder
    private func entryContent(isFailure: Bool) -> some View {
        if let sendId = entry.failedSendID {
            VStack(alignment: .trailing, spacing: 5) {
                message
                HStack(spacing: 8) {
                    Text("Couldn’t send")
                        .foregroundStyle(.secondary)
                    Button("Retry") { store.retrySend(sendId) }
                        .disabled(store.connectionPaused)
                }
                .font(.footnote)
            }
        } else if isFailure {
            MarbleFailedTurnRow(task: task, entry: entry, sender: speaker?.name)
        } else if allowsThreads, entry.serverID != nil, ["user", "assistant", "handoff"].contains(entry.kind) {
            VStack(alignment: .leading, spacing: 6) {
                if usesThreadActionsDialog {
                    message
                        .highPriorityGesture(LongPressGesture().onEnded { _ in
                            showsMessageActions = true
                        })
                        .confirmationDialog("Message actions", isPresented: $showsMessageActions) {
                            replyInThread
                            if !entry.text.isEmpty {
                                Button("Copy", systemImage: "doc.on.doc") {
                                    UIPasteboard.general.string = entry.text
                                }
                            }
                        } message: { Text(exactTime) }
                        .accessibilityAction(named: Text("Reply in thread")) {
                            Task { await openThread() }
                        }
                } else {
                    message.contextMenu {
                        Text(exactTime)
                        replyInThread
                    }
                }
                if let thread = store.threadSummary(for: entry) {
                    NavigationLink(value: ChatRoute.thread(thread.id)) {
                        Label(thread.replyLabel, systemImage: "text.bubble")
                            .font(.footnote.weight(.medium))
                    }
                    .buttonStyle(.borderless)
                }
            }
            .alert("Couldn’t open thread", isPresented: $threadError) {
                Button("OK", role: .cancel) {}
            } message: {
                Text("Try again in a moment.")
            }
        } else if entry.at > 0 && (isUser || entry.kind == "assistant") {
            message.contextMenu { Text(exactTime) }
        } else {
            message
        }
    }

    private var replyInThread: some View {
        Button("Reply in thread", systemImage: "arrowshape.turn.up.left") {
            Task { await openThread() }
        }
        .disabled(creatingThread)
    }

    private func openThread() async {
        if let thread = store.threadSummary(for: entry) {
            navigation.openThread(thread.id)
            return
        }
        guard !creatingThread else { return }
        creatingThread = true
        let startingNavigation = navigation.state
        defer { creatingThread = false }
        do {
            let thread = try await store.startThread(from: entry)
            guard navigation.state == startingNavigation else { return }
            navigation.openThread(thread.id)
        } catch is CancellationError {
        } catch {
            threadError = true
        }
    }

    @ViewBuilder
    private var message: some View {
        if isUser {
            Text(entry.text)
                .font(.body)
                .foregroundStyle(MarbleTheme.ink)
                .textSelection(.enabled)
                .padding(.horizontal, 13)
                .padding(.vertical, 8)
                .background(
                    MarbleTheme.wash,
                    in: UnevenRoundedRectangle(topLeadingRadius: 20, bottomLeadingRadius: 20,
                                               bottomTrailingRadius: 6, topTrailingRadius: 20)
                )
                .padding(.leading, 48)
                .frame(maxWidth: .infinity, alignment: .trailing)
        } else if entry.kind == "pending" {
            MarbleThinkingBubble(name: speaker?.name ?? store.agentName(for: entry.taskId) ?? "Marble",
                                 retrying: store.automaticRetries.contains(entry.taskId),
                                 status: store.workingLines[entry.taskId])
        } else if entry.kind == "handoff", let handoff = entry.handoff {
            HandoffLine(handoff: handoff)
        } else if entry.kind == "reasoning" {
            // Thinking stays tucked away — never inline with the reply.
            MarbleReasoningRow(text: entry.text, speaker: speaker?.name ?? store.agentName(for: entry.taskId) ?? "Marble")
        } else if let tool = store.toolActivity(for: entry) {
            MarbleToolRow(tool: tool, sender: speaker?.name ?? tool.sender)
        } else if entry.kind == "system" || entry.kind == "tool" {
            Text(entry.text)
                .font(.footnote)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .center)
                .multilineTextAlignment(.center)
        } else {
            replyBubble(entry.text)
        }
    }

    private func replyBubble(_ text: String) -> some View {
        let hue = speaker?.hue ?? replyHue
        return VStack(alignment: .leading, spacing: 6) {
            if let speaker {
                HStack(spacing: 7) {
                    MarbleAvatar(glyph: speaker.glyph, hue: hue, size: 20)
                    Text(speaker.name)
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(MarbleTheme.marble(hue: hue, brightness: colorScheme == .dark ? 1 : 0.38))
                }
                .accessibilityElement(children: .combine)
                .accessibilityLabel("\(speaker.name) said")
            }
            if !reduceMotion, let start = store.replyRevealStart(for: entry),
               Date().timeIntervalSince(start) < MarbleReplyReveal.duration {
                MarbleRevealedReply(text: text, startsAt: start)
            } else {
                MarbleMarkdownView(text: text).equatable().textSelection(.enabled)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 9)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            MarbleTheme.card,
            in: RoundedRectangle(cornerRadius: 14)
        )
    }
}

private struct MarbleRevealedReply: View {
    let text: String
    let startsAt: Date
    @State private var frame = 0

    var body: some View {
        let frames = MarbleReplyReveal.frames(for: text)
        let elapsed = max(0, Date().timeIntervalSince(startsAt))
        let visible = min(max(frame, Int(elapsed / MarbleReplyReveal.interval)), frames.count - 1)
        ZStack(alignment: .topLeading) {
            MarbleMarkdownView(text: frames[visible]).equatable()
                .textSelection(.enabled)
                .id(visible)
                .transition(.opacity)
        }
        .task(id: startsAt) {
            for index in (visible + 1)..<frames.count {
                let delay = startsAt.addingTimeInterval(Double(index) * MarbleReplyReveal.interval).timeIntervalSinceNow
                if delay > 0 { try? await Task.sleep(for: .seconds(delay)) }
                guard !Task.isCancelled else { return }
                withAnimation(.easeInOut(duration: 0.11)) { frame = index }
            }
        }
    }
}

struct MarbleToolRow: View {
    var tool: MarbleToolActivity
    var sender: String? = nil
    @State private var showsDetails = false

    var body: some View {
        Button {
            showsDetails = true
        } label: {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Image(systemName: tool.isWebSearch ? "magnifyingglass" : "wrench.and.screwdriver")
                Text([sender, tool.title].compactMap { $0 }.joined(separator: " · "))
                    .lineLimit(1)
                Spacer(minLength: 4)
                if let elapsed = tool.details?.elapsedMs { Text(MarbleToolTimeline.duration(elapsed)) }
                if tool.status == .failed {
                    Label("Failed", systemImage: "xmark.circle.fill").foregroundStyle(.red)
                } else {
                    Text(tool.status.rawValue)
                }
            }
            .frame(minHeight: 44)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .font(.caption)
        .foregroundStyle(.secondary)
        .accessibilityElement(children: .combine)
        .sheet(isPresented: $showsDetails) {
            NavigationStack {
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        detail("Command", tool.details?.command ?? tool.query ?? "")
                        detail("Output", tool.details?.output ?? "")
                    }
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .navigationTitle(tool.title)
                .navigationBarTitleDisplayMode(.inline)
                .toolbar { Button("Done") { showsDetails = false } }
            }
        }
    }

    private func detail(_ title: String, _ text: String) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(.headline)
            Text(text.isEmpty ? "Not recorded" : text)
                .font(.body.monospaced())
                .textSelection(.enabled)
        }
    }
}

struct MarbleToolTimeline: View {
    let tools: [MarbleToolActivity]

    static func duration(_ milliseconds: UInt64) -> String {
        milliseconds < 1000 ? "<1 s" : "\(milliseconds / 1000) s"
    }

    var body: some View {
        DisclosureGroup {
            VStack(spacing: 0) {
                ForEach(tools) { MarbleToolRow(tool: $0) }
            }
        } label: {
            let elapsed = tools.compactMap { $0.details?.elapsedMs }
            let summary = "Used \(tools.count) \(tools.count == 1 ? "tool" : "tools")"
            Text(elapsed.isEmpty ? summary : "\(summary) · \(Self.duration(elapsed.reduce(0, +)))")
                .frame(minHeight: 44)
        }
        .font(.footnote)
        .foregroundStyle(.secondary)
    }
}

struct MarbleReasoningRow: View {
    var text: String
    var speaker: String? = nil
    @State private var expanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button {
                // Make the disclosure height and the transcript's bottom
                // offset change in the same layout pass.
                expanded.toggle()
            } label: {
                HStack {
                    Text(speaker.map { "\($0) thought for a moment" } ?? "Thought for a moment")
                    Spacer()
                    Image(systemName: expanded ? "chevron.down" : "chevron.right")
                        .accessibilityHidden(true)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityValue(expanded ? "Expanded" : "Collapsed")
            if expanded {
                Text(text)
                    .font(.footnote)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .font(.caption)
        .foregroundStyle(.secondary)
    }
}

struct MarbleTurnTail: View {
    @EnvironmentObject private var store: MarbleStore
    var task: MarbleTask?
    var entries: [MarbleEntry]
    var showsLiveTools = false

    var body: some View {
        if let task {
            let rows = entries.filter { $0.taskId == task.id }
            let hasFailure = rows.contains { MarbleTurnFailure.representsFailure($0, status: task.status) }
            let hasAnswer = rows.contains { $0.kind == "assistant" }
            let hasPending = rows.contains { $0.kind == "pending" }
            if !hasFailure && !hasAnswer && !rows.contains(where: { $0.kind == "tool" })
                && !hasPending && (showsLiveTools || !task.isRunning) {
                let tools = store.tools(for: task.id)
                if !tools.isEmpty { MarbleToolTimeline(tools: tools) }
            }
            if !hasFailure && !hasAnswer && !hasPending
                && MarbleTurnFailure.message(for: task.status) != nil {
                MarbleFailedTurnRow(task: task)
            }
        }
    }
}

private struct MarbleFailedTurnRow: View {
    @EnvironmentObject private var store: MarbleStore
    var task: MarbleTask?
    var entry: MarbleEntry? = nil
    var sender: String? = nil

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Label(message, systemImage: "exclamationmark.circle")
                .foregroundStyle(.secondary)
            if let taskId = MarbleTurnFailure.retryID(entry: entry, task: task) {
                Button("Retry") { Task { await store.retry(taskId: taskId) } }
                    .buttonStyle(.glass)
                    .frame(minHeight: 44)
                    .disabled(store.retryingTurns.contains(taskId))
                if store.retryFailures.contains(taskId) {
                    Text("Couldn’t retry. Try again in a moment.")
                        .foregroundStyle(.secondary)
                }
            }
            if let diagnostics = entry?.failure?.diagnostics, !diagnostics.isEmpty {
                DisclosureGroup("Details") {
                    Text(diagnostics)
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
            }
        }
        .font(.footnote)
    }

    private var message: String {
        let name = sender ?? store.agentName(for: entry?.taskId ?? task?.id ?? "") ?? "Marble"
        let status = entry?.cancelled == true ? "cancelled" : task?.status
        return status == "cancelled" ? "\(name) stopped before finishing" : "\(name) couldn’t finish this reply."
    }
}

/// The message box: grows with the text up to six lines, then scrolls
/// inside. Return adds a line; the arrow sends.
struct ComposerBar: View {
    @EnvironmentObject private var store: MarbleStore
    @State private var submitting = false
    @Binding var draft: String
    var isWorking: Bool
    var isDictating: Bool = false
    var placeholder: String = "Message"
    var workingPlaceholder: String = "Add to what Marble is doing"
    /// False where the host cannot take a correction mid-turn.
    var canSteer: Bool = true
    var steerReady: Bool = true
    var onSend: (String) -> Bool
    /// Correct the running turn instead of starting a new one.
    var onSteer: (String) async -> Bool = { _ in false }
    var onStop: () -> Void
    var onDictate: (() -> Void)? = nil
    @FocusState private var focused: Bool

    private var action: MarbleSteer.Action {
        MarbleSteer.action(isWorking: isWorking, canSteer: canSteer)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            MarbleConnectionNotice()
                .padding(.horizontal, 16)
            if action == .stopOnly {
                Text(MarbleSteer.unsupportedNote)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .padding(.horizontal, 16)
                    .padding(.top, 8)
            }
            field
        }
    }

    private var field: some View {
        HStack(alignment: .bottom, spacing: 8) {
            if let onDictate {
                Button(action: onDictate) {
                    Image(systemName: isDictating ? "waveform" : "mic")
                        .font(.system(size: 17, weight: .medium))
                        .foregroundStyle(isDictating ? .red : .secondary)
                        .frame(width: 44, height: 44)
                        .contentShape(Rectangle())
                }
                .accessibilityLabel(isDictating ? "Stop dictation" : "Dictate")
            }

            TextField(
                action == .steer ? workingPlaceholder : placeholder,
                text: $draft,
                axis: .vertical
            )
            .lineLimit(1...6)
            .focused($focused)
            .padding(.horizontal, 14)
            .padding(.vertical, 9)
            .accessibilityIdentifier("composer.field")

            if action == .send || (action == .steer && !trimmed.isEmpty) {
                Button { submit() } label: {
                    Image(systemName: "arrow.up.circle.fill")
                        .font(.system(size: 32))
                        .symbolRenderingMode(.palette)
                        .foregroundStyle(MarbleTheme.card, trimmed.isEmpty ? MarbleTheme.muted : MarbleTheme.tint)
                        .frame(width: 44, height: 44)
                        .contentShape(Rectangle())
                }
                .disabled(trimmed.isEmpty || submitting || store.connectionPaused || store.needsToken
                          || (action == .steer && !steerReady))
                .accessibilityLabel(action == .steer ? "Steer" : "Send")
            }
            if isWorking {
                Button(action: onStop) {
                    Image(systemName: "stop.circle.fill")
                        .font(.system(size: 32))
                        .foregroundStyle(.red)
                        .frame(width: 44, height: 44)
                        .contentShape(Rectangle())
                }
                .accessibilityLabel("Stop")
            }
        }
        .padding(.horizontal, 6)
        .padding(.vertical, 5)
        .marbleFloating()
        .padding(.horizontal, 16)
        .padding(.vertical, 6)
    }

    private func submit() {
        guard !trimmed.isEmpty, !submitting, !store.connectionPaused else { return }
        let sent = draft
        guard action == .steer else {
            if onSend(sent) { draft = "" }
            return
        }
        submitting = true
        Task {
            defer { submitting = false }
            let accepted = await onSteer(sent)
            draft = MarbleDraft.draft(after: sent, current: draft, accepted: accepted)
        }
    }

    private var trimmed: String {
        draft.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

struct MarbleTranscriptEnd: View {
    static let rowID = "marble-timeline-end"
    var bottomPadding: CGFloat

    var body: some View {
        Color.clear
            .frame(height: bottomPadding)
            .accessibilityHidden(true)
            .id(Self.rowID)
    }
}

typealias MarbleOlderHistoryLoader = (@escaping () -> Void) async -> Void

private struct MarbleHistoryRowPositions: PreferenceKey {
    static let space = "marble-history-content"
    static let defaultValue: [String: CGFloat] = [:]

    static func reduce(value: inout [String: CGFloat], nextValue: () -> [String: CGFloat]) {
        value.merge(nextValue(), uniquingKeysWith: { _, latest in latest })
    }
}

private extension EnvironmentValues {
    @Entry var marbleMeasuresHistory = true
}

extension View {
    func marbleHistoryScrollTargets() -> some View {
        scrollTargetLayout().coordinateSpace(name: MarbleHistoryRowPositions.space)
    }

    func marbleComposerInset(@ViewBuilder content: () -> some View) -> some View {
        marbleChatBar(edge: .bottom, content: content)
    }

    func marbleFollowsLatest(hasMessages: Bool, searchJump: UUID? = nil, outgoing: UUID? = nil,
                             startsAtLatest: Bool = false,
                             loadOlder: MarbleOlderHistoryLoader? = nil) -> some View {
        modifier(MarbleFollowLatest(
            hasMessages: hasMessages, searchJump: searchJump, outgoing: outgoing,
            startsAtLatest: startsAtLatest, loadOlder: loadOlder
        ))
    }
}

private struct MarbleFollowLatest: ViewModifier {
    let hasMessages: Bool
    let searchJump: UUID?
    let outgoing: UUID?
    let startsAtLatest: Bool
    let loadOlder: MarbleOlderHistoryLoader?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var position: ScrollPosition
    @State private var follow = MarbleScrollFollow()
    @State private var phase: ScrollPhase = .idle
    @State private var beyondViewport = false
    @State private var positionedHistory = false
    @State private var measurement = PrependMeasurement()

    init(hasMessages: Bool, searchJump: UUID?, outgoing: UUID?, startsAtLatest: Bool,
         loadOlder: MarbleOlderHistoryLoader?) {
        self.hasMessages = hasMessages
        self.searchJump = searchJump
        self.outgoing = outgoing
        self.startsAtLatest = startsAtLatest
        self.loadOlder = loadOlder
        _position = State(initialValue: hasMessages && startsAtLatest
            ? ScrollPosition(idType: String.self, edge: .bottom) : ScrollPosition(idType: String.self))
    }

    // ScrollPosition retains the top row during a prepend but aligns it flush
    // to the top. Preserve its offset as well, including a partially read row.
    private final class PrependMeasurement {
        var offset: CGFloat = 0
        var rowPositions: [String: CGFloat] = [:]
        var anchor: (id: String, rowY: CGFloat, offset: CGFloat)?
        var followCorrection: Task<Void, Never>?
    }

    private struct Layout: Equatable {
        let bottom: CGFloat
        let viewport: CGFloat
        let offset: CGFloat
        let topInset: CGFloat

        init(_ geometry: ScrollGeometry) {
            bottom = geometry.contentSize.height + geometry.contentInsets.bottom
            viewport = geometry.containerSize.height
            offset = geometry.contentOffset.y
            topInset = geometry.contentInsets.top
        }

        var distanceFromBottom: Double {
            Double(max(-topInset, bottom - viewport) - offset)
        }

        var distanceFromTop: Double {
            Double(offset + topInset)
        }
    }

    func body(content: Content) -> some View {
        content
            .marbleChatScrollEdges()
            .environment(\.marbleMeasuresHistory, !startsAtLatest || !follow.following)
            .defaultScrollAnchor(searchJump == nil ? .bottom : .top, for: .initialOffset)
            .defaultScrollAnchor(startsAtLatest ? .bottom : nil, for: .alignment)
            .defaultScrollAnchor(startsAtLatest && follow.following ? .bottom : nil, for: .sizeChanges)
            .scrollPosition($position, anchor: .top)
            .onScrollPhaseChange { oldPhase, newPhase, context in
                phase = newPhase
                if newPhase.isUserScrolling {
                    measurement.anchor = nil
                    measurement.followCorrection?.cancel()
                    measurement.followCorrection = nil
                }
                guard oldPhase.isUserScrolling, newPhase == .idle else { return }
                let layout = Layout(context.geometry)
                let distance = layout.distanceFromBottom
                updateFollow(distance: distance, userMoved: oldPhase != .tracking)
                if hasMessages && follow.following && abs(distance) > 1 {
                    correctBottomAfterLayout()
                }
                if hasMessages, oldPhase != .tracking, let loadOlder, layout.distanceFromTop < 180 {
                    Task {
                        await loadOlder {
                            guard !follow.following, let id = position.viewID(type: String.self),
                                  let rowY = measurement.rowPositions[id] else { return }
                            measurement.anchor = (id, rowY, measurement.offset)
                        }
                    }
                }
            }
            .onScrollGeometryChange(for: Layout.self) { geometry in
                Layout(geometry)
            } action: { old, new in
                // ScrollPosition's y coordinate starts after the top inset.
                measurement.offset = new.offset + new.topInset
                let resized = old.viewport != new.viewport || old.bottom != new.bottom || old.topInset != new.topInset
                let distance = new.distanceFromBottom
                beyondViewport = distance > new.viewport && new.viewport > 0
                updateFollow(
                    distance: distance,
                    userMoved: phase.isUserScrolling && old.offset != new.offset && !resized
                )
                guard hasMessages, new.viewport > 0, new.bottom > 0 else { return }
                let initialLayout = !positionedHistory
                positionedHistory = true
                if follow.following && !phase.isUserScrolling && abs(distance) > 1
                    && (resized || (initialLayout && searchJump == nil)) {
                    correctBottomAfterLayout()
                }
            }
            .onPreferenceChange(MarbleHistoryRowPositions.self) { rows in
                measurement.rowPositions = rows
                guard let anchor = measurement.anchor, let rowY = rows[anchor.id],
                      rowY > anchor.rowY else { return }
                measurement.anchor = nil
                position.scrollTo(y: anchor.offset + rowY - anchor.rowY)
            }
            .task(id: hasMessages) {
                let loaded = hasMessages
                guard loaded, !positionedHistory else { return }
                // An already-sized scroll view may replace its loading rows
                // without changing geometry. Wait for that content transaction.
                await Task.yield()
                guard !Task.isCancelled, !positionedHistory else { return }
                positionedHistory = true
                if searchJump == nil && follow.following { scrollToEnd() }
            }
            .onChange(of: searchJump, initial: true) { _, target in
                if target != nil {
                    follow.pauseForSearchJump()
                }
            }
            .onChange(of: outgoing) { _, send in
                if send != nil && follow.following && !phase.isUserScrolling {
                    jumpToLatest()
                }
            }
            .onDisappear {
                measurement.followCorrection?.cancel()
                measurement.followCorrection = nil
            }
            .overlay(alignment: .bottomTrailing) {
                if follow.showJumpButton && beyondViewport {
                    Button {
                        jumpToLatest()
                    } label: {
                        Label("Jump to latest", systemImage: "arrow.down")
                            .labelStyle(.iconOnly)
                            .frame(width: 44, height: 44)
                            .marbleFloating(in: Circle())
                    }
                    .accessibilityLabel("Jump to latest")
                    .padding(.trailing, 16)
                    .padding(.bottom, 8)
                }
            }
    }

    private func jumpToLatest() {
        follow.jumpToLatest()
        withAnimation(reduceMotion ? nil : .easeOut(duration: 0.22)) {
            scrollToEnd()
        }
    }

    private func updateFollow(distance: Double, userMoved: Bool) {
        var updated = follow
        if userMoved {
            updated.userMoved(distanceFromBottom: distance)
        } else {
            updated.layoutMoved(distanceFromBottom: distance)
        }
        if updated != follow { follow = updated }
    }

    private func scrollToEnd() {
        position.scrollTo(edge: .bottom)
    }

    private func correctBottomAfterLayout() {
        guard measurement.followCorrection == nil else { return }
        // Writing to ScrollPosition inside the lazy stack's geometry update
        // can trigger another layout before the first transaction completes.
        measurement.followCorrection = Task { @MainActor in
            await Task.yield()
            guard !Task.isCancelled else { return }
            measurement.followCorrection = nil
            if follow.following && !phase.isUserScrolling { scrollToEnd() }
        }
    }
}

private extension ScrollPhase {
    var isUserScrolling: Bool {
        switch self {
        case .tracking, .interacting, .decelerating: true
        default: false
        }
    }
}

/// A governed call asking for a decision, rendered as a grouped inset block.
struct ApprovalCard: View {
    var approval: MarbleApproval
    /// `remember` widens one allow into a standing grant for this directory.
    var onDecision: (_ allow: Bool, _ remember: Bool) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(MarbleDecisionWords.action(tool: approval.tool))
                .font(.headline)
            Text(approval.argumentsSummary)
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
                .lineLimit(3)
            if let conflict = approval.conflict, !conflict.isEmpty {
                Label(conflict, systemImage: "exclamationmark.triangle")
                    .font(.caption)
                    .foregroundStyle(.orange)
                    .fixedSize(horizontal: false, vertical: true)
            }
            HStack(spacing: 10) {
                Button("Deny", role: .destructive) { onDecision(false, false) }
                    .buttonStyle(.bordered)
                Spacer()
                Button("Always allow") { onDecision(true, true) }
                    .buttonStyle(.bordered)
                Button("Allow") { onDecision(true, false) }
                    .buttonStyle(.borderedProminent)
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            Color(.secondarySystemGroupedBackground),
            in: RoundedRectangle(cornerRadius: 12, style: .continuous)
        )
    }
}
