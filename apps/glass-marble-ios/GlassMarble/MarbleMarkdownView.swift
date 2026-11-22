import SwiftUI

/// Native rendering of a reply's Markdown: quotes, lists, tables, code, and
/// links. Parsing lives in `MarbleMarkdown`; this view only draws its blocks,
/// so it can be swapped for the shared rich components when they land.
struct MarbleMarkdownView: View {
    var text: String
    var font: Font = .body
    var color: Color = .primary

    var body: some View {
        MarbleMarkdownBlocks(blocks: MarbleMarkdown.parse(text), font: font, color: color)
    }

    static func inline(_ value: String) -> AttributedString {
        let options = AttributedString.MarkdownParsingOptions(
            interpretedSyntax: .inlineOnlyPreservingWhitespace,
            failurePolicy: .returnPartiallyParsedIfPossible
        )
        return (try? AttributedString(markdown: value, options: options)) ?? AttributedString(value)
    }
}

private struct MarbleMarkdownBlocks: View {
    var blocks: [MarbleMarkdownBlock]
    var font: Font
    var color: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                view(for: block)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private func view(for block: MarbleMarkdownBlock) -> some View {
        switch block {
        case .paragraph(let value):
            Text(MarbleMarkdownView.inline(value))
                .font(font)
                .foregroundStyle(color)
                .fixedSize(horizontal: false, vertical: true)
        case .heading(let level, let value):
            Text(MarbleMarkdownView.inline(value))
                .font(level == 1 ? .title3.bold() : level == 2 ? .headline : .subheadline.bold())
                .foregroundStyle(color)
                .accessibilityAddTraits(.isHeader)
        case .quote(let inner):
            HStack(alignment: .top, spacing: 10) {
                RoundedRectangle(cornerRadius: 1.5)
                    .fill(Color(.tertiaryLabel))
                    .frame(width: 3)
                MarbleMarkdownBlocks(blocks: inner, font: font, color: .secondary)
            }
            .fixedSize(horizontal: false, vertical: true)
        case .list(_, let items):
            VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(items.enumerated()), id: \.offset) { _, item in
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        if let checked = item.checked {
                            Image(systemName: checked ? "checkmark.square.fill" : "square")
                                .foregroundStyle(checked ? Color.accentColor : .secondary)
                        } else {
                            Text(item.marker)
                                .foregroundStyle(.secondary)
                                .monospacedDigit()
                        }
                        Text(MarbleMarkdownView.inline(item.text))
                            .foregroundStyle(color)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .font(font)
                    .padding(.leading, CGFloat(item.level) * 16)
                }
            }
        case .code(let language, let code):
            MarbleCodeBlock(code: code, language: language)
        case .table(let header, let alignments, let rows):
            MarbleTableBlock(header: header, alignments: alignments, rows: rows)
        case .rule:
            Divider()
        }
    }
}

private struct MarbleCodeBlock: View {
    var code: String
    var language: String
    @State private var copied = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(language.isEmpty ? "code" : language)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                Spacer()
                Button {
                    UIPasteboard.general.string = code
                    copied = true
                    Task {
                        try? await Task.sleep(for: .seconds(1.5))
                        copied = false
                    }
                } label: {
                    Image(systemName: copied ? "checkmark" : "doc.on.doc")
                        .font(.caption)
                }
                .buttonStyle(.borderless)
                .accessibilityLabel(copied ? "Copied" : "Copy code")
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            Divider()
            ScrollView(.horizontal, showsIndicators: false) {
                Text(code)
                    .font(.system(.footnote, design: .monospaced))
                    .textSelection(.enabled)
                    .padding(12)
            }
        }
        .background(Color(.secondarySystemBackground), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
}

private struct MarbleTableBlock: View {
    var header: [String]
    var alignments: [MarbleMarkdownAlignment]
    var rows: [[String]]

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            Grid(alignment: .leading, horizontalSpacing: 16, verticalSpacing: 8) {
                GridRow {
                    ForEach(Array(header.enumerated()), id: \.offset) { column, cell in
                        Text(MarbleMarkdownView.inline(cell))
                            .font(.subheadline.weight(.semibold))
                            .gridColumnAlignment(alignment(column))
                    }
                }
                Divider()
                ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                    GridRow {
                        ForEach(Array(row.enumerated()), id: \.offset) { _, cell in
                            Text(MarbleMarkdownView.inline(cell))
                                .font(.subheadline)
                        }
                    }
                }
            }
            .padding(12)
        }
        .background(Color(.secondarySystemBackground), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }

    private func alignment(_ column: Int) -> HorizontalAlignment {
        switch alignments.indices.contains(column) ? alignments[column] : .leading {
        case .leading: .leading
        case .center: .center
        case .trailing: .trailing
        }
    }
}
