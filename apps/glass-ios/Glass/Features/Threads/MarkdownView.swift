import GlassModels
import SwiftUI

extension EnvironmentValues {
    @Entry var openReferencedFile: ((String) -> Void)? = nil
    @Entry var completeMarkdownFencesOnly = false
}

/// Renders assistant Markdown natively: prose, headings, nested and task lists, quotes, tables,
/// `<details>` sections, inline and display math, images, and fenced code.
struct MarkdownView: View, Equatable {
    @Environment(\.completeMarkdownFencesOnly) private var completeFencesOnly
    var text: String
    var streaming = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    enum ListMarker: Hashable {
        case bullet
        case number(String)
        case task(done: Bool)
    }

    enum ColumnAlignment: Hashable { case leading, center, trailing }

    enum Block: Hashable {
        case prose(String)
        case heading(String, level: Int)
        case item(String, marker: ListMarker, depth: Int)
        case code(String, language: String)
        case math(String)
        case rule
        case quote(String)
        case table(header: [String], alignments: [ColumnAlignment], rows: [[String]])
        case details(summary: String, body: String)
        case image(alt: String, url: String)
        case file(label: String, path: String)
    }

    private final class ParsedBlocks {
        let blocks: [Block]
        init(_ blocks: [Block]) { self.blocks = blocks }
    }

    private final class InlineText {
        let value: AttributedString
        init(_ value: AttributedString) { self.value = value }
    }

    private static let inlineCache: NSCache<NSString, InlineText> = {
        let cache = NSCache<NSString, InlineText>()
        cache.totalCostLimit = 2 * 1024 * 1024
        cache.countLimit = 1024
        return cache
    }()

    private static let blockCache: NSCache<NSString, ParsedBlocks> = {
        let cache = NSCache<NSString, ParsedBlocks>()
        cache.totalCostLimit = 4 * 1024 * 1024
        cache.countLimit = 256
        return cache
    }()

    nonisolated static func == (left: Self, right: Self) -> Bool {
        left.text == right.text && left.streaming == right.streaming
    }

    private var parsedBlocks: [Block] {
        // While streaming, everything before the last settled paragraph break is parsed once and cached;
        // only the growing tail is parsed again. The finished reply is parsed whole, once.
        if streaming, let split = Self.settledBreak(in: text) {
            return Self.cachedBlocks(String(text[..<split]), completeFencesOnly: completeFencesOnly)
                + Self.blocks(String(text[split...]), completeFencesOnly: completeFencesOnly)
        }
        return Self.cachedBlocks(text, completeFencesOnly: completeFencesOnly)
    }

    private static func cachedBlocks(_ text: String, completeFencesOnly: Bool) -> [Block] {
        let key = (completeFencesOnly ? "1" + text : "0" + text) as NSString
        if let cached = blockCache.object(forKey: key) { return cached.blocks }
        let blocks = blocks(text, completeFencesOnly: completeFencesOnly)
        blockCache.setObject(ParsedBlocks(blocks), forKey: key, cost: text.utf8.count * 3)
        return blocks
    }

    /// The last blank line outside a code fence or details block, where the text before it can no longer change.
    static func settledBreak(in text: String) -> String.Index? {
        guard let blank = text.range(of: "\n\n", options: .backwards) else { return nil }
        let head = text[..<blank.lowerBound]
        var fences = 0
        var details = 0
        for line in head.split(separator: "\n", omittingEmptySubsequences: true) {
            let trimmed = line.drop { $0 == " " }
            if trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~") || trimmed.hasPrefix("$$") { fences += 1 }
            if trimmed.hasPrefix("<details") { details += 1 }
            if trimmed.hasPrefix("</details") { details -= 1 }
        }
        return fences.isMultiple(of: 2) && details <= 0 ? blank.upperBound : nil
    }

    var body: some View {
        let blocks = parsedBlocks
        VStack(alignment: .leading, spacing: Design.gap) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { index, block in
                // The newest two blocks fade their new text in; the one before keeps its fade if a block starts.
                if streaming, index >= blocks.count - 2 {
                    BlockView(block: block)
                        .modifier(StreamFade(count: Self.fadeCount(block)))
                } else {
                    BlockView(block: block).equatable()
                }
            }
        }
        .lineSpacing(4)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    // MARK: Inline

    static func inline(_ value: String, mathSize: CGFloat, colorScheme: ColorScheme) -> Text {
        let segments = InlineMath.split(stripImages(value))
        var accessibleText = Text("")
        var hasMath = false
        let typeset = segments.reduce(Text("")) { text, segment in
            let next: Text
            let accessible: Text
            switch segment {
            case .text(let prose):
                next = Text(markdown(prose))
                accessible = next
            case .math(let source):
                hasMath = true
                if let image = NativeMath.image(source, size: mathSize, display: false, colorScheme: colorScheme) {
                    accessible = Text(InlineMath.render(source))
                    next = Text(NativeMath.artwork(image))
                } else {
                    accessible = Text(verbatim: source)
                    next = Text(verbatim: InlineMath.render(source)).font(.system(.body, design: .serif))
                }
            }
            accessibleText = Text("\(accessibleText)\(accessible)")
            return Text("\(text)\(next)")
        }
        return hasMath ? typeset.accessibilityLabel(accessibleText) : typeset
    }

    /// Characters a block draws, so a stream knows which glyphs just arrived. Zero means it is not faded.
    static func fadeCount(_ block: Block) -> Int {
        switch block {
        case .prose(let value), .heading(let value, _), .item(let value, _, _):
            InlineMath.split(stripImages(value)).reduce(0) { count, segment in
                if case .text(let prose) = segment { return count + markdown(prose).characters.count }
                return count + 1
            }
        case .code(let code, _): code.count
        default: 0
        }
    }

