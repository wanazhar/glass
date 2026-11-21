import SwiftUI

/// Renders assistant Markdown: prose via `AttributedString`, fenced code as scrollable monospaced blocks.
struct MarkdownView: View {
    var text: String

    enum Block: Hashable {
        case prose(String)
        case heading(String, level: Int)
        case bullet(String, marker: String)
        case code(String, language: String)
        case rule
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(Array(Self.blocks(text).enumerated()), id: \.offset) { _, block in
                switch block {
                case .prose(let value):
                    Text(Self.inline(value)).textSelection(.enabled)
                case .heading(let value, let level):
                    Text(Self.inline(value))
                        .font(level == 1 ? .title2.bold() : level == 2 ? .title3.bold() : .headline)
                        .accessibilityAddTraits(.isHeader)
                case .bullet(let value, let marker):
                    HStack(alignment: .firstTextBaseline, spacing: 8) {
                        Text(marker).foregroundStyle(.secondary).monospacedDigit()
                        Text(Self.inline(value)).textSelection(.enabled)
                    }
                case .code(let code, let language):
                    CodeBlock(code: code, language: language)
                case .rule:
                    Divider()
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    static func inline(_ value: String) -> AttributedString {
        let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace, failurePolicy: .returnPartiallyParsedIfPossible)
        return (try? AttributedString(markdown: value, options: options)) ?? AttributedString(value)
    }

    static func blocks(_ text: String) -> [Block] {
        var blocks: [Block] = []
        var paragraph: [String] = []
        var code: [String]?
        var language = ""
        func flush() {
            let joined = paragraph.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
            if !joined.isEmpty { blocks.append(.prose(joined)) }
            paragraph = []
        }
        for line in text.components(separatedBy: "\n") {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if trimmed.hasPrefix("```") {
                if let lines = code {
                    blocks.append(.code(lines.joined(separator: "\n"), language: language))
                    code = nil
                } else {
                    flush()
                    language = String(trimmed.dropFirst(3)).trimmingCharacters(in: .whitespaces)
                    code = []
                }
                continue
            }
            if code != nil { code?.append(line); continue }
            if trimmed.isEmpty { flush(); continue }
            if let hashes = trimmed.firstIndex(where: { $0 != "#" }), trimmed.hasPrefix("#"),
               trimmed[hashes] == " ", trimmed.distance(from: trimmed.startIndex, to: hashes) <= 6 {
                flush()
                blocks.append(.heading(String(trimmed[hashes...]).trimmingCharacters(in: .whitespaces),
                                       level: trimmed.distance(from: trimmed.startIndex, to: hashes)))
            } else if trimmed == "---" || trimmed == "***" {
                flush()
                blocks.append(.rule)
            } else if trimmed.hasPrefix("- ") || trimmed.hasPrefix("* ") {
                flush()
                blocks.append(.bullet(String(trimmed.dropFirst(2)), marker: "•"))
            } else if let dot = trimmed.firstIndex(of: "."), trimmed[..<dot].allSatisfy(\.isNumber), !trimmed[..<dot].isEmpty,
                      trimmed[trimmed.index(after: dot)...].hasPrefix(" ") {
                flush()
                blocks.append(.bullet(String(trimmed[trimmed.index(dot, offsetBy: 2)...]), marker: String(trimmed[...dot])))
            } else {
                paragraph.append(line)
            }
        }
        if let lines = code { blocks.append(.code(lines.joined(separator: "\n"), language: language)) }
        flush()
        return blocks
    }
}

struct CodeBlock: View {
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
                Button(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc") {
                    UIPasteboard.general.string = code
                    copied = true
                    Task { try? await Task.sleep(for: .seconds(1.5)); copied = false }
                }
                .font(.caption)
                .labelStyle(.iconOnly)
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
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("timeline.code")
    }
}
