import GlassModels
import SwiftUI

struct TimelineRow: View {
    var entry: TimelineEntry

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
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("timeline.assistant")
        case .reasoning(_, let text):
            DisclosureGroup {
                Text(text)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } label: {
                Label("Reasoning", systemImage: "text.alignleft")
                    .font(.footnote.weight(.medium))
                    .foregroundStyle(.secondary)
            }
            .accessibilityIdentifier("timeline.reasoning")
        case .tool(_, let name, let summary, let output, let failed):
            ToolCard(name: name, summary: summary, output: output, failed: failed)
        case .notice(_, let text, let isError):
            Label(text, systemImage: isError ? "exclamationmark.triangle.fill" : "info.circle")
                .font(.footnote)
                .foregroundStyle(isError ? .red : .secondary)
        case .checklist(_, let tasks):
            if !tasks.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    Label("Checklist", systemImage: "checklist").font(.footnote.weight(.semibold))
                    ForEach(Array(tasks.enumerated()), id: \.offset) { _, task in
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
        case .largeRecord(_, _, let bytes):
            Label("Large record · \(ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file)) kept on the host",
                  systemImage: "doc.text.magnifyingglass")
                .font(.footnote)
                .foregroundStyle(.secondary)
        }
    }
}

private struct ToolCard: View {
    var name: String
    var summary: String
    var output: String
    var failed: Bool?
    @State private var expanded = false

    var body: some View {
        DisclosureGroup(isExpanded: $expanded) {
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
                    Text(name).font(.footnote.weight(.semibold))
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
