import Foundation

/// Block-level Markdown for chat replies, parsed into plain values so the
/// renderer stays a thin view and the rules are testable on Linux. Inline
/// syntax (bold, code spans, links) is left inside each block's text for the
/// view to render. When the shared Glass Core rich-component schema lands,
/// it replaces this type; the view only switches over these cases.
enum MarbleMarkdownBlock: Equatable {
    case heading(level: Int, text: String)
    case paragraph(String)
    case quote([MarbleMarkdownBlock])
    case list(ordered: Bool, items: [MarbleMarkdownListItem])
    case code(language: String, code: String)
    case table(header: [String], alignments: [MarbleMarkdownAlignment], rows: [[String]])
    case rule
}

struct MarbleMarkdownListItem: Equatable {
    /// "•" for bullets, "1." style for ordered items.
    var marker: String
    /// Nesting depth, from leading indentation (two spaces per level).
    var level: Int
    var text: String
    /// Set for `- [ ]` / `- [x]` task items.
    var checked: Bool?
}

enum MarbleMarkdownAlignment: Equatable {
    case leading, center, trailing
}

enum MarbleMarkdown {
    static func parse(_ text: String) -> [MarbleMarkdownBlock] {
        let lines = text.replacingOccurrences(of: "\r\n", with: "\n").components(separatedBy: "\n")
        return parse(lines: lines[...])
    }

    private static func parse(lines: ArraySlice<String>) -> [MarbleMarkdownBlock] {
        var blocks: [MarbleMarkdownBlock] = []
        var paragraph: [String] = []
        var index = lines.startIndex

        func flush() {
            let joined = paragraph.joined(separator: "\n").trimmingCharacters(in: .whitespacesAndNewlines)
            if !joined.isEmpty { blocks.append(.paragraph(joined)) }
            paragraph = []
        }

        while index < lines.endIndex {
            let line = lines[index]
            let trimmed = line.trimmingCharacters(in: .whitespaces)

            if let fence = fenceMarker(trimmed) {
                flush()
                let language = String(trimmed.dropFirst(fence.count)).trimmingCharacters(in: .whitespaces)
                var code: [String] = []
                index += 1
                while index < lines.endIndex,
                      !lines[index].trimmingCharacters(in: .whitespaces).hasPrefix(fence) {
                    code.append(lines[index])
                    index += 1
                }
                blocks.append(.code(language: language, code: code.joined(separator: "\n")))
                index += 1
                continue
            }

            if trimmed.isEmpty {
                flush()
                index += 1
                continue
            }

            if trimmed.hasPrefix(">") {
                flush()
                var inner: [String] = []
                while index < lines.endIndex {
                    let quoted = lines[index].trimmingCharacters(in: .whitespaces)
                    guard quoted.hasPrefix(">") else { break }
                    var body = quoted.dropFirst()
                    if body.hasPrefix(" ") { body = body.dropFirst() }
                    inner.append(String(body))
                    index += 1
                }
                blocks.append(.quote(parse(lines: inner[...])))
                continue
            }

            if let heading = heading(trimmed) {
                flush()
                blocks.append(heading)
                index += 1
                continue
            }

            if isRule(trimmed) {
                flush()
                blocks.append(.rule)
                index += 1
                continue
            }

            if trimmed.contains("|"), index + 1 < lines.endIndex,
               let alignments = delimiterRow(lines[index + 1]) {
                flush()
                let header = cells(trimmed)
                var rows: [[String]] = []
                index += 2
                while index < lines.endIndex {
                    let row = lines[index].trimmingCharacters(in: .whitespaces)
                    guard !row.isEmpty, row.contains("|") else { break }
                    rows.append(normalized(cells(row), count: header.count))
                    index += 1
                }
                blocks.append(.table(
                    header: header,
                    alignments: normalizedAlignments(alignments, count: header.count),
                    rows: rows
                ))
                continue
            }

            if let first = listItem(line) {
                flush()
                var items = [first.item]
                index += 1
                while index < lines.endIndex {
                    let next = lines[index]
                    if let item = listItem(next) {
                        items.append(item.item)
                        index += 1
                    } else if !next.trimmingCharacters(in: .whitespaces).isEmpty,
                              next.hasPrefix("  "), !items.isEmpty {
                        // An indented continuation line belongs to the item above.
                        items[items.count - 1].text += "\n" + next.trimmingCharacters(in: .whitespaces)
                        index += 1
                    } else {
                        break
                    }
                }
                blocks.append(.list(ordered: first.ordered, items: items))
                continue
            }

            paragraph.append(line)
            index += 1
        }
        flush()
        return blocks
    }

