import SwiftUI

struct LensPatch {
    enum Kind: Equatable { case hunk, context, addition, removal, note }
    struct Row: Equatable {
        let old: Int?
        let new: Int?
        let text: String
        let kind: Kind
    }

    let rows: [Row]
    let added: Int
    let removed: Int

    init(_ patch: String) {
        let header = /@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/
        var result: [Row] = []
        var old = 0, new = 0, oldRemaining = 0, newRemaining = 0
        var additions = 0, removals = 0
        for line in patch.split(separator: "\n", omittingEmptySubsequences: false) {
            if line.hasPrefix("@@ "), let match = line.prefixMatch(of: header) {
                old = Int(match.1) ?? 0
                new = Int(match.3) ?? 0
                oldRemaining = match.2.flatMap { Int($0) } ?? 1
                newRemaining = match.4.flatMap { Int($0) } ?? 1
                result.append(Row(old: nil, new: nil, text: String(line), kind: .hunk))
                continue
            }
            if line.hasPrefix("\\ "), !result.isEmpty {
                result.append(Row(old: nil, new: nil, text: String(line.dropFirst(2)), kind: .note))
                continue
            }
            guard oldRemaining > 0 || newRemaining > 0, let sign = line.first else { continue }
            let text = String(line.dropFirst())
            switch sign {
            case "+" where newRemaining > 0:
                result.append(Row(old: nil, new: new, text: text, kind: .addition))
                new += 1
                newRemaining -= 1
                additions += 1
            case "-" where oldRemaining > 0:
                result.append(Row(old: old, new: nil, text: text, kind: .removal))
                old += 1
                oldRemaining -= 1
                removals += 1
            case " " where oldRemaining > 0 && newRemaining > 0:
                result.append(Row(old: old, new: new, text: text, kind: .context))
                old += 1
                new += 1
                oldRemaining -= 1
                newRemaining -= 1
            default: break
            }
        }
        rows = result
        added = additions
        removed = removals
    }
}

struct PatchCounts: View {
    let added: Int
    let removed: Int

    var body: some View {
        HStack(spacing: Design.gap) {
            Text("+\(added)").foregroundStyle(Design.success)
            Text("−\(removed)").foregroundStyle(Design.failure)
        }
        .font(.subheadline.monospacedDigit().weight(.semibold))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(added) lines added, \(removed) lines removed")
    }
}

struct PatchRows: View {
    let patch: LensPatch
    @ScaledMetric(relativeTo: .caption) private var gutter: CGFloat = 36

    var body: some View {
        LazyVStack(alignment: .leading, spacing: 1) {
            HStack(spacing: Design.gap) {
                Text("Old").frame(width: gutter, alignment: .trailing)
                Text("New").frame(width: gutter, alignment: .trailing)
            }
            .foregroundStyle(Design.muted)
            .padding(.horizontal, Design.gap)
            ForEach(Array(patch.rows.enumerated()), id: \.offset) { _, row in
                let tint = row.kind == .addition ? Design.success : row.kind == .removal ? Design.failure : Design.ink
                if row.kind == .hunk || row.kind == .note {
                    Text(row.text).foregroundStyle(Design.muted)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, Design.gap)
                        .padding(.vertical, 5)
                        .background(Design.terminal)
                } else {
                    HStack(alignment: .firstTextBaseline, spacing: Design.gap) {
                        Text(row.old.map { String($0) } ?? "")
                            .frame(width: gutter, alignment: .trailing)
                            .foregroundStyle(Design.muted)
                        Text(row.new.map { String($0) } ?? "")
                            .frame(width: gutter, alignment: .trailing)
                            .foregroundStyle(Design.muted)
                        Text(row.kind == .addition ? "+" : row.kind == .removal ? "−" : " ")
                            .foregroundStyle(tint)
                        Text(row.text.isEmpty ? " " : row.text)
                            .foregroundStyle(tint)
                            .fixedSize(horizontal: false, vertical: true)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .padding(.horizontal, Design.gap)
                    .padding(.vertical, 5)
                    .background(row.kind == .context ? .clear : tint.opacity(0.10))
                    .accessibilityElement(children: .combine)
                }
            }
        }
        .font(.caption.monospaced())
    }
}