    private static func markdown(_ value: String) -> AttributedString {
        let key = value as NSString
        if let cached = inlineCache.object(forKey: key) { return cached.value }
        let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace, failurePolicy: .returnPartiallyParsedIfPossible)
        var text = (try? AttributedString(markdown: value, options: options)) ?? AttributedString(value)
        for run in text.runs {
            if run.inlinePresentationIntent?.contains(.code) == true {
                text[run.range].backgroundColor = Design.accent.opacity(0.12)
            }
            if run.link != nil { text[run.range].underlineStyle = .single }
        }
        inlineCache.setObject(InlineText(text), forKey: key, cost: value.utf8.count * 3)
        return text
    }

    private static let inlineImage = try! NSRegularExpression(pattern: #"!\[[^\]]*\]\([^)]*\)"#)

    static func stripImages(_ value: String) -> String {
        guard value.contains("![") else { return value }
        let range = NSRange(value.startIndex..., in: value)
        return inlineImage.stringByReplacingMatches(in: value, range: range, withTemplate: "")
    }

    // MARK: Blocks

    private static let listPattern = try! NSRegularExpression(pattern: #"^([ \t]*)([-*+]|\d{1,9}[.)])[ \t]+(.*)$"#)
    private static let imagePattern = try! NSRegularExpression(pattern: #"^!\[([^\]]*)\]\(<?([^)\s>]+)>?(?:\s+"[^"]*")?\)$"#)
    private static let filePattern = #/^\[([^\]]+)\]\(((?:[\w.-]+\/)*[\w.-]+\.[A-Za-z][\w-]*)\)$/#
    private static let separatorPattern = try! NSRegularExpression(pattern: #"^\|?\s*:?-{1,}:?\s*(\|\s*:?-{1,}:?\s*)*\|?$"#)

    static func blocks(_ text: String, completeFencesOnly: Bool = false) -> [Block] {
        var blocks: [Block] = []
        var paragraph: [String] = []
        let lines = text.replacingOccurrences(of: "\r\n", with: "\n").components(separatedBy: "\n")
        var index = 0

        func flush() {
            let joined = paragraph.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
            if !joined.isEmpty { blocks.append(.prose(joined)) }
            paragraph = []
        }

        while index < lines.count {
            let line = lines[index]
            let trimmed = line.trimmingCharacters(in: .whitespaces)

            // Fenced code (also glass-ui, mermaid and math fences).
            if trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~") {
                flush()
                let marker: Character = trimmed.first == "~" ? "~" : "`"
                let fenceLength = trimmed.prefix { $0 == marker }.count
                let language = String(trimmed.dropFirst(fenceLength)).trimmingCharacters(in: .whitespaces)
                var code: [String] = []
                index += 1
                while index < lines.count {
                    let candidate = lines[index].trimmingCharacters(in: .whitespaces)
                    let closingLength = candidate.prefix { $0 == marker }.count
                    if closingLength >= fenceLength, candidate.count == closingLength { break }
                    code.append(lines[index])
                    index += 1
                }
                if completeFencesOnly, index == lines.count { break }
                blocks.append(.code(code.joined(separator: "\n"), language: language))
                index += 1
                continue
            }

            // Display math: $$ … $$ or \[ … \].
            if trimmed.hasPrefix("$$") || trimmed.hasPrefix("\\[") {
                let close = trimmed.hasPrefix("$$") ? "$$" : "\\]"
                let rest = String(trimmed.dropFirst(2))
                if rest.count >= 2, rest.hasSuffix(close) {
                    flush()
                    blocks.append(.math(String(rest.dropLast(2)).trimmingCharacters(in: .whitespaces)))
                    index += 1
                    continue
                }
                var body = [rest]
                var end = index + 1
                var closed = false
                while end < lines.count {
                    let candidate = lines[end].trimmingCharacters(in: .whitespaces)
                    if candidate.hasSuffix(close) {
                        body.append(String(candidate.dropLast(2)))
                        closed = true
                        break
                    }
                    body.append(lines[end])
                    end += 1
                }
                if closed {
                    flush()
                    blocks.append(.math(body.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)))
                    index = end + 1
                } else {
                    // Still streaming: show the source until the closing fence arrives.
                    paragraph.append(line)
                    index += 1
                }
                continue
            }

            if trimmed.isEmpty { flush(); index += 1; continue }

            // <details><summary>…</summary> … </details>
            if trimmed.lowercased().hasPrefix("<details") {
                flush()
                var depth = 0
                var collected: [String] = []
                var end = index
                while end < lines.count {
                    let current = lines[end]
                    let lower = current.lowercased()
                    depth += lower.components(separatedBy: "<details").count - 1
                    depth -= lower.components(separatedBy: "</details>").count - 1
                    collected.append(current)
                    end += 1
                    if depth <= 0 { break }
                }
                blocks.append(details(collected.joined(separator: "\n")))
                index = end
                continue
            }

            // Blockquote (nested quotes recurse).
            if trimmed.hasPrefix(">") {
                flush()
                var quoted: [String] = []
                while index < lines.count {
                    let candidate = lines[index].trimmingCharacters(in: .whitespaces)
                    guard candidate.hasPrefix(">") else { break }
                    var inner = candidate.dropFirst()
                    if inner.hasPrefix(" ") { inner = inner.dropFirst() }
                    quoted.append(String(inner))
                    index += 1
                }
                blocks.append(.quote(quoted.joined(separator: "\n")))
                continue
            }

            // Table: a header row followed by a separator row.
            if trimmed.contains("|"), index + 1 < lines.count, matches(separatorPattern, lines[index + 1].trimmingCharacters(in: .whitespaces)) {
                let header = cells(trimmed)
                let separator = cells(lines[index + 1].trimmingCharacters(in: .whitespaces))
                if header.count >= 1, separator.count == header.count || separator.count >= 1 {
                    flush()
                    let alignments: [ColumnAlignment] = header.indices.map { column in
                        let spec = column < separator.count ? separator[column] : ""
                        if spec.hasPrefix(":") && spec.hasSuffix(":") { return .center }
                        if spec.hasSuffix(":") { return .trailing }
                        return .leading
                    }
                    var rows: [[String]] = []
                    index += 2
                    while index < lines.count {
                        let row = lines[index].trimmingCharacters(in: .whitespaces)
                        guard !row.isEmpty, row.contains("|") else { break }
                        var values = cells(row)
                        if values.count < header.count { values += Array(repeating: "", count: header.count - values.count) }
                        rows.append(Array(values.prefix(header.count)))
                        index += 1
                    }
                    blocks.append(.table(header: header, alignments: alignments, rows: rows))
                    continue
                }
            }

            if let hashes = trimmed.firstIndex(where: { $0 != "#" }), trimmed.hasPrefix("#"),
               trimmed[hashes] == " ", trimmed.distance(from: trimmed.startIndex, to: hashes) <= 6 {
                flush()
                blocks.append(.heading(String(trimmed[hashes...]).trimmingCharacters(in: .whitespaces),
                                       level: trimmed.distance(from: trimmed.startIndex, to: hashes)))
            } else if ["---", "***", "___"].contains(trimmed.replacingOccurrences(of: " ", with: "")) && trimmed.count >= 3 {
                flush()
                blocks.append(.rule)
            } else if let match = imagePattern.firstMatch(in: trimmed, range: NSRange(trimmed.startIndex..., in: trimmed)),
                      let alt = Range(match.range(at: 1), in: trimmed), let url = Range(match.range(at: 2), in: trimmed) {
                flush()
                blocks.append(.image(alt: String(trimmed[alt]), url: String(trimmed[url])))
            } else if let file = trimmed.wholeMatch(of: filePattern),
                      !file.2.split(separator: "/").contains("..") {
                flush()
                blocks.append(.file(label: String(file.1), path: String(file.2)))
            } else if let item = listItem(line) {
                flush()
                blocks.append(item)
            } else if paragraph.isEmpty, line.first?.isWhitespace == true,
                      case .item(let text, let marker, let depth)? = blocks.last {
                // An indented continuation line belongs to the list item above it.
                blocks[blocks.count - 1] = .item(text + " " + trimmed, marker: marker, depth: depth)
            } else {
                paragraph.append(line)
            }
            index += 1
        }
        flush()
        return blocks
    }

    private static func matches(_ pattern: NSRegularExpression, _ value: String) -> Bool {
        pattern.firstMatch(in: value, range: NSRange(value.startIndex..., in: value)) != nil
    }

    private static func listItem(_ line: String) -> Block? {
        guard let match = listPattern.firstMatch(in: line, range: NSRange(line.startIndex..., in: line)),
              let lead = Range(match.range(at: 1), in: line), let mark = Range(match.range(at: 2), in: line),
              let rest = Range(match.range(at: 3), in: line) else { return nil }
        let indent = line[lead].reduce(0) { $0 + ($1 == "\t" ? 4 : 1) }
        let depth = min(indent / 2, 6)
        let symbol = String(line[mark])
        var content = String(line[rest])
        if ["-", "*", "+"].contains(symbol) {
            let lower = content.lowercased()
            if lower.hasPrefix("[ ] ") || lower == "[ ]" {
                content = String(content.dropFirst(3)).trimmingCharacters(in: .whitespaces)
                return .item(content, marker: .task(done: false), depth: depth)
            }
            if lower.hasPrefix("[x] ") || lower == "[x]" {
                content = String(content.dropFirst(3)).trimmingCharacters(in: .whitespaces)
                return .item(content, marker: .task(done: true), depth: depth)
            }
            return .item(content, marker: .bullet, depth: depth)
        }
        return .item(content, marker: .number(symbol.replacingOccurrences(of: ")", with: ".")), depth: depth)
    }

    /// Splits a table row on unescaped pipes outside inline code.
    static func cells(_ row: String) -> [String] {
        var value = row
        if value.hasPrefix("|") { value.removeFirst() }
        if value.hasSuffix("|"), !value.hasSuffix("\\|") { value.removeLast() }
        var cells: [String] = []
        var current = ""
        var inCode = false
        var previous: Character?
        for character in value {
            if character == "`" { inCode.toggle() }
            if character == "|", !inCode, previous != "\\" {
                cells.append(current.trimmingCharacters(in: .whitespaces))
                current = ""
            } else {
                if character == "|", previous == "\\" { current.removeLast() }
                current.append(character)
            }
            previous = character
        }
        cells.append(current.trimmingCharacters(in: .whitespaces))
        return cells
    }

    private static func details(_ source: String) -> Block {
        var body = source
        if let open = body.range(of: "<details", options: .caseInsensitive),
           let end = body.range(of: ">", range: open.upperBound..<body.endIndex) {
            body.removeSubrange(open.lowerBound..<end.upperBound)
        }
        if let close = body.range(of: "</details>", options: [.caseInsensitive, .backwards]) {
            body.removeSubrange(close)
        }
        var summary = "Details"
        if let open = body.range(of: "<summary>", options: .caseInsensitive),
           let close = body.range(of: "</summary>", options: .caseInsensitive, range: open.upperBound..<body.endIndex) {
            summary = String(body[open.upperBound..<close.lowerBound])
                .replacingOccurrences(of: "\n", with: " ")
                .trimmingCharacters(in: .whitespaces)
            body.removeSubrange(open.lowerBound..<close.upperBound)
        }
        return .details(summary: summary.isEmpty ? "Details" : summary, body: body.trimmingCharacters(in: .whitespacesAndNewlines))
    }
}

private struct BlockView: View, Equatable {
    var block: MarkdownView.Block

    nonisolated static func == (left: Self, right: Self) -> Bool { left.block == right.block }
    @Environment(\.openReferencedFile) private var openReferencedFile
    @Environment(\.colorScheme) private var colorScheme
    @ScaledMetric(relativeTo: .body) private var mathSize = 17.0

    var body: some View {
        switch block {
        case .prose(let value):
            MarkdownView.inline(value, mathSize: mathSize, colorScheme: colorScheme).font(Design.body).selectableUnlessStreaming()
        case .heading(let value, let level):
            MarkdownView.inline(value, mathSize: mathSize, colorScheme: colorScheme)
                .font(level == 1 ? Design.title : level == 2 ? .title3.weight(.semibold) : Design.headline)
                .padding(.top, level <= 2 ? 4 : 0)
                .accessibilityAddTraits(.isHeader)
        case .item(let value, let marker, let depth):
            ListItemView(text: value, marker: marker, depth: depth)
        case .code(let code, let language):
            switch language.lowercased() {
            case ReplyComponent.fence: ComponentBlock(source: code)
            case "mermaid": RichBlockView(source: code)
            case "math", "latex", "tex", "katex": DisplayMath(source: code)
            default: CodeBlock(code: code, language: language)
            }
        case .math(let source):
            DisplayMath(source: source)
        case .rule:
            Divider().padding(.vertical, 4)
        case .quote(let inner):
            HStack(alignment: .top, spacing: 12) {
                Capsule().fill(Color(.tertiaryLabel)).frame(width: 3)
                MarkdownView(text: inner).foregroundStyle(.secondary)
            }
            .fixedSize(horizontal: false, vertical: true)
            .accessibilityElement(children: .combine)
            .accessibilityLabel("Quote")
            .accessibilityIdentifier("markdown.quote")
        case .table(let header, let alignments, let rows):
            DataTable(header: header, alignments: alignments, rows: rows)
                .equatable()
                .accessibilityIdentifier("markdown.table")
        case .details(let summary, let body):
            DetailsBlock(summary: summary, content: body)
        case .image(let alt, let url):
            if let url = URL(string: url), ["https", "http"].contains(url.scheme?.lowercased() ?? "") {
                RemoteMarkdownImage(url: url, alt: alt).id(url)
            }
            // Local paths are images a tool already showed above; nothing more to draw.
        case .file(let label, let path):
            if let openReferencedFile {
                Button { openReferencedFile(path) } label: {
                    Label(label, systemImage: "doc.text")
                        .font(.footnote.weight(.medium))
                        .foregroundStyle(Design.ink)
                        .padding(.horizontal, 12)
                        .frame(minHeight: Design.control)
                        .background(Design.inset, in: .capsule)
                        .contentShape(.capsule)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("markdown.file")
            } else {
                Label(label, systemImage: "doc.text")
            }
        }
    }
}

private struct ListItemView: View {
    var text: String
    var marker: MarkdownView.ListMarker
    var depth: Int
    @Environment(\.colorScheme) private var colorScheme
    @ScaledMetric(relativeTo: .body) private var indent: CGFloat = 18
    @ScaledMetric(relativeTo: .body) private var mathSize = 17.0

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            switch marker {
            case .bullet:
                Text(depth == 0 ? "•" : depth == 1 ? "◦" : "▪︎").foregroundStyle(.secondary)
                    .accessibilityHidden(true)
            case .number(let number):
                Text(number).foregroundStyle(.secondary).monospacedDigit()
            case .task(let done):
                Image(systemName: done ? "checkmark.circle.fill" : "circle")
                    .foregroundStyle(done ? AnyShapeStyle(.tint) : AnyShapeStyle(.tertiary))
                    .accessibilityHidden(true)
            }
            MarkdownView.inline(text, mathSize: mathSize, colorScheme: colorScheme)
                .foregroundStyle(marker == .task(done: true) ? .secondary : .primary)
                .selectableUnlessStreaming()
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.leading, CGFloat(depth) * indent)
        .accessibilityElement(children: .combine)
        .accessibilityValue(taskValue)
        .accessibilityIdentifier(isTask ? "markdown.task" : "markdown.item")
    }

    private var isTask: Bool { if case .task = marker { true } else { false } }
    private var taskValue: String {
        if case .task(let done) = marker { done ? "Done" : "Not done" } else { "" }
    }
}

