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
                        MarbleWorkingDots()
                            .padding(.leading, 2)
                    }
                    if !store.liveText.isEmpty {
                        MarbleEntryView(entry: .preview(kind: "assistant", text: store.liveText))
                    }
                    if let task = retryableTask {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(footnote(task))
                                .font(.footnote)
                                .foregroundStyle(.secondary)
                            Button("Retry") { Task { await store.retry(task) } }
                                .font(.footnote.weight(.semibold))
                        }
                    }
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

    /// The newest turn for this chat that ended without a reply.
    private var retryableTask: MarbleTask? {
        let conversationId = store.selectedConversationId
        return store.state.tasks.reversed().first { task in
            ["failed", "cancelled", "interrupted"].contains(task.status)
                && task.conversationId == conversationId
                && !task.prompt.isEmpty
        }
    }

    private func footnote(_ task: MarbleTask) -> String {
        switch task.status {
        case "cancelled": return "Cancelled before replying"
        case "interrupted": return "Interrupted before replying"
        default: return "Failed before replying"
        }
    }

    private func now() -> TimeInterval {
        Date().timeIntervalSinceReferenceDate
    }

    /// Follow the tail only while the person is at the end, and at most as
    /// fast as the throttle allows — streaming must never fight their scroll.
    private func scrollIfFollowing(_ proxy: ScrollViewProxy) {
        guard follow.shouldFollow(now: now()) else { return }
        proxy.scrollTo(Self.bottomAnchor, anchor: .bottom)
    }
}

/// One transcript entry: assistant replies read as plain full-width text on
/// the background; the person's own messages sit in a quiet bubble.
struct MarbleEntryView: View {
    var entry: MarbleEntry
    /// Who said it, when the row is on a shared timeline — a coloured dot and
    /// the marble's name. Nil on one-to-one chats, which need no label.
    var speaker: MarbleSpeakerStyle? = nil

    private var isUser: Bool { entry.kind == "user" }
    private var isMuted: Bool { entry.kind == "reasoning" || entry.kind == "system" }

    var body: some View {
        if isUser {
            Text(entry.text)
                .font(.body)
                .textSelection(.enabled)
                .padding(.horizontal, 12)
                .padding(.vertical, 8)
                .background(
                    Color(.secondarySystemBackground),
                    in: RoundedRectangle(cornerRadius: 16, style: .continuous)
                )
                .frame(maxWidth: .infinity, alignment: .trailing)
        } else if entry.kind == "handoff", let handoff = entry.handoff {
            HandoffLine(handoff: handoff)
        } else if entry.kind == "reasoning" {
            // Thinking stays tucked away — never inline with the reply.
            DisclosureGroup("Reasoning") {
                Text(entry.text)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .font(.caption)
            .foregroundStyle(.secondary)
        } else {
            VStack(alignment: .leading, spacing: 4) {
                if let speaker {
                    HStack(spacing: 5) {
                        Circle()
                            .fill(Color(hue: Double(speaker.hue ?? 210) / 360, saturation: 0.55, brightness: 0.75))
                            .frame(width: 7, height: 7)
                        Text(speaker.name)
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(.secondary)
                    }
                } else if isMuted {
                    Text(entry.kind)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Text(entry.text)
                    .font(isMuted ? .footnote : .body)
                    .foregroundStyle(isMuted ? .secondary : .primary)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }
}

struct MarbleToolRow: View {
    var tool: MarbleStore.LiveTool

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Image(systemName: tool.isError ? "exclamationmark.triangle" : "wrench.and.screwdriver")
                .font(.caption)
                .foregroundStyle(tool.isError ? .red : .secondary)
            Text(tool.name)
                .font(.caption.weight(.semibold))
            Text(tool.summary)
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(2)
        }
    }
}

/// The composer floats above the transcript — the one place real glass
/// belongs in this app.
struct ComposerBar: View {
    @Binding var draft: String
    var isWorking: Bool
    var isDictating: Bool = false
    var onSend: () -> Void
    /// Correct the running turn instead of starting a new one.
    var onSteer: () -> Void = {}
    var onStop: () -> Void
    var onDictate: (() -> Void)? = nil
    @FocusState private var focused: Bool

    var body: some View {
        HStack(alignment: .bottom, spacing: 10) {
            if let onDictate {
                Button(action: onDictate) {
                    Image(systemName: isDictating ? "waveform" : "mic")
                        .imageScale(.medium)
                        .foregroundStyle(isDictating ? .red : .secondary)
                        .frame(width: 44, height: 44)
                        .contentShape(Rectangle())
                }
                .accessibilityLabel(isDictating ? "Stop dictation" : "Dictate")
            }

            TextField(
                isWorking
                    ? "Correct Marble while it works…"
                    : "Ask Marble to work in this workspace…",
                text: $draft,
                axis: .vertical
            )
            .lineLimit(1...6)
            .focused($focused)
            .submitLabel(.send)
            .onSubmit(onSend)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(
                Color(.secondarySystemBackground),
                in: RoundedRectangle(cornerRadius: 18, style: .continuous)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 18, style: .continuous)
                    .strokeBorder(Color(.separator), lineWidth: 0.5)
            )

            Button(action: isWorking ? onSteer : onSend) {
                Image(systemName: "arrow.up.circle.fill")
                    .font(.system(size: 30))
                    .foregroundStyle(trimmed.isEmpty ? .secondary : MarbleTheme.tint)
                    .frame(width: 44, height: 44)
                    .contentShape(Rectangle())
            }
            .disabled(trimmed.isEmpty)
            .accessibilityLabel(isWorking ? "Steer" : "Send")
            if isWorking {
                Button(action: onStop) {
                    Image(systemName: "stop.circle.fill")
                        .font(.system(size: 30))
                        .foregroundStyle(.red)
                        .frame(width: 44, height: 44)
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

/// A governed call asking for a decision, rendered as a grouped inset block.
struct ApprovalCard: View {
    var approval: MarbleApproval
    /// `remember` widens one allow into a standing grant for this directory.
    var onDecision: (_ allow: Bool, _ remember: Bool) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(approval.tool)
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
