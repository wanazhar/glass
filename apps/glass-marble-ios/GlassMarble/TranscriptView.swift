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
                            description: Text("Ask Marble to read, change, or explain something in this workspace.")
                        )
                    }
                    let transcript = MarbleReasoningAssociation(entries)
                    ForEach(transcript.standalone, id: \.rowID) { entry in
                        MarbleEntryView(entry: entry, reasoning: transcript.attached[entry.id])
                            .transition(.opacity.combined(with: .move(edge: .bottom)))
                    }
                    if !store.liveReasoning.isEmpty && !entries.contains(where: {
                        $0.taskId == store.activeTaskId && ["reasoning", "assistant"].contains($0.kind)
                    }) {
                        MarbleEntryView(entry: .preview(kind: "reasoning", text: store.liveReasoning))
                    }
                    ForEach(store.liveTools) { tool in
                        MarbleToolRow(tool: tool)
                    }
                    let working = store.isWorking && !entries.contains {
                        $0.kind == "assistant" && $0.taskId == store.activeTaskId
                    }
                    if store.liveText.isEmpty && (!presentation.responders.isEmpty || working
                        || store.isAwaitingReply(in: conversationId, entries: entries)) {
                        MarbleThinkingBubble()
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
    @State private var creatingThread = false
    @State private var threadError = false
    var entry: MarbleEntry
    /// Who said it, when the row is on a shared timeline. Nil on one-to-one
    /// chats, which need no label.
    var speaker: MarbleSpeakerStyle? = nil
    var replyHue: Int = MarbleSpeakerStyle.defaultHue
    var reasoning: String? = nil
    var allowsThreads = true

    private var isUser: Bool { entry.kind == "user" }
    private var task: MarbleTask? {
        guard !entry.taskId.isEmpty else { return nil }
        return store.state.tasks.first { $0.id == entry.taskId }
    }

    var body: some View {
        let failure = MarbleTurnFailure.representsFailure(entry, status: task?.status)
        VStack(alignment: .leading, spacing: 6) {
            if entry.kind == "assistant" || entry.kind == "pending" || failure {
                let thought = reasoning ?? store.reasoning(for: entry.taskId)
                if !thought.isEmpty { MarbleReasoningRow(text: thought, speaker: speaker?.name) }
                if entry.kind != "pending", entry.threadId == nil,
                   !store.hasTranscriptToolRows(in: entry.conversationId, taskId: entry.taskId) {
                    ForEach(store.tools(for: entry.taskId)) { MarbleToolRow(tool: $0) }
                }
            }
            entryContent(isFailure: failure)
            if !failure && MarbleTurnFailure.isStoppedPartialReply(entry, taskStatus: task?.status) {
                MarbleFailedTurnRow(task: task, entry: entry)
            }
        }
        .background {
            GeometryReader { geometry in
                Color.clear.preference(
                    key: MarbleHistoryRowPositions.self,
                    value: [entry.rowID: geometry.frame(in: .named(MarbleHistoryRowPositions.space)).minY]
                )
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
                message
                    .contextMenu {
                        Button("Reply in thread", systemImage: "arrowshape.turn.up.left") {
                            Task { await openThread() }
                        }
                        .disabled(creatingThread)
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
        } else {
            message
        }
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
                .foregroundStyle(.white)
                .textSelection(.enabled)
                .padding(.horizontal, 13)
                .padding(.vertical, 8)
                .background(
                    Color.accentColor,
                    in: RoundedRectangle(cornerRadius: 18, style: .continuous)
                )
                .padding(.leading, 48)
                .frame(maxWidth: .infinity, alignment: .trailing)
        } else if entry.kind == "pending" {
            let retrying = store.automaticRetries.contains(entry.taskId)
            if let text = store.pendingReplyText(for: entry.taskId), !retrying {
                replyBubble(text)
            } else {
                MarbleThinkingBubble(name: speaker?.name ?? "Marble", retrying: retrying)
            }
        } else if entry.kind == "handoff", let handoff = entry.handoff {
            HandoffLine(handoff: handoff)
        } else if entry.kind == "reasoning" {
            // Thinking stays tucked away — never inline with the reply.
            MarbleReasoningRow(text: entry.text)
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
            let blocks = store.retractedReplyBlocks(for: entry)
            if blocks.isEmpty {
                MarbleMarkdownView(text: text).equatable().textSelection(.enabled)
            } else {
                ZStack(alignment: .topLeading) {
                    ForEach(blocks.indices, id: \.self) { index in
                        MarbleMarkdownView(text: blocks[index]).equatable()
                            .hidden()
                            .accessibilityHidden(true)
                    }
                    MarbleMarkdownView(text: text).equatable().textSelection(.enabled)
                }
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 9)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            MarbleTheme.marble(hue: hue).opacity(colorScheme == .dark ? 0.17 : 0.09),
            in: RoundedRectangle(cornerRadius: 14)
        )
    }
}

struct MarbleToolRow: View {
    var tool: MarbleToolActivity
    var sender: String? = nil

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Image(systemName: tool.isWebSearch ? "magnifyingglass" : "wrench.and.screwdriver")
            Text([sender, tool.title, tool.query].compactMap { $0 }.joined(separator: " · "))
                .lineLimit(1)
            Spacer(minLength: 4)
            Text(tool.status.rawValue)
        }
        .font(.caption)
        .foregroundStyle(.secondary)
        .accessibilityElement(children: .combine)
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
            if !hasFailure && !hasAnswer && !rows.contains(where: { $0.kind == "tool" })
                && (showsLiveTools || !task.isRunning) {
                ForEach(store.tools(for: task.id)) { MarbleToolRow(tool: $0) }
            }
            if !hasFailure && !hasAnswer && !rows.contains(where: { $0.kind == "pending" })
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
            if let sender { Text(sender).fontWeight(.semibold) }
            Label(message, systemImage: "exclamationmark.circle")
                .foregroundStyle(.secondary)
            if let taskId = MarbleTurnFailure.retryID(entry: entry, task: task) {
                Button("Retry") { Task { await store.retry(taskId: taskId) } }
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
        if let entry, entry.kind == "failure", !entry.text.isEmpty { return entry.text }
        let status = entry?.cancelled == true ? "cancelled" : task?.status
        return status.flatMap(MarbleTurnFailure.message(for:)) ?? "Marble couldn’t finish this reply."
    }
}