struct RemoteMarkdownImage: View {
    let url: URL
    let alt: String
    @State private var requested = false

    var body: some View {
        if requested {
            AsyncImage(url: url) { phase in
                switch phase {
                case .success(let image): image.resizable().scaledToFit()
                case .failure: Label("Couldn’t load image", systemImage: "photo")
                default: ProgressView()
                }
            }
            .clipShape(.rect(cornerRadius: Design.corner))
            .accessibilityLabel(alt.isEmpty ? "Image" : alt)
        } else {
            Button { requested = true } label: {
                Label(alt.isEmpty ? "Load remote image" : "Load image: \(alt)", systemImage: "photo")
            }
            .glassButtonStyle()
            .accessibilityIdentifier("markdown.loadRemoteImage")
        }
    }
}

private struct DetailsBlock: View {
    var summary: String
    var content: String
    @Environment(\.colorScheme) private var colorScheme
    @State private var expanded = false
    @ScaledMetric(relativeTo: .body) private var mathSize = 17.0

    var body: some View {
        DisclosureGroup(isExpanded: $expanded) {
            MarkdownView(text: content).padding(.top, 8)
        } label: {
            MarkdownView.inline(summary, mathSize: mathSize, colorScheme: colorScheme)
                .font(.body.weight(.medium)).foregroundStyle(.primary)
        }
        .tint(.secondary)
        .accessibilityIdentifier("markdown.details")
    }
}

