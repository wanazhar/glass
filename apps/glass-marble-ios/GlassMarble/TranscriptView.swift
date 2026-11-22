import SwiftUI

struct TranscriptView: View {
    @EnvironmentObject private var store: MarbleStore
    @State private var follow = MarbleScrollFollow()
    /// True only while the person's finger is driving the scroll view;
    /// content growth alone must never count as "scrolled away".
    @State private var userDriven = false

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
                    ForEach(store.transcript) { entry in
                        MarbleEntryView(entry: entry)
                            .transition(.opacity.combined(with: .move(edge: .bottom)))
                    }
                    if !store.liveReasoning.isEmpty {
                        MarbleEntryView(entry: .preview(kind: "reasoning", text: store.liveReasoning))
                    }
                    ForEach(store.liveTools) { tool in
                        MarbleToolRow(tool: tool)
                    }
                    if store.isWorking
                        && store.liveText.isEmpty
                        && store.liveReasoning.isEmpty
                        && store.liveTools.isEmpty {
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
            .onScrollPhaseChange { _, phase in
                userDriven = phase == .tracking || phase == .interacting || phase == .decelerating
            }
            .onScrollGeometryChange(for: Double.self) { geometry in
                geometry.contentSize.height - geometry.contentOffset.y - geometry.containerSize.height
            } action: { _, distance in
                if userDriven { follow.userMoved(distanceFromBottom: distance) }
            }
            .onChange(of: store.transcript.count) { _, _ in scrollIfFollowing(proxy) }
            .onChange(of: store.liveText) { _, _ in scrollIfFollowing(proxy) }
            .onChange(of: store.liveTools.count) { _, _ in scrollIfFollowing(proxy) }
            .onChange(of: store.loaded) { _, _ in scrollIfFollowing(proxy) }
            .onAppear {
                // First appearance jumps straight to the end, no animation.
                if follow.initialFollow(now: now()) { proxy.scrollTo(Self.bottomAnchor, anchor: .bottom) }
            }
            .marbleKeepsLatestVisible(proxy: proxy, anchor: Self.bottomAnchor, following: follow.following)
            .overlay(alignment: .bottomTrailing) {
                if follow.showJumpButton {
                    Button {
                        follow.jumpToLatest(now: now())
                        proxy.scrollTo(Self.bottomAnchor, anchor: .bottom)
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

    private func now() -> TimeInterval {
        Date().timeIntervalSinceReferenceDate
    }

    /// Follow the tail only while the person is at the end, and at most as
    /// fast as the throttle allows — streaming must never fight their scroll.
    private func scrollIfFollowing(_ proxy: ScrollViewProxy) {
        guard follow.shouldFollow(now: now()) else { return }
        DispatchQueue.main.async { proxy.scrollTo(Self.bottomAnchor, anchor: .bottom) }
    }
}

/// One transcript entry. The person's own messages sit in a blue bubble on
/// the right; replies read as full-width Markdown under a clear speaker line
/// with the marble's avatar and name in its colour.
struct MarbleEntryView: View {
    @EnvironmentObject private var store: MarbleStore
    var entry: MarbleEntry
    /// Who said it, when the row is on a shared timeline. Nil on one-to-one
    /// chats, which need no label.
    var speaker: MarbleSpeakerStyle? = nil

    private var isUser: Bool { entry.kind == "user" }
    private var task: MarbleTask? {
        guard !entry.taskId.isEmpty else { return nil }
        return store.state.tasks.first { $0.id == entry.taskId }
    }

    var body: some View {
        let failure = MarbleTurnFailure.representsFailure(entry, status: task?.status)
        if entry.kind == "assistant" || failure {
            ForEach(store.tools(for: entry.taskId)) { MarbleToolRow(tool: $0) }
        }
        if failure {
            MarbleFailedTurnRow(task: task)
        } else {
            message
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

    var body: some View {
        DisclosureGroup("Thought for a moment") {
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
            if !hasFailure && !hasAnswer && (showsLiveTools || !task.isRunning) {
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

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Label(task.flatMap { MarbleTurnFailure.message(for: $0.status) } ?? "Marble couldn’t finish this reply.",
                  systemImage: "exclamationmark.circle")
                .foregroundStyle(.secondary)
            if let task, MarbleTurnFailure.message(for: task.status) != nil,
               !task.prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                Button("Retry") { Task { await store.retry(task) } }
                    .disabled(store.retryingTurns.contains(task.id))
                if store.retryFailures.contains(task.id) {
                    Text("Couldn’t retry. Try again in a moment.")
                        .foregroundStyle(.secondary)
                }
            }
        }
        .font(.footnote)
    }
}

/// The message box: grows with the text up to six lines, then scrolls
/// inside. Return adds a line; the arrow sends. While the keyboard is up a
/// button beside the box hides it.
struct ComposerBar: View {
    @Binding var draft: String
    var isWorking: Bool
    var isDictating: Bool = false
    var placeholder: String = "Message"
    var workingPlaceholder: String = "Add to what Marble is doing"
    /// False where the host cannot take a correction mid-turn.
    var canSteer: Bool = true
    var onSend: () -> Void
    /// Correct the running turn instead of starting a new one.
    var onSteer: () -> Void = {}
    var onStop: () -> Void
    var onDictate: (() -> Void)? = nil
    @FocusState private var focused: Bool

    private var action: MarbleSteer.Action {
        MarbleSteer.action(isWorking: isWorking, canSteer: canSteer)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
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
                Button(action: action == .steer ? onSteer : onSend) {
                    Image(systemName: "arrow.up.circle.fill")
                        .font(.system(size: 32))
                        .symbolRenderingMode(.palette)
                        .foregroundStyle(.white, trimmed.isEmpty ? Color(.systemGray3) : Color.accentColor)
                        .frame(width: 38, height: 38)
                        .contentShape(Rectangle())
                }
                .disabled(trimmed.isEmpty)
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

    /// Keep the newest message in view when the keyboard opens, as long as
    /// the person was reading the end of the chat.
    func marbleKeepsLatestVisible(proxy: ScrollViewProxy, anchor: String, following: Bool) -> some View {
        onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardDidShowNotification)) { _ in
            guard following else { return }
            withAnimation(.easeOut(duration: 0.2)) {
                proxy.scrollTo(anchor, anchor: .bottom)
            }
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