/// The message box: grows with the text up to six lines, then scrolls
/// inside. Return adds a line; the arrow sends. While the keyboard is up a
/// button beside the box hides it.
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
            if store.connectionPaused {
                HStack {
                    Text("Not connected")
                        .foregroundStyle(.secondary)
                    Button("Connect") { store.reconnect(force: true) }
                        .buttonStyle(.borderless)
                }
                .font(.footnote)
                .padding(.horizontal, 16)
                .padding(.top, 8)
            } else if action == .stopOnly {
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
            if focused {
                Button { focused = false } label: {
                    Image(systemName: "keyboard.chevron.compact.down")
                        .font(.system(size: 17, weight: .medium))
                        .foregroundStyle(.secondary)
                        .frame(width: 36, height: 38)
                        .contentShape(Rectangle())
                }
                .accessibilityLabel("Hide keyboard")
            } else if let onDictate {
                Button(action: onDictate) {
                    Image(systemName: isDictating ? "waveform" : "mic")
                        .font(.system(size: 17, weight: .medium))
                        .foregroundStyle(isDictating ? .red : .secondary)
                        .frame(width: 36, height: 38)
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
            .background(
                Color(.systemBackground),
                in: RoundedRectangle(cornerRadius: 19, style: .continuous)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 19, style: .continuous)
                    .strokeBorder(Color(.separator), lineWidth: 0.5)
            )
            .accessibilityIdentifier("composer.field")

            if action == .send || (action == .steer && !trimmed.isEmpty) {
                Button { submit() } label: {
                    Image(systemName: "arrow.up.circle.fill")
                        .font(.system(size: 32))
                        .symbolRenderingMode(.palette)
                        .foregroundStyle(.white, trimmed.isEmpty ? Color(.systemGray3) : Color.accentColor)
                        .frame(width: 38, height: 38)
                        .contentShape(Rectangle())
                }
                .disabled(trimmed.isEmpty || submitting || store.connectionPaused)
                .accessibilityLabel(action == .steer ? "Steer" : "Send")
            }
            if isWorking {
                Button(action: onStop) {
                    Image(systemName: "stop.circle.fill")
                        .font(.system(size: 32))
                        .foregroundStyle(.red)
                        .frame(width: 38, height: 38)
                        .contentShape(Rectangle())
                }
                .accessibilityLabel("Stop")
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
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

extension View {
    func marbleHistoryScrollTargets() -> some View {
        scrollTargetLayout().coordinateSpace(name: MarbleHistoryRowPositions.space)
    }

    func marbleComposerInset(@ViewBuilder content: () -> some View) -> some View {
        safeAreaInset(edge: .bottom, spacing: 0, content: content)
    }

    /// A solid bar behind the composer so messages never show through it,
    /// reaching down under the home indicator and the keyboard.
    func marbleComposerBackground() -> some View {
        background {
            Rectangle()
                .fill(.bar)
                .overlay(alignment: .top) { Divider() }
                .ignoresSafeArea(edges: .bottom)
        }
    }

    func marbleFollowsLatest(hasMessages: Bool, searchJump: UUID? = nil, outgoing: UUID? = nil,
                             startAtCachedLatest: Bool = false,
                             loadOlder: MarbleOlderHistoryLoader? = nil) -> some View {
        modifier(MarbleFollowLatest(
            hasMessages: hasMessages, searchJump: searchJump, outgoing: outgoing,
            startAtCachedLatest: startAtCachedLatest, loadOlder: loadOlder
        ))
    }
}

private struct MarbleFollowLatest: ViewModifier {
    let hasMessages: Bool
    let searchJump: UUID?
    let outgoing: UUID?
    let startAtCachedLatest: Bool
    let loadOlder: MarbleOlderHistoryLoader?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var position: ScrollPosition
    @State private var follow = MarbleScrollFollow()
    @State private var phase: ScrollPhase = .idle
    @State private var positionedHistory = false
    @State private var measurement = PrependMeasurement()

    init(hasMessages: Bool, searchJump: UUID?, outgoing: UUID?, startAtCachedLatest: Bool,
         loadOlder: MarbleOlderHistoryLoader?) {
        self.hasMessages = hasMessages
        self.searchJump = searchJump
        self.outgoing = outgoing
        self.startAtCachedLatest = startAtCachedLatest
        self.loadOlder = loadOlder
        // Give the first layout the bottom position with the cached rows
        // already present. Uncached screens still reposition as rows arrive.
        _position = State(initialValue: hasMessages && startAtCachedLatest
            ? ScrollPosition(id: MarbleTranscriptEnd.rowID, anchor: .bottom) : ScrollPosition(idType: String.self))
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
    }

    func body(content: Content) -> some View {
        content
            .defaultScrollAnchor(searchJump == nil ? .bottom : .top, for: .initialOffset)
            .defaultScrollAnchor(
                startAtCachedLatest && follow.following && !phase.isUserScrolling ? .bottom : nil,
                for: .sizeChanges
            )
            .scrollPosition($position, anchor: .top)
            .onScrollPhaseChange { oldPhase, newPhase, context in
                phase = newPhase
                if newPhase.isUserScrolling {
                    measurement.anchor = nil
                    measurement.followCorrection?.cancel()
                    measurement.followCorrection = nil
                }
                guard oldPhase.isUserScrolling, newPhase == .idle else { return }
                let distance = Layout(context.geometry).distanceFromBottom
                updateFollow(distance: distance, userMoved: oldPhase != .tracking)
                if hasMessages && follow.following && abs(distance) > 1 {
                    correctBottomAfterLayout()
                }
                if hasMessages, oldPhase != .tracking, let loadOlder,
                   context.geometry.contentOffset.y + context.geometry.contentInsets.top < 180 {
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
                updateFollow(
                    distance: new.distanceFromBottom,
                    userMoved: phase.isUserScrolling && old.offset != new.offset && !resized
                )
                guard hasMessages, new.viewport > 0, new.bottom > 0 else { return }
                let initialLayout = !positionedHistory
                positionedHistory = true
                if follow.following && !phase.isUserScrolling && abs(new.distanceFromBottom) > 1
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
                if follow.showJumpButton {
                    Button {
                        jumpToLatest()
                    } label: {
                        Label("Jump to latest", systemImage: "arrow.down")
                            .labelStyle(.iconOnly)
                            .padding(10)
                            .marbleFloating()
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