struct DataTable: View, Equatable {
    var header: [String]
    var alignments: [MarkdownView.ColumnAlignment] = []
    var rows: [[String]]
    var markdown = true
    @Environment(\.colorScheme) private var colorScheme
    @ScaledMetric(relativeTo: .subheadline) private var mathSize = 15.0
    @ScaledMetric(relativeTo: .subheadline) private var minimumColumnWidth = 120.0
    @ScaledMetric(relativeTo: .subheadline) private var maximumColumnWidth = 260.0
    @State private var viewportWidth = 0.0

    nonisolated static func == (left: Self, right: Self) -> Bool {
        left.header == right.header && left.alignments == right.alignments && left.rows == right.rows && left.markdown == right.markdown
    }

    private var columnWidth: CGFloat {
        min(maximumColumnWidth, max(minimumColumnWidth, viewportWidth / CGFloat(max(1, header.count))))
    }

    private func alignment(_ column: Int) -> TextAlignment {
        switch column < alignments.count ? alignments[column] : .leading {
        case .center: .center
        case .trailing: .trailing
        default: .leading
        }
    }

    private func cell(_ text: String, column: Int, isHeader: Bool = false) -> some View {
        let alignment = alignment(column)
        return (markdown ? MarkdownView.inline(text, mathSize: mathSize, colorScheme: colorScheme) : Text(verbatim: text))
            .font(isHeader ? .subheadline.weight(.semibold) : .subheadline)
            .multilineTextAlignment(alignment)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: alignment == .center ? .center : alignment == .trailing ? .trailing : .leading)
            .padding(.horizontal, 12)
            .padding(.vertical, 9)
            .frame(width: columnWidth)
            .selectableUnlessStreaming()
            .accessibilityAddTraits(isHeader ? .isHeader : [])
    }

    var body: some View {
        ScrollView(.horizontal) {
            Grid(alignment: .topLeading, horizontalSpacing: 0, verticalSpacing: 0) {
                GridRow {
                    ForEach(header.indices, id: \.self) { column in
                        cell(header[column], column: column, isHeader: true)
                    }
                }
                .background(Design.inset)
                Divider().gridCellUnsizedAxes(.horizontal)
                ForEach(rows.indices, id: \.self) { row in
                    GridRow {
                        ForEach(header.indices, id: \.self) { column in
                            cell(column < rows[row].count ? rows[row][column] : "", column: column)
                        }
                    }
                    if row < rows.count - 1 {
                        Divider().gridCellUnsizedAxes(.horizontal)
                    }
                }
            }
            .overlay(alignment: .topLeading) {
                HStack(spacing: 0) {
                    ForEach(header.indices.dropLast(), id: \.self) { _ in
                        Divider().frame(width: columnWidth, alignment: .trailing)
                    }
                }
                .allowsHitTesting(false)
                .accessibilityHidden(true)
            }
            .frame(minWidth: viewportWidth, alignment: .leading)
        }
        .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
        .defaultScrollAnchor(.leading)
        .background(Design.surface)
        .clipShape(.rect(cornerRadius: Design.corner))
        .overlay {
            RoundedRectangle(cornerRadius: Design.corner).strokeBorder(Design.line)
                .allowsHitTesting(false)
        }
        .onGeometryChange(for: CGFloat.self, of: \.size.width) { viewportWidth = $0 }
    }
}

