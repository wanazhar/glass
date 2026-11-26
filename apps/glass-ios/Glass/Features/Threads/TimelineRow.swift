import GlassModels
import SwiftUI

extension EnvironmentValues {
    /// Loads an image shared into the thread through `GET /api/attachment`.
    @Entry var attachmentLoader: @Sendable @MainActor (String) async -> Data? = { _ in nil }
    /// Opens a large saved activity record by its history offset.
    @Entry var openSavedActivity: @MainActor (Int) -> Void = { _ in }
    /// Quotes or branches from a saved message.
    @Entry var messageAction: (@MainActor (TimelineEntry, MessageAction) -> Void)? = nil
    /// Shows an HTML code block as an isolated interactive page.
    @Entry var previewHTML: (@MainActor (String) -> Void)? = nil
}

struct TimelineRow: View {
    var entry: TimelineEntry
    var liveReasoning = false
    var reasoningExpansion: (Bool, CGFloat) -> Void = { _, _ in }
    @Environment(\.openSavedActivity) private var openSavedActivity
    @Environment(\.messageAction) private var messageAction
    @State private var reasoningExpanded = false
    @State private var reasoningTop: CGFloat = 0

    @ViewBuilder
    private func actions(for kinds: [MessageAction]) -> some View {
        if let messageAction, entry.recordOffset != nil {
            Divider()
            ForEach(kinds, id: \.self) { kind in
                Button(kind.title, systemImage: kind == .quote ? "text.quote" : "arrow.triangle.branch") {
                    messageAction(entry, kind)
                }
            }
        }
    }

