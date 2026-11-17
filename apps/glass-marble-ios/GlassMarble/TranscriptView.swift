import SwiftUI

struct TranscriptView: View {
    @EnvironmentObject private var store: MarbleStore

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 12) {
                    if store.transcript.isEmpty
                        && store.liveText.isEmpty
                        && store.liveReasoning.isEmpty
                        && store.liveTools.isEmpty {
                        MarbleEmptyState()
                    }
                    ForEach(store.transcript) { entry in
                        MarbleBubble(entry: entry)
                    }
                    if !store.liveReasoning.isEmpty {
                        MarbleBubble(entry: .preview(kind: "reasoning", text: store.liveReasoning))
                    }
                    ForEach(store.liveTools) { tool in
                        MarbleToolRow(tool: tool)
                    }
                    if !store.liveText.isEmpty {
                        MarbleBubble(entry: .preview(kind: "assistant", text: store.liveText))
                    }
                    Color.clear
                        .frame(height: 1)
                        .id(Self.bottomAnchor)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 14)
            }
            .scrollDismissesKeyboard(.interactively)
            .onChange(of: store.transcript.count) { _, _ in scroll(proxy) }
            .onChange(of: store.liveText) { _, _ in scroll(proxy) }
            .onChange(of: store.liveTools.count) { _, _ in scroll(proxy) }
        }
    }

    private static let bottomAnchor = "marble-bottom"

    private func scroll(_ proxy: ScrollViewProxy) {
        withAnimation(.easeOut(duration: 0.18)) {
            proxy.scrollTo(Self.bottomAnchor, anchor: .bottom)
        }
    }
}

struct MarbleBubble: View {
    var entry: MarbleEntry

    private var isUser: Bool { entry.kind == "user" }
    private var isMuted: Bool { entry.kind == "reasoning" || entry.kind == "system" }

    var body: some View {
        VStack(alignment: isUser ? .trailing : .leading, spacing: 4) {
            Text(label)
                .font(.caption2.weight(.semibold))
                .textCase(.uppercase)
                .foregroundStyle(MarblePalette.muted)
            Text(entry.text)
                .font(.callout)
                .foregroundStyle(isMuted ? MarblePalette.muted : MarblePalette.ink)
                .textSelection(.enabled)
                .padding(.horizontal, 13)
                .padding(.vertical, 10)
                .background(
                    bubbleColor,
                    in: RoundedRectangle(cornerRadius: 18, style: .continuous)
                )
        }
        .frame(maxWidth: .infinity, alignment: isUser ? .trailing : .leading)
    }

    private var label: String {
        switch entry.kind {
        case "user": return "you"
        case "assistant": return "marble"
        case "reasoning": return "reasoning"
        case "system": return "system"
        default: return entry.kind
        }
    }

    private var bubbleColor: Color {
        if isUser { return MarblePalette.accent.opacity(0.30) }
        if isMuted { return Color.white.opacity(0.05) }
        return Color.white.opacity(0.09)
    }
}

struct MarbleToolRow: View {
    var tool: MarbleStore.LiveTool

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Image(systemName: tool.isError ? "exclamationmark.triangle" : "wrench.and.screwdriver")
                .font(.caption2)
                .foregroundStyle(tool.isError ? MarblePalette.danger : MarblePalette.muted)
            Text(tool.name)
                .font(.caption.weight(.semibold))
                .foregroundStyle(MarblePalette.ink)
            Text(tool.summary)
                .font(.caption)
                .foregroundStyle(MarblePalette.muted)
                .lineLimit(2)
        }
        .padding(.leading, 10)
        .overlay(alignment: .leading) {
            Rectangle()
                .fill(Color.white.opacity(0.12))
                .frame(width: 2)
        }
    }
}

struct MarbleEmptyState: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            MarbleMarbleMark()
            Text("A durable collaborator on your own machine")
                .font(.headline)
                .foregroundStyle(MarblePalette.ink)
            Text("Ask Marble to read, change, or explain something in this workspace. Every turn is journaled on the host, so you can stop it, resume it later, and audit exactly what changed.")
                .font(.callout)
                .foregroundStyle(MarblePalette.muted)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .marbleGlass(cornerRadius: 24)
    }
}

struct ComposerBar: View {
    @Binding var draft: String
    var isWorking: Bool
    var onSend: () -> Void
    var onStop: () -> Void
    @FocusState private var focused: Bool

    var body: some View {
        HStack(alignment: .bottom, spacing: 10) {
            TextField(
                "Ask Marble to work in this workspace…",
                text: $draft,
                axis: .vertical
            )
            .lineLimit(1...6)
            .focused($focused)
            .submitLabel(.send)
            .onSubmit(onSend)
            .padding(.vertical, 6)
            .foregroundStyle(MarblePalette.ink)

            Button(action: isWorking ? onStop : onSend) {
                Image(systemName: isWorking ? "stop.fill" : "arrow.up")
                    .font(.system(size: 15, weight: .bold))
                    .frame(width: 34, height: 34)
                    .background(isWorking ? MarblePalette.danger : MarblePalette.accent, in: Circle())
                    .foregroundStyle(.white)
            }
            .buttonStyle(.plain)
            .disabled(!isWorking && trimmed.isEmpty)
            .opacity(!isWorking && trimmed.isEmpty ? 0.4 : 1)
            .accessibilityLabel(isWorking ? "Stop" : "Send")
        }
        .padding(.leading, 16)
        .padding(.trailing, 8)
        .padding(.vertical, 8)
        .marbleGlass(cornerRadius: 26)
        .padding(.horizontal, 14)
        .padding(.bottom, 10)
    }

    private var trimmed: String {
        draft.trimmingCharacters(in: .whitespacesAndNewlines)
    }
}

struct ApprovalCard: View {
    var approval: MarbleApproval
    /// `remember` widens one allow into a standing grant for this directory.
    var onDecision: (_ allow: Bool, _ remember: Bool) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("approval · \(approval.tool)")
                .font(.caption2.weight(.semibold))
                .textCase(.uppercase)
                .foregroundStyle(MarblePalette.muted)
            Text(approval.argumentsSummary)
                .font(.caption.monospaced())
                .foregroundStyle(MarblePalette.ink)
                .lineLimit(3)
            HStack(spacing: 8) {
                decisionButton("Deny", tint: MarblePalette.danger, allow: false, remember: false)
                decisionButton("Always", tint: MarblePalette.muted, allow: true, remember: true)
                decisionButton("Allow", tint: MarblePalette.accent, allow: true, remember: false)
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .marbleGlass(cornerRadius: 22)
    }

    private func decisionButton(
        _ title: String,
        tint: Color,
        allow: Bool,
        remember: Bool
    ) -> some View {
        Button(title) { onDecision(allow, remember) }
            .font(.caption.weight(.semibold))
            .foregroundStyle(.white)
            .padding(.horizontal, 14)
            .padding(.vertical, 7)
            .background(tint, in: Capsule())
            .buttonStyle(.plain)
    }
}