/// Locates inline LaTeX and supplies a readable fallback when typesetting fails.
enum InlineMath {
    enum Segment: Equatable {
        case text(String)
        case math(String)
    }

    /// Finds `$…$` (not `$$`, not money like "$5 and $10") and `\(…\)`.
    static func split(_ value: String) -> [Segment] {
        guard value.contains("$") || value.contains("\\(") else { return [.text(value)] }
        var segments: [Segment] = []
        var text = ""
        let characters = Array(value)
        var index = 0
        var inCode = false
        while index < characters.count {
            let character = characters[index]
            if character == "`" { inCode.toggle() }
            if !inCode, character == "\\", index + 1 < characters.count, characters[index + 1] == "(" {
                if let close = find(characters, ["\\", ")"], from: index + 2) {
                    if !text.isEmpty { segments.append(.text(text)); text = "" }
                    segments.append(.math(String(characters[(index + 2)..<close])))
                    index = close + 2
                    continue
                }
            }
            if !inCode, character == "$", index + 1 < characters.count, characters[index + 1] != "$",
               !characters[index + 1].isWhitespace, index == 0 || characters[index - 1] != "\\" {
                var end = index + 1
                var found: Int?
                while end < characters.count, characters[end] != "\n" {
                    if characters[end] == "$", characters[end - 1] != "\\", !characters[end - 1].isWhitespace,
                       end + 1 >= characters.count || !characters[end + 1].isNumber {
                        found = end
                        break
                    }
                    end += 1
                }
                if let found {
                    if !text.isEmpty { segments.append(.text(text)); text = "" }
                    segments.append(.math(String(characters[(index + 1)..<found])))
                    index = found + 1
                    continue
                }
            }
            text.append(character)
            index += 1
        }
        if !text.isEmpty { segments.append(.text(text)) }
        return segments
    }

    private static func find(_ characters: [Character], _ needle: [Character], from start: Int) -> Int? {
        guard start <= characters.count - needle.count else { return nil }
        for index in start...(characters.count - needle.count) where Array(characters[index..<(index + needle.count)]) == needle {
            return index
        }
        return nil
    }

