import GlassModels
import SwiftUI

/// Renders assistant Markdown natively: prose, headings, nested and task lists, quotes, tables,
/// `<details>` sections, inline and display math, images, and fenced code.
struct MarkdownView: View {
    var text: String

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
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ForEach(Array(Self.blocks(text).enumerated()), id: \.offset) { _, block in
                BlockView(block: block)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    // MARK: Inline

    /// Inline Markdown with `$…$` and `\(…\)` math drawn as Unicode, and inline images removed
    /// (generated images are shown by the tool that made them).
    static func inline(_ value: String) -> AttributedString {
        var result = AttributedString()
        for segment in InlineMath.split(stripImages(value)) {
            switch segment {
            case .text(let text):
                result += markdown(text)
            case .math(let source):
                var math = AttributedString(InlineMath.render(source))
                math.inlinePresentationIntent = .emphasized
                result += math
            }
        }
        return result
    }

    private static func markdown(_ value: String) -> AttributedString {
        let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace, failurePolicy: .returnPartiallyParsedIfPossible)
        return (try? AttributedString(markdown: value, options: options)) ?? AttributedString(value)
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
    private static let separatorPattern = try! NSRegularExpression(pattern: #"^\|?\s*:?-{1,}:?\s*(\|\s*:?-{1,}:?\s*)*\|?$"#)

    static func blocks(_ text: String) -> [Block] {
        var blocks: [Block] = []
        var paragraph: [String] = []
        let lines = text.components(separatedBy: "\n")
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
                let fence = String(trimmed.prefix(3))
                let language = String(trimmed.dropFirst(3)).trimmingCharacters(in: .whitespaces)
                var code: [String] = []
                index += 1
                while index < lines.count, !lines[index].trimmingCharacters(in: .whitespaces).hasPrefix(fence) {
                    code.append(lines[index])
                    index += 1
                }
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

private struct BlockView: View {
    var block: MarkdownView.Block

    var body: some View {
        switch block {
        case .prose(let value):
            Text(MarkdownView.inline(value)).textSelection(.enabled)
        case .heading(let value, let level):
            Text(MarkdownView.inline(value))
                .font(level == 1 ? .title2.bold() : level == 2 ? .title3.bold() : .headline)
                .padding(.top, level <= 2 ? 4 : 0)
                .accessibilityAddTraits(.isHeader)
        case .item(let value, let marker, let depth):
            ListItemView(text: value, marker: marker, depth: depth)
        case .code(let code, let language):
            switch language.lowercased() {
            case ReplyComponent.fence: ComponentBlock(source: code)
            case "mermaid": RichBlockView(kind: .mermaid, source: code)
            case "math", "latex", "tex", "katex": RichBlockView(kind: .math, source: code)
            default: CodeBlock(code: code, language: language)
            }
        case .math(let source):
            RichBlockView(kind: .math, source: source)
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
            DataTable(header: header.map { MarkdownView.inline($0) }, alignments: alignments,
                      rows: rows.map { $0.map { MarkdownView.inline($0) } })
                .accessibilityIdentifier("markdown.table")
        case .details(let summary, let body):
            DetailsBlock(summary: summary, content: body)
        case .image(let alt, let url):
            if let url = URL(string: url), ["https", "http"].contains(url.scheme?.lowercased() ?? "") {
                AsyncImage(url: url) { phase in
                    switch phase {
                    case .success(let image): image.resizable().scaledToFit()
                    case .failure: EmptyView()
                    default: Color(.secondarySystemFill).frame(height: 160)
                    }
                }
                .clipShape(.rect(cornerRadius: 12, style: .continuous))
                .accessibilityLabel(alt.isEmpty ? "Image" : alt)
            }
            // Local paths are images a tool already showed above; nothing more to draw.
        }
    }
}

private struct ListItemView: View {
    var text: String
    var marker: MarkdownView.ListMarker
    var depth: Int
    @ScaledMetric(relativeTo: .body) private var indent: CGFloat = 18

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
            Text(MarkdownView.inline(text))
                .foregroundStyle(marker == .task(done: true) ? .secondary : .primary)
                .textSelection(.enabled)
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

private struct DetailsBlock: View {
    var summary: String
    var content: String
    @State private var expanded = false

    var body: some View {
        DisclosureGroup(isExpanded: $expanded) {
            MarkdownView(text: content).padding(.top, 8)
        } label: {
            Text(MarkdownView.inline(summary)).font(.body.weight(.medium)).foregroundStyle(.primary)
        }
        .tint(.secondary)
        .accessibilityIdentifier("markdown.details")
    }
}

/// A table with hairline row separators that scrolls sideways when it is wider than the reply.
struct DataTable: View {
    var header: [AttributedString]
    var alignments: [MarkdownView.ColumnAlignment] = []
    var rows: [[AttributedString]]

    private func alignment(_ column: Int) -> HorizontalAlignment {
        switch column < alignments.count ? alignments[column] : .leading {
        case .center: .center
        case .trailing: .trailing
        default: .leading
        }
    }

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            Grid(alignment: .leading, horizontalSpacing: 20, verticalSpacing: 0) {
                GridRow {
                    ForEach(header.indices, id: \.self) { column in
                        Text(header[column])
                            .font(.subheadline.weight(.semibold))
                            .gridColumnAlignment(alignment(column))
                            .padding(.vertical, 8)
                    }
                }
                Divider().gridCellUnsizedAxes(.horizontal)
                ForEach(rows.indices, id: \.self) { row in
                    GridRow {
                        ForEach(header.indices, id: \.self) { column in
                            Text(column < rows[row].count ? rows[row][column] : "")
                                .font(.subheadline)
                                .frame(maxWidth: 260, alignment: .init(horizontal: alignment(column), vertical: .center))
                                .fixedSize(horizontal: false, vertical: true)
                                .padding(.vertical, 8)
                                .textSelection(.enabled)
                        }
                    }
                    if row < rows.count - 1 {
                        Divider().gridCellUnsizedAxes(.horizontal)
                    }
                }
            }
            .padding(.horizontal, 2)
        }
        .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
    }
}

/// Inline LaTeX shown as Unicode text, so short formulas sit in the sentence without a web view.
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
                let wrap = { (part: String) in part.count > 1 && part.contains(where: { "+-·× ".contains($0) }) ? "(\(part))" : part }
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
                    .foregroundStyle(.secondary)
                Spacer()
                if isHTML, let previewHTML {
                    Button("Preview", systemImage: "play.rectangle") { previewHTML(code) }
                        .font(.caption)
                        .labelStyle(.iconOnly)
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
                .accessibilityLabel(copied ? "Copied" : "Copy code")
            }
            .padding(.horizontal, 14)
            .padding(.top, 10)
            .padding(.bottom, 4)
            ScrollView(.horizontal, showsIndicators: false) {
                Text(code)
                    .font(.system(.footnote, design: .monospaced))
                    .textSelection(.enabled)
                    .padding(.horizontal, 14)
                    .padding(.bottom, 12)
                    .padding(.top, 4)
            }
        }
        .background(Color(.secondarySystemBackground), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("timeline.code")
    }
}