    private static func fenceMarker(_ trimmed: String) -> String? {
        if trimmed.hasPrefix("```") { return "```" }
        if trimmed.hasPrefix("~~~") { return "~~~" }
        return nil
    }

    private static func heading(_ trimmed: String) -> MarbleMarkdownBlock? {
        let hashes = trimmed.prefix(while: { $0 == "#" }).count
        guard (1...6).contains(hashes) else { return nil }
        let rest = trimmed.dropFirst(hashes)
        guard rest.first == " " else { return nil }
        let text = rest.trimmingCharacters(in: .whitespaces)
            .trimmingCharacters(in: CharacterSet(charactersIn: "#"))
            .trimmingCharacters(in: .whitespaces)
        return .heading(level: hashes, text: text)
    }

    private static func isRule(_ trimmed: String) -> Bool {
        let compact = trimmed.replacingOccurrences(of: " ", with: "")
        guard compact.count >= 3, let first = compact.first, "-*_".contains(first) else { return false }
        return compact.allSatisfy { $0 == first }
    }

    private static func cells(_ row: String) -> [String] {
        var body = row.trimmingCharacters(in: .whitespaces)
        if body.hasPrefix("|") { body.removeFirst() }
        if body.hasSuffix("|") { body.removeLast() }
        return body.components(separatedBy: "|").map { $0.trimmingCharacters(in: .whitespaces) }
    }

    private static func delimiterRow(_ line: String) -> [MarbleMarkdownAlignment]? {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        guard trimmed.contains("-"), trimmed.contains("|") || trimmed.hasPrefix(":") || trimmed.hasPrefix("-")
        else { return nil }
        var alignments: [MarbleMarkdownAlignment] = []
        for cell in cells(trimmed) {
            let leading = cell.hasPrefix(":")
            let trailing = cell.hasSuffix(":")
            let dashes = cell.trimmingCharacters(in: CharacterSet(charactersIn: ":"))
            guard !dashes.isEmpty, dashes.allSatisfy({ $0 == "-" }) else { return nil }
            alignments.append(leading && trailing ? .center : trailing ? .trailing : .leading)
        }
        return alignments.isEmpty ? nil : alignments
    }

    private static func normalized(_ row: [String], count: Int) -> [String] {
        if row.count >= count { return Array(row.prefix(count)) }
        return row + Array(repeating: "", count: count - row.count)
    }

    private static func normalizedAlignments(
        _ alignments: [MarbleMarkdownAlignment], count: Int
    ) -> [MarbleMarkdownAlignment] {
        if alignments.count >= count { return Array(alignments.prefix(count)) }
        return alignments + Array(repeating: .leading, count: count - alignments.count)
    }

    private static func listItem(_ line: String) -> (item: MarbleMarkdownListItem, ordered: Bool)? {
        let indent = line.prefix(while: { $0 == " " || $0 == "\t" })
            .reduce(0) { $0 + ($1 == "\t" ? 4 : 1) }
        let body = line.trimmingCharacters(in: .whitespaces)
        var marker: String
        var rest: Substring
        var ordered = false
        if let first = body.first, "-*+".contains(first), body.dropFirst().first == " " {
            marker = "•"
            rest = body.dropFirst(2)
        } else {
            let digits = body.prefix(while: \.isNumber)
            guard !digits.isEmpty, digits.count <= 9 else { return nil }
            let after = body.dropFirst(digits.count)
            guard let punctuation = after.first, punctuation == "." || punctuation == ")",
                  after.dropFirst().first == " " else { return nil }
            marker = digits + "."
            rest = after.dropFirst(2)
            ordered = true
        }
        var text = rest.trimmingCharacters(in: .whitespaces)
        var checked: Bool?
        if text.hasPrefix("[ ] ") {
            checked = false
            text = String(text.dropFirst(4))
        } else if text.lowercased().hasPrefix("[x] ") {
            checked = true
            text = String(text.dropFirst(4))
        }
        if checked != nil { marker = "" }
        return (MarbleMarkdownListItem(marker: marker, level: min(indent / 2, 4), text: text, checked: checked), ordered)
    }
}