    static let symbols: [String: String] = [
        "alpha": "α", "beta": "β", "gamma": "γ", "delta": "δ", "epsilon": "ε", "varepsilon": "ε", "zeta": "ζ", "eta": "η",
        "theta": "θ", "vartheta": "ϑ", "iota": "ι", "kappa": "κ", "lambda": "λ", "mu": "μ", "nu": "ν", "xi": "ξ", "pi": "π",
        "rho": "ρ", "sigma": "σ", "tau": "τ", "upsilon": "υ", "phi": "φ", "varphi": "φ", "chi": "χ", "psi": "ψ", "omega": "ω",
        "Gamma": "Γ", "Delta": "Δ", "Theta": "Θ", "Lambda": "Λ", "Xi": "Ξ", "Pi": "Π", "Sigma": "Σ", "Phi": "Φ", "Psi": "Ψ", "Omega": "Ω",
        "sum": "∑", "prod": "∏", "int": "∫", "oint": "∮", "infty": "∞", "partial": "∂", "nabla": "∇", "sqrt": "√",
        "cdot": "·", "times": "×", "div": "÷", "pm": "±", "mp": "∓", "leq": "≤", "le": "≤", "geq": "≥", "ge": "≥", "neq": "≠", "ne": "≠",
        "approx": "≈", "equiv": "≡", "sim": "∼", "propto": "∝", "to": "→", "rightarrow": "→", "leftarrow": "←", "Rightarrow": "⇒",
        "Leftarrow": "⇐", "leftrightarrow": "↔", "iff": "⇔", "implies": "⇒", "mapsto": "↦", "in": "∈", "notin": "∉", "subset": "⊂",
        "subseteq": "⊆", "supset": "⊃", "cup": "∪", "cap": "∩", "emptyset": "∅", "forall": "∀", "exists": "∃", "neg": "¬",
        "land": "∧", "lor": "∨", "ldots": "…", "cdots": "⋯", "dots": "…", "circ": "∘", "degree": "°", "angle": "∠", "perp": "⊥",
        "parallel": "∥", "hbar": "ℏ", "ell": "ℓ", "Re": "ℜ", "Im": "ℑ", "aleph": "ℵ", "quad": "  ", "qquad": "    ",
        "langle": "⟨", "rangle": "⟩", "lfloor": "⌊", "rfloor": "⌋", "lceil": "⌈", "rceil": "⌉", "mid": "∣", "star": "⋆",
        "log": "log", "ln": "ln", "sin": "sin", "cos": "cos", "tan": "tan", "exp": "exp", "lim": "lim", "max": "max", "min": "min",
    ]
    private static let blackboard: [Character: String] = ["R": "ℝ", "N": "ℕ", "Z": "ℤ", "Q": "ℚ", "C": "ℂ", "P": "ℙ"]
    private static let superscripts: [Character: Character] = [
        "0": "⁰", "1": "¹", "2": "²", "3": "³", "4": "⁴", "5": "⁵", "6": "⁶", "7": "⁷", "8": "⁸", "9": "⁹",
        "+": "⁺", "-": "⁻", "=": "⁼", "(": "⁽", ")": "⁾", "n": "ⁿ", "i": "ⁱ", "x": "ˣ", "y": "ʸ", "k": "ᵏ", "t": "ᵗ", "a": "ᵃ",
        "b": "ᵇ", "c": "ᶜ", "d": "ᵈ", "e": "ᵉ", "m": "ᵐ", "p": "ᵖ", "r": "ʳ", "s": "ˢ", "T": "ᵀ", "*": "*", "′": "′",
    ]
    private static let subscripts: [Character: Character] = [
        "0": "₀", "1": "₁", "2": "₂", "3": "₃", "4": "₄", "5": "₅", "6": "₆", "7": "₇", "8": "₈", "9": "₉",
        "+": "₊", "-": "₋", "=": "₌", "(": "₍", ")": "₎", "a": "ₐ", "e": "ₑ", "i": "ᵢ", "j": "ⱼ", "k": "ₖ", "n": "ₙ", "m": "ₘ",
        "o": "ₒ", "p": "ₚ", "r": "ᵣ", "s": "ₛ", "t": "ₜ", "u": "ᵤ", "v": "ᵥ", "x": "ₓ",
    ]

    static func render(_ source: String) -> String {
        var parser = Parser(characters: Array(source))
        return parser.expression(until: nil).replacingOccurrences(of: "  ", with: " ").trimmingCharacters(in: .whitespaces)
    }

    private struct Parser {
        var characters: [Character]
        var index = 0

        mutating func expression(until close: Character?) -> String {
            var out = ""
            while index < characters.count {
                let character = characters[index]
                if let close, character == close { index += 1; return out }
                switch character {
                case "\\": out += command()
                case "{": index += 1; out += expression(until: "}")
                case "^": index += 1; out += script(InlineMath.superscripts, fallback: "^")
                case "_": index += 1; out += script(InlineMath.subscripts, fallback: "_")
                case "~": index += 1; out += " "
                default: index += 1; out.append(character)
                }
            }
            return out
        }

        /// One argument: a braced group or a single token.
        mutating func argument() -> String {
            while index < characters.count, characters[index] == " " { index += 1 }
            guard index < characters.count else { return "" }
            if characters[index] == "{" { index += 1; return expression(until: "}") }
            if characters[index] == "\\" { return command() }
            defer { index += 1 }
            return String(characters[index])
        }

        mutating func script(_ table: [Character: Character], fallback: String) -> String {
            let value = argument()
            let mapped = value.compactMap { table[$0] }
            if mapped.count == value.count, !value.isEmpty { return String(mapped) }
            return value.count == 1 ? fallback + value : fallback + "(" + value + ")"
        }

