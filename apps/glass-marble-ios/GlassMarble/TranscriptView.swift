import SwiftUI

struct TranscriptView: View {
    @EnvironmentObject private var store: MarbleStore

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 12) {
                    if !store.loaded {
                        placeholderRows
                    } else if store.connection == .offline
                        && store.transcript.isEmpty
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
                    } else if store.transcript.isEmpty
                        && store.liveText.isEmpty
                        && store.liveReasoning.isEmpty
                        && store.liveTools.isEmpty {
                        ContentUnavailableView(
                            "No messages yet",
                            systemImage: "text.bubble",
                            description: Text("Ask Marble to read, change, or explain something in this workspace.")
                        )
                    }
                    let transcript = MarbleReasoningAssociation(store.transcript)
                    ForEach(transcript.standalone) { entry in
                        MarbleEntryView(entry: entry, reasoning: transcript.attached[entry.id])
                            .transition(.opacity.combined(with: .move(edge: .bottom)))
                    }
                    if !store.liveReasoning.isEmpty {
                        MarbleEntryView(entry: .preview(kind: "reasoning", text: store.liveReasoning))
                    }
                    ForEach(store.liveTools) { tool in
                        MarbleToolRow(tool: tool)
                    }
                    if store.liveText.isEmpty && (store.isWorking
                        || store.isAwaitingReply(in: store.selectedConversationId, entries: store.transcript)) {
                        MarbleThinkingBubble()
                    }
                    if !store.liveText.isEmpty {
                        MarbleEntryView(entry: .preview(kind: "assistant", text: store.liveText))
                    }
                    MarbleTurnTail(task: store.latestTurn(in: store.selectedConversationId), entries: store.transcript)
                    Color.clear
                        .frame(height: 1)
                        .id(Self.bottomAnchor)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 14)
                // Cap the measure on iPad and Mac so a reply never becomes a
                // single 1,000-character line.
                .frame(maxWidth: 820, alignment: .leading)
                .frame(maxWidth: .infinity)
            }
            .scrollDismissesKeyboard(.interactively)
            .marbleFollowsLatest(proxy: proxy, anchor: Self.bottomAnchor)
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
    }

    private static let bottomAnchor = "marble-bottom"

}

