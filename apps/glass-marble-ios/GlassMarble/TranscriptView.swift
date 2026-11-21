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
            .onChange(of: store.transcript.count) { _, _ in scroll(proxy) }
            .onChange(of: store.liveText) { _, _ in scroll(proxy) }
            .onChange(of: store.liveTools.count) { _, _ in scroll(proxy) }
            .onChange(of: store.loaded) { _, _ in scroll(proxy) }
            .animation(.snappy(duration: 0.24), value: store.transcript.count)
            .animation(.snappy(duration: 0.2), value: store.isWorking)
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

    private func scroll(_ proxy: ScrollViewProxy) {
        withAnimation(.easeOut(duration: 0.18)) {
            proxy.scrollTo(Self.bottomAnchor, anchor: .bottom)
        }
    }
}

/// One transcript entry: assistant replies read as plain full-width text on
/// the background; the person's own messages sit in a quiet bubble.
struct MarbleEntryView: View {
    var entry: MarbleEntry

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
        } else {
            VStack(alignment: .leading, spacing: 4) {
                if isMuted {
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
            .padding(.vertical, 6)

            if let onDictate {
                Button(action: onDictate) {
                    Image(systemName: isDictating ? "waveform" : "mic")
                        .imageScale(.medium)
                        .frame(width: 34, height: 34)
                        .foregroundStyle(isDictating ? .red : .secondary)
                }
                .accessibilityLabel(isDictating ? "Stop dictation" : "Dictate")
            }

            Button(action: isWorking ? onSteer : onSend) {
                Image(systemName: isWorking ? "arrow.turn.up.left" : "arrow.up")
                    .imageScale(.medium)
                    .fontWeight(.bold)
                    .frame(width: 34, height: 34)
            }
            .buttonStyle(.borderedProminent)
            .buttonBorderShape(.circle)
            .disabled(trimmed.isEmpty)
            .accessibilityLabel(isWorking ? "Steer" : "Send")

            if isWorking {
                Button(action: onStop) {
                    Image(systemName: "stop.fill")
                        .imageScale(.medium)
                        .frame(width: 34, height: 34)
                }
                .buttonStyle(.bordered)
                .buttonBorderShape(.circle)
                .tint(.red)
                .accessibilityLabel("Stop")
            }
        }
        .padding(.leading, 16)
        .padding(.trailing, 8)
        .padding(.vertical, 8)
        .marbleFloating()
        .padding(.horizontal, 14)
        .padding(.bottom, 10)
        .frame(maxWidth: 820)
        .frame(maxWidth: .infinity)
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