        mutating func command() -> String {
            index += 1
            guard index < characters.count else { return "" }
            var name = ""
            while index < characters.count, characters[index].isLetter { name.append(characters[index]); index += 1 }
            if name.isEmpty {
                let character = characters[index]
                index += 1
                switch character {
                case ",", ";", ":", " ", "!": return character == "!" ? "" : " "
                case "{", "}", "$", "%", "&", "#", "_": return String(character)
                case "\\": return " "
                default: return String(character)
                }
            }
            switch name {
            case "frac", "dfrac", "tfrac":
                let top = argument(), bottom = argument()
                // Parenthesise only when an operator sits outside any brackets: "a+b" → "(a+b)", "n(n+1)" stays.
                let wrap = { (part: String) -> String in
                    var depth = 0
                    for character in part {
                        if character == "(" { depth += 1 } else if character == ")" { depth -= 1 }
                        else if depth == 0, "+-·× ".contains(character) { return "(\(part))" }
                    }
                    return part
                }
                return wrap(top) + "⁄" + wrap(bottom)
            case "sqrt":
                var degree = ""
                if index < characters.count, characters[index] == "[" {
                    index += 1
                    degree = expression(until: "]")
                }
                let value = argument()
                let root = degree == "3" ? "∛" : degree == "4" ? "∜" : "√"
                return root + (value.count > 1 ? "(\(value))" : value)
            case "text", "mathrm", "mathbf", "mathit", "operatorname", "textbf", "textit", "mathsf", "boldsymbol":
                return argument()
            case "mathbb":
                return argument().map { InlineMath.blackboard[$0] ?? String($0) }.joined()
            case "left", "right", "big", "Big", "bigg", "Bigg", "displaystyle", "limits", "nolimits":
                return ""
            case "hat": return argument() + "\u{0302}"
            case "bar", "overline": return argument() + "\u{0305}"
            case "vec": return argument() + "\u{20D7}"
            case "dot": return argument() + "\u{0307}"
            case "tilde": return argument() + "\u{0303}"
            default:
                return InlineMath.symbols[name].map { $0.count > 1 && $0.first!.isLetter ? $0 + " " : $0 } ?? name
            }
        }
    }
}

struct CodeBlock: View {
    var code: String
    var language: String
    @State private var copied = false
    @Environment(\.previewHTML) private var previewHTML

    private var isHTML: Bool { ["html", "htm", "xhtml"].contains(language.lowercased()) }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(language.isEmpty ? "Code" : language)
                    .font(.caption.weight(.medium))
                    .foregroundStyle(Design.muted)
                Spacer()
                if isHTML, let previewHTML {
                    Button("Preview", systemImage: "play.rectangle") { previewHTML(code) }
                        .font(.caption)
                        .labelStyle(.iconOnly)
                        .frame(minWidth: Design.control, minHeight: Design.control)
                        .accessibilityLabel("Preview page")
                        .accessibilityIdentifier("code.preview")
                }
                Button(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc") {
                    UIPasteboard.general.string = code
                    copied = true
                    Task { try? await Task.sleep(for: .seconds(1.5)); copied = false }
                }
                .font(.caption)
                .labelStyle(.iconOnly)
                .frame(minWidth: Design.control, minHeight: Design.control)
                .accessibilityLabel(copied ? "Copied" : "Copy code")
            }
            .padding(.horizontal, Design.padding)
            Divider().overlay(Design.line)
            Text(highlighted)
                .font(.system(.footnote, design: .monospaced))
                .fixedSize(horizontal: false, vertical: true)
                .selectableUnlessStreaming()
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(Design.padding)
        }
        .background(Design.terminal, in: .rect(cornerRadius: Design.corner))
        .overlay { RoundedRectangle(cornerRadius: Design.corner).strokeBorder(Design.line) }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("timeline.code")
    }

    private var highlighted: AttributedString {
        guard !language.isEmpty, code.utf8.count <= 32_768 else { return AttributedString(code) }
        let tokens = code.ranges(of: /\/\/[^\n]*|\/\*[\s\S]*?\*\/|#[^\n]*|"(?:\\.|[^"\\\n])*"|'(?:\\.|[^'\\\n])*'|\b(?:let|var|func|class|struct|enum|import|return|if|else|for|in|while|async|await|try|throw|true|false|nil|const|def|fn|match|pub|guard|switch|case|extension)\b|\b\d+(?:\.\d+)?\b|[{}()\[\]=+*?!:<>.,\/-]/)
        var output = AttributedString()
        var start = code.startIndex
        for range in tokens {
            output += AttributedString(code[start..<range.lowerBound])
            let value = code[range]
            var token = AttributedString(value)
            token.foregroundColor = value.hasPrefix("//") || value.hasPrefix("/*") || value.hasPrefix("#") ? Design.muted
                : value.hasPrefix("\"") || value.hasPrefix("'") ? Design.warning
                : value.first?.isNumber == true ? Design.success
                : value.first?.isPunctuation == true || value.first?.isSymbol == true ? Design.muted : Design.accent
            output += token
            start = range.upperBound
        }
        output += AttributedString(code[start...])
        return output
    }
}

/// Newly streamed text eases in from low opacity, its tail a little behind its head, so the newest words
/// read as a soft gradient. Settled glyphs draw untouched; one renderer per block, no per-glyph views.
extension EnvironmentValues {
    @Entry var fadingStream = false
}

extension View {
    /// Selectable text draws through a path that skips custom text renderers, so it waits for the reply to settle.
    func selectableUnlessStreaming() -> some View { modifier(SelectableUnlessStreaming()) }
}

private struct SelectableUnlessStreaming: ViewModifier {
    @Environment(\.fadingStream) private var fadingStream

    func body(content: Content) -> some View {
        if fadingStream { content } else { content.textSelection(.enabled) }
    }
}

struct StreamFade: ViewModifier {
    struct Chunk { var start: Int; var date: Date }

    var count: Int
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var chunks: [Chunk] = []
    @State private var known: Int?
    @State private var idle = false