    var body: some View {
        switch entry {
        case .user(_, let text):
            HStack {
                Spacer(minLength: 48)
                Text(text)
                    .textSelection(.enabled)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .background(Color(.secondarySystemBackground), in: .rect(cornerRadius: 18, style: .continuous))
                    .contextMenu {
                        CopyButton(text: text)
                        actions(for: [.editInBranch, .branch, .quote])
                    }
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("timeline.user")
        case .assistant(_, let text, let streaming):
            VStack(alignment: .leading, spacing: 6) {
                MarkdownView(text: text)
                if streaming {
                    ProgressView().controlSize(.small)
                }
            }
            .contextMenu {
                CopyButton(text: text)
                actions(for: [.quote, .branch])
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("timeline.assistant")
        case .reasoning(_, let text):
            DisclosureGroup(isExpanded: Binding(get: { reasoningExpanded }, set: { expanded in
                reasoningExpansion(expanded, reasoningTop)
                reasoningExpanded = expanded
            })) {
                Text(text)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityIdentifier("timeline.reasoning.full")
            } label: {
                VStack(alignment: .leading, spacing: 3) {
                    Label("Reasoning", systemImage: "text.alignleft")
                        .font(.footnote.weight(.medium))
                    if liveReasoning && !reasoningExpanded {
                        let preview = text.prefix(160)
                        Text(verbatim: String(preview) + (preview.endIndex == text.endIndex ? "" : "…"))
                            .font(.caption)
                            .lineLimit(2)
                            .accessibilityIdentifier("timeline.reasoning.preview")
                    }
                }
                .foregroundStyle(Color(.secondaryLabel))
            }
            .tint(Color(.secondaryLabel))
            .onGeometryChange(for: CGFloat.self) {
                $0.frame(in: .named(ConversationView.historyContent)).minY
            } action: { reasoningTop = $0 }
            .accessibilityIdentifier("timeline.reasoning")
        case .tool(_, let name, let summary, let output, let failed):
            if failed != true, let shared = SharedImage(name: name, output: output) {
                SharedImageView(image: shared)
            } else {
                ToolCard(name: name, summary: summary, output: output, failed: failed)
            }
        case .notice(_, let text, let isError):
            Label(text, systemImage: isError ? "exclamationmark.triangle.fill" : "info.circle")
                .font(.footnote)
                .foregroundStyle(isError ? .red : .secondary)
        case .checklist(_, let tasks):
            if !tasks.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    Label("Checklist", systemImage: "checklist").font(.footnote.weight(.semibold))
                    ForEach(Array(tasks.enumerated()), id: \.offset) { _, task in
                        // Recorded snapshot; the live board is in the toolbar checklist.
                        Label(task, systemImage: "circle").font(.footnote)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(12)
                .background(Color(.secondarySystemBackground), in: .rect(cornerRadius: 12, style: .continuous))
            }
        case .image(_, _, let base64):
            if let data = Data(base64Encoded: base64), let image = UIImage(data: data) {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFit()
                    .frame(maxHeight: 320)
                    .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
                    .accessibilityLabel("Thread attachment")
            }
        case .largeRecord(_, let offset, let bytes):
            Button {
                openSavedActivity(offset)
            } label: {
                Label("Large activity · \(ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file)). Tap to read.",
                      systemImage: "doc.text.magnifyingglass")
                    .font(.footnote)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .accessibilityIdentifier("timeline.largeRecord")
        }
    }
}

private struct ToolCard: View {
    var name: String
    var summary: String
    var output: String
    var failed: Bool?
    @AppStorage(ToolDensity.key) private var density: ToolDensity = .compact
    @State private var expanded: Bool?

    var body: some View {
        DisclosureGroup(isExpanded: Binding { expanded ?? (density == .expanded) } set: { expanded = $0 }) {
            if !output.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    Text(output)
                        .font(.system(.caption, design: .monospaced))
                        .textSelection(.enabled)
                        .padding(.vertical, 6)
                }
                .frame(maxHeight: 280)
            }
        } label: {
            HStack(spacing: 8) {
                Image(systemName: symbol)
                    .foregroundStyle(failed == true ? .red : failed == false ? .green : .secondary)
                VStack(alignment: .leading, spacing: 2) {
                    Text(CommandLabel.title(name)).font(.footnote.weight(.semibold)).foregroundStyle(.primary)
                    if !summary.isEmpty {
                        Text(summary)
                            .font(.system(.caption, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(Color(.secondarySystemBackground), in: .rect(cornerRadius: 12, style: .continuous))
        .tint(.secondary)
        .accessibilityIdentifier("timeline.tool")
    }

    private var symbol: String {
        switch failed {
        case .some(true): "xmark.octagon.fill"
        case .some(false): "checkmark.circle.fill"
        case .none: name == "bash" ? "terminal" : "wrench.and.screwdriver"
        }
    }
}

private struct CopyButton: View {
    var text: String
    var body: some View {
        Button("Copy", systemImage: "doc.on.doc") { UIPasteboard.general.string = text }
    }
}

/// A `display_image` tool result: `[display image]` then one JSON line.
struct SharedImage: Equatable {
    var path: String
    var attachmentID: String?
    var width: Int
    var height: Int

    init?(name: String, output: String) {
        guard name == "display_image", output.hasPrefix("[display image]\n") else { return nil }
        let line = output.split(separator: "\n", omittingEmptySubsequences: false).dropFirst().first.map(String.init) ?? ""
        guard let value = try? JSONDecoder().decode(JSONValue.self, from: Data(line.utf8)),
              value["kind"]?.stringValue == "image_display",
              let path = value["path"]?.stringValue, path.count <= 4096,
              let type = value["media_type"]?.stringValue, ["image/png", "image/jpeg", "image/gif", "image/webp"].contains(type),
              let width = value["width"]?.doubleValue, let height = value["height"]?.doubleValue,
              width > 0, height > 0, width * height <= 32_000_000 else { return nil }
        self.path = path
        self.width = Int(width)
        self.height = Int(height)
        attachmentID = value["attachment_id"]?.stringValue.flatMap { id in
            id.count == 64 && id.allSatisfy { "0123456789abcdef".contains($0) } ? id : nil
        }
    }

    var filename: String { path.split(separator: "/").last.map(String.init) ?? path }
}

private struct SharedImageView: View {
    var image: SharedImage
    @Environment(\.attachmentLoader) private var load
    @State private var loaded: UIImage?
    @State private var failed = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Group {
                if let loaded {
                    Image(uiImage: loaded).resizable().scaledToFit()
                } else {
                    Color(.secondarySystemBackground)
                        .aspectRatio(CGFloat(image.width) / CGFloat(image.height), contentMode: .fit)
                        .overlay { if failed { Text("Image unavailable").font(.footnote).foregroundStyle(.secondary) } else { ProgressView() } }
                }
            }
            .frame(maxHeight: 360)
            .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
            Text(image.filename).font(.caption).foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Shared image: \(image.filename)")
        .accessibilityIdentifier("timeline.sharedImage")
        .task(id: image.attachmentID) {
            guard let id = image.attachmentID else { failed = true; return }
            if let data = await load(id), let decoded = UIImage(data: data) { loaded = decoded } else { failed = true }
        }
    }
}