/// One transcript entry. The person's own messages sit in a blue bubble on
/// the right; replies read as full-width Markdown under a clear speaker line
/// with the marble's avatar and name in its colour.
struct MarbleEntryView: View {
    @EnvironmentObject private var store: MarbleStore
    @EnvironmentObject private var navigation: MarbleNavigationModel
    @State private var creatingThread = false
    @State private var threadError = false
    var entry: MarbleEntry
    /// Who said it, when the row is on a shared timeline. Nil on one-to-one
    /// chats, which need no label.
    var speaker: MarbleSpeakerStyle? = nil
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
            if entry.kind == "assistant" || failure {
                let thought = reasoning ?? store.reasoning(for: entry.taskId)
                if !thought.isEmpty { MarbleReasoningRow(text: thought, speaker: speaker?.name) }
                if entry.threadId == nil && !store.hasTranscriptToolRows(in: entry.conversationId, taskId: entry.taskId) {
                    ForEach(store.tools(for: entry.taskId)) { MarbleToolRow(tool: $0) }
                }
            }
            entryContent(isFailure: failure)
        }
    }

    @ViewBuilder
    private func entryContent(isFailure: Bool) -> some View {
        if isFailure {
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
        } else if entry.kind == "handoff", let handoff = entry.handoff {
            HandoffLine(handoff: handoff)
        } else if entry.kind == "reasoning" {
            // Thinking stays tucked away — never inline with the reply.
            MarbleReasoningRow(text: entry.text)
        } else if let tool = MarbleToolActivity.fromTranscript(entry) {
            MarbleToolRow(tool: tool)
        } else if entry.kind == "system" || entry.kind == "tool" {
            Text(entry.text)
                .font(.footnote)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .center)
                .multilineTextAlignment(.center)
        } else {
            VStack(alignment: .leading, spacing: 6) {
                if let speaker {
                    HStack(spacing: 7) {
                        MarbleAvatar(glyph: speaker.glyph, hue: speaker.hue ?? MarbleSpeakerStyle.defaultHue, size: 20)
                        Text(speaker.name)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(MarbleTheme.marble(hue: speaker.hue))
                    }
                    .accessibilityElement(children: .combine)
                    .accessibilityLabel("\(speaker.name) said")
                }
                MarbleMarkdownView(text: entry.text)
                    .textSelection(.enabled)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

struct MarbleToolRow: View {
    var tool: MarbleToolActivity

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Image(systemName: tool.isWebSearch ? "magnifyingglass" : "wrench.and.screwdriver")
            Text(tool.title + (tool.query.map { " · " + $0 } ?? ""))
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

    var body: some View {
        DisclosureGroup(speaker.map { "\($0) thought for a moment" } ?? "Thought for a moment") {
            Text(text)
                .font(.footnote)
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
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
            if !hasFailure && MarbleTurnFailure.message(for: task.status) != nil {
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
        return task.flatMap { MarbleTurnFailure.message(for: $0.status) } ?? "Marble couldn’t finish this reply."
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
    var onSend: (String) async -> Bool
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
        let operation = action == .steer ? onSteer : onSend
        submitting = true
        Task {
            defer { submitting = false }
            let accepted = await operation(sent)
            draft = MarbleDraft.draft(after: sent, current: draft, accepted: accepted)
        }
    }

    private var trimmed: String {
        draft.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

extension View {
    @ViewBuilder
    func marbleComposerInset(@ViewBuilder content: () -> some View) -> some View {
        if #available(iOS 26.0, *) {
            safeAreaBar(edge: .bottom, spacing: 0, content: content)
        } else {
            safeAreaInset(edge: .bottom, spacing: 0, content: content)
        }
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

    func marbleFollowsLatest(proxy: ScrollViewProxy, anchor: String, searchJump: UUID? = nil) -> some View {
        modifier(MarbleFollowLatest(proxy: proxy, anchor: anchor, searchJump: searchJump))
    }
}

private struct MarbleFollowLatest: ViewModifier {
    let proxy: ScrollViewProxy
    let anchor: String
    let searchJump: UUID?
    @State private var follow = MarbleScrollFollow()
    @State private var phase: ScrollPhase = .idle

    private struct Layout: Equatable {
        let bottom: CGFloat
        let viewport: CGFloat
        let offset: CGFloat

        var distanceFromBottom: Double { Double(bottom - viewport - offset) }
    }

    func body(content: Content) -> some View {
        content
            .defaultScrollAnchor(searchJump == nil ? .bottom : .top, for: .initialOffset)
            .onScrollPhaseChange { _, newPhase in
                phase = newPhase
                if newPhase == .idle { scrollIfFollowing() }
            }
            .onScrollGeometryChange(for: Layout.self) { geometry in
                Layout(
                    bottom: geometry.contentSize.height + geometry.contentInsets.bottom,
                    viewport: geometry.containerSize.height,
                    offset: geometry.contentOffset.y
                )
            } action: { old, new in
                if old.bottom != new.bottom || old.viewport != new.viewport {
                    // SwiftUI now knows the row's rendered height and the
                    // space left by the composer, including keyboard changes.
                    scrollIfFollowing()
                } else if old.offset != new.offset && (phase == .interacting || phase == .decelerating) {
                    follow.userMoved(distanceFromBottom: new.distanceFromBottom)
                }
            }
            .onChange(of: searchJump, initial: true) { _, target in
                if target != nil {
                    follow.pauseForSearchJump()
                } else {
                    follow.jumpToLatest()
                    scrollIfFollowing()
                }
            }
            .overlay(alignment: .bottomTrailing) {
                if follow.showJumpButton {
                    Button {
                        follow.jumpToLatest()
                        scrollIfFollowing()
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

    private func scrollIfFollowing() {
        guard follow.following, phase == .idle || phase == .animating else { return }
        DispatchQueue.main.async {
            guard follow.following, phase == .idle || phase == .animating else { return }
            proxy.scrollTo(anchor, anchor: .bottom)
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