    private var duration: Double { reduceMotion ? 0.08 : 0.3 }
    private var lag: Double { reduceMotion ? 0 : 0.08 }

    func body(content: Content) -> some View {
        TimelineView(.animation(paused: idle)) { timeline in
            content.textRenderer(StreamFadeRenderer(chunks: chunks, known: known ?? 0, count: count,
                                                    now: timeline.date, duration: duration, lag: lag))
                .environment(\.fadingStream, true)
        }
        .onAppear { arrive(count) }
        .onChange(of: count) { _, value in arrive(value) }
        .task {
            // One cheap check a tenth of a second, rather than a task per token, pauses the clock once settled.
            while !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(100))
                let settled = chunks.last.map { Date.now.timeIntervalSince($0.date) > duration + lag } ?? true
                if settled != idle { idle = settled }
            }
        }
    }

    private func arrive(_ value: Int) {
        let now = Date.now
        idle = false
        chunks.removeAll { now.timeIntervalSince($0.date) > duration + lag || $0.start >= value }
        if value > (known ?? 0) { chunks.append(Chunk(start: known ?? 0, date: now)) }
        known = value
    }
}

private struct StreamFadeRenderer: TextRenderer {
    var chunks: [StreamFade.Chunk]
    var known: Int
    var count: Int
    var now: Date
    var duration: Double
    var lag: Double

    func draw(layout: Text.Layout, in context: inout GraphicsContext) {
        let settled = count == 0 ? Int.max : chunks.first?.start ?? known
        var index = 0
        for line in layout {
            for run in line {
                let end = index + run.count
                if end <= settled {
                    context.draw(run)
                    index = end
                    continue
                }
                for slice in run {
                    let opacity = index < settled ? 1 : self.opacity(at: index)
                    if opacity >= 0.999 {
                        context.draw(slice)
                    } else {
                        var faded = context
                        faded.opacity = opacity
                        faded.draw(slice)
                    }
                    index += 1
                }
            }
        }
    }

    private func opacity(at index: Int) -> Double {
        let floor = 0.15
        // Glyphs drawn before their arrival is recorded start at the floor instead of flashing in at full.
        guard index < known, let at = chunks.lastIndex(where: { $0.start <= index }) else { return floor }
        let chunk = chunks[at]
        let length = (at + 1 < chunks.count ? chunks[at + 1].start : known) - chunk.start
        let position = Double(index - chunk.start) / Double(max(length, 1))
        let progress = min(max((now.timeIntervalSince(chunk.date) - lag * position) / duration, 0), 1)
        return floor + (1 - floor) * (1 - pow(1 - progress, 3))
    }
}

/// Releases a streamed reply at a steady pace, so bursty network chunks read as an even flow. The pace follows
/// how fast text has been arriving and speeds up as the backlog grows, so it neither lags far behind nor stalls
/// between chunks. Replies already on screen when it appears show whole.
struct RevealingReply: View {
    var text: String
    var streaming: Bool
    @State private var anchor = 0
    @State private var anchorDate = Date.distantPast
    @State private var rate = 400.0
    @State private var lastArrival: Date?
    @State private var gap = 0.25
    @State private var caughtUp = true
    @State private var started = false

    var body: some View {
        TimelineView(.animation(paused: caughtUp)) { timeline in
            MarkdownView(text: visible(at: timeline.date), streaming: streaming || !caughtUp)
                .equatable()
        }
        .onAppear {
            guard !started else { return }
            started = true
            anchor = text.utf8.count
        }
        .onChange(of: text.utf8.count) { _, total in arrive(total) }
        .onChange(of: streaming) { _, _ in arrive(text.utf8.count) }
        .task(id: TaskKey(anchor: anchor, date: anchorDate, total: text.utf8.count)) {
            let remaining = Double(text.utf8.count - anchor) / rate
            guard remaining > 0 else { caughtUp = true; return }
            try? await Task.sleep(for: .seconds(remaining + 0.02))
            if !Task.isCancelled { caughtUp = true }
        }
    }

    private struct TaskKey: Hashable { var anchor: Int; var date: Date; var total: Int }

    private func revealed(at date: Date) -> Int {
        min(text.utf8.count, anchor + Int(max(0, date.timeIntervalSince(anchorDate)) * rate))
    }

    private func arrive(_ total: Int) {
        let now = Date.now
        let shown = min(revealed(at: now), total)
        if let lastArrival {
            // Quick to learn a longer pause, slow to forget it, so one quiet spell does not cause the next stall.
            let latest = min(max(now.timeIntervalSince(lastArrival), 0.016), 2)
            gap = latest > gap ? gap * 0.5 + latest * 0.5 : gap * 0.9 + latest * 0.1
        }
        lastArrival = now
        let backlog = Double(total - shown)
        // Aim to finish the backlog just as the next chunk is due; drain faster once the stream has ended.
        let horizon = streaming ? min(max(gap * 1.3, 0.25), 1.5) : 0.35
        rate = max(60, backlog / horizon)
        anchor = shown
        anchorDate = now
        caughtUp = shown >= total
    }

    /// The revealed prefix, ending on a word boundary so words arrive whole.
    private func visible(at date: Date) -> String {
        let utf8 = text.utf8
        var count = revealed(at: date)
        guard count < utf8.count else { return text }
        var index = utf8.index(utf8.startIndex, offsetBy: count)
        var scanned = 0
        while index < utf8.endIndex, scanned < 24, utf8[index] != 0x20, utf8[index] != 0x0A {
            index = utf8.index(after: index)
            scanned += 1
            count += 1
        }
        while index > utf8.startIndex, utf8[index] & 0xC0 == 0x80 { index = utf8.index(before: index) }
        return String(text[..<index])
    }
}
