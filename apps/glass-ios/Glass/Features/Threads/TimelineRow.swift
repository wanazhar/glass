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
    @Environment(\.messageAction) private var messageAction
    @State private var showingFullAnswer = false

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
                    .padding(.horizontal, Design.corner)
                    .padding(.vertical, Design.gap)
                    .background(Design.surface, in: .rect(cornerRadius: Design.panel))
                    .contextMenu {
                        CopyButton(text: text)
                        actions(for: [.editInBranch, .branch, .quote])
                    }
            }
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("timeline.user")
        case .assistant(_, let text, let streaming):
            VStack(alignment: .leading, spacing: Design.gap) {
                MarkdownView(text: !streaming && text.count > 5_000 ? answerPreview(text) : text)
                    .equatable()
                if !streaming, text.count > 5_000 {
                    Button("Show more", systemImage: "arrow.up.right.square") { showingFullAnswer = true }
                        .glassButtonStyle()
                }
            }
                .sheet(isPresented: $showingFullAnswer) {
                    NavigationStack {
                        ScrollView { MarkdownView(text: text).padding(Design.padding) }
                            .navigationTitle("Full reply")
                            .navigationBarTitleDisplayMode(.inline)
                            .toolbar {
                                ToolbarItem(placement: .confirmationAction) {
                                    Button("Done") { showingFullAnswer = false }
                                }
                            }
                    }
                }
                .contextMenu {
                    CopyButton(text: text)
                    actions(for: [.quote, .branch])
                }
                .accessibilityElement(children: .contain)
                .accessibilityIdentifier("timeline.assistant")
        case .tool(_, let name, _, let output, let failed):
            if failed != true, let shared = SharedImage(name: name, output: output) {
                SharedImageView(image: shared)
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
        default:
            EmptyView()
        }
    }

    private func answerPreview(_ text: String) -> String {
        let preview = String(text.prefix(2_400))
        guard let boundary = preview.range(of: "\n\n", options: .backwards) else { return preview + "…" }
        return String(preview[..<boundary.lowerBound])
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
