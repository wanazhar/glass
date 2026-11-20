import SwiftUI

/// Marble's own layer of control: posture, the pause switch, path rules, forms
/// of standing consent, and the decisions already journalled.
struct ControlView: View {
    @EnvironmentObject private var store: MarbleStore
    @Environment(\.dismiss) private var dismiss

    private var control: MarbleControl { store.control }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    if control.paused {
                        Text("Paused. New messages wait below until you resume.")
                            .font(.footnote)
                            .foregroundStyle(MarblePalette.muted)
                    }

                    posture
                    switches
                    queueSection
                    grantsSection
                    rulesSection
                    cratesSection
                    ledgerSection
                }
                .padding(20)
            }
            .background(MarbleBackdrop())
            .navigationTitle("Marble control")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") { dismiss() }
                }
            }
            .task { await store.refresh() }
            .task { await store.loadCrates() }
        }
    }

    // MARK: - Posture

    private var posture: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionTitle("Posture")
            ForEach(control.profiles) { option in
                Button {
                    Task { await store.setProfile(option.id) }
                } label: {
                    HStack(alignment: .top, spacing: 12) {
                        Image(
                            systemName: control.profile == option.id
                                ? "largecircle.fill.circle"
                                : "circle"
                        )
                        .foregroundStyle(
                            control.profile == option.id
                                ? MarblePalette.accent
                                : MarblePalette.muted
                        )
                        VStack(alignment: .leading, spacing: 3) {
                            Text(option.label)
                                .font(.callout.weight(.semibold))
                                .foregroundStyle(MarblePalette.ink)
                            Text(option.description)
                                .font(.caption)
                                .foregroundStyle(MarblePalette.muted)
                                .multilineTextAlignment(.leading)
                        }
                        Spacer(minLength: 0)
                        if !option.allowsWrites {
                            Tag("read only")
                        }
                    }
                    .padding(14)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .marbleGlass(cornerRadius: 20)
                }
                .buttonStyle(.plain)
            }
        }
    }

    private var switches: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionTitle("Admission")
            HStack(spacing: 10) {
                Button {
                    Task { await store.setPaused(!control.paused) }
                } label: {
                    Label(
                        control.paused ? "Resume queue" : "Pause queue",
                        systemImage: control.paused ? "play.fill" : "pause.fill"
                    )
                    .font(.callout.weight(.medium))
                    .frame(maxWidth: .infinity, minHeight: 44)
                }
                .marbleGlass(cornerRadius: 18)

                Button(role: .destructive) {
                    Task { await store.stopEverything() }
                } label: {
                    Label("Stop all", systemImage: "stop.fill")
                        .font(.callout.weight(.medium))
                        .frame(maxWidth: .infinity, minHeight: 44)
                }
                .marbleGlass(cornerRadius: 18)
            }
            .buttonStyle(.plain)
            Text("Pause holds new messages. Stop cancels anything running.")
                .font(.caption2)
                .foregroundStyle(MarblePalette.muted)
        }
    }

    // MARK: - Queue

    private var queueSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionTitle("Queued messages")
            if control.queue.isEmpty {
                EmptyNote("Nothing waiting.")
            } else {
                ForEach(control.queue) { entry in
                    RecordRow(
                        title: entry.text,
                        detail: entry.conversationId ?? "new conversation"
                    )
                }
            }
        }
    }

    // MARK: - Grants

    private var grantsSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionTitle("Standing grants")
            if control.grants.isEmpty {
                EmptyNote("No standing grants. Every call asks first.")
            } else {
                ForEach(control.grants) { grant in
                    HStack(spacing: 10) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(grant.tool)
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(MarblePalette.ink)
                            Text(grant.scopeLabel)
                                .font(.caption2)
                                .foregroundStyle(MarblePalette.muted)
                            Text(grant.uses == 1 ? "1 use" : "\(grant.uses) uses")
                                .font(.caption2)
                                .foregroundStyle(MarblePalette.muted)
                        }
                        Spacer(minLength: 0)
                        Button("Revoke") {
                            Task { await store.revokeGrant(grant.id) }
                        }
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(MarblePalette.danger)
                        .buttonStyle(.plain)
                    }
                    .padding(12)
                    .marbleGlass(cornerRadius: 16)
                }
            }
        }
    }

    private var rulesSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionTitle("Denied paths")
            if control.deniedPaths.isEmpty {
                EmptyNote("No denied path rules.")
            } else {
                FlowRow {
                    ForEach(control.deniedPaths, id: \.self) { rule in
                        Tag(rule)
                    }
                }
            }
        }
    }

    // MARK: - Workspace crates

    /// Control decides what may touch the workspace; this says what the
    /// workspace contains, so a grant can be scoped against the real crates.
    private var cratesSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionTitle("Workspace crates")
            if store.cratesLoading {
                ForEach(0..<3, id: \.self) { _ in
                    MarbleSkeleton(height: 44, cornerRadius: 16)
                }
            } else if let error = store.cratesError {
                EmptyNote(error)
            } else if let map = store.crates {
                Text(
                    "\(map.workspace.members) crate\(map.workspace.members == 1 ? "" : "s")"
                        + (map.workspace.marble.map { " · \($0)" } ?? "")
                        + (map.truncated ? " · truncated" : "")
                )
                .font(.caption)
                .foregroundStyle(MarblePalette.muted)
                if map.crates.isEmpty {
                    EmptyNote("The crate map is empty.")
                } else {
                    ForEach(map.crates) { entry in
                        VStack(alignment: .leading, spacing: 3) {
                            HStack(spacing: 6) {
                                Text(entry.name)
                                    .font(.caption.weight(.semibold))
                                    .foregroundStyle(MarblePalette.ink)
                                if !entry.version.isEmpty {
                                    Text(entry.version)
                                        .font(.caption2)
                                        .foregroundStyle(MarblePalette.muted)
                                }
                                Spacer(minLength: 0)
                                Tag(entry.reachLabel)
                            }
                            Text(entry.path)
                                .font(.caption2)
                                .foregroundStyle(MarblePalette.muted)
                            if !entry.description.isEmpty {
                                Text(entry.description)
                                    .font(.caption2)
                                    .foregroundStyle(MarblePalette.muted)
                            }
                        }
                        .padding(12)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .marbleGlass(cornerRadius: 16)
                    }
                }
            } else {
                EmptyNote("No crate map on this host.")
            }
        }
    }

    private var ledgerSection: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionTitle("Recent decisions")
            if control.ledger.isEmpty {
                EmptyNote("No governed decisions yet.")
            } else {
                ForEach(control.ledger.suffix(25).reversed()) { entry in
                    HStack(alignment: .top, spacing: 10) {
                        Tag(entry.decision)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(entry.tool.isEmpty ? entry.reason : entry.tool)
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(MarblePalette.ink)
                            if !entry.path.isEmpty {
                                Text(entry.path)
                                    .font(.caption2)
                                    .foregroundStyle(MarblePalette.muted)
                            }
                            if !entry.reason.isEmpty {
                                Text(entry.reason)
                                    .font(.caption2)
                                    .foregroundStyle(MarblePalette.muted)
                            }
                        }
                        Spacer(minLength: 0)
                    }
                    .padding(12)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .marbleGlass(cornerRadius: 16)
                }
            }
        }
    }
}

struct SectionTitle: View {
    var text: String

    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text.uppercased())
            .font(.caption2.weight(.semibold))
            .tracking(0.9)
            .foregroundStyle(MarblePalette.muted)
    }
}

struct Tag: View {
    var text: String

    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text)
            .font(.caption2.weight(.medium))
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .marbleGlass(cornerRadius: 999)
    }
}

struct EmptyNote: View {
    var text: String

    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text)
            .font(.caption)
            .foregroundStyle(MarblePalette.muted)
    }
}

private struct RecordRow: View {
    var title: String
    var detail: String

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.caption)
                .foregroundStyle(MarblePalette.ink)
            Text(detail)
                .font(.caption2)
                .foregroundStyle(MarblePalette.muted)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .marbleGlass(cornerRadius: 16)
    }
}

/// A row of chips that wraps onto a second line instead of pushing the last
/// tag off the edge of the sheet.
private struct FlowRow: Layout {
    var spacing: CGFloat = 6
    var lineSpacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let limit = proposal.width ?? .infinity
        var lineWidth: CGFloat = 0
        var lineHeight: CGFloat = 0
        var result = CGSize.zero
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if lineWidth > 0, lineWidth + spacing + size.width > limit {
                result.width = max(result.width, lineWidth)
                result.height += lineHeight + lineSpacing
                lineWidth = 0
                lineHeight = 0
            }
            lineWidth += (lineWidth > 0 ? spacing : 0) + size.width
            lineHeight = max(lineHeight, size.height)
        }
        result.width = max(result.width, lineWidth)
        result.height += lineHeight
        return result
    }

    func placeSubviews(
        in bounds: CGRect,
        proposal: ProposedViewSize,
        subviews: Subviews,
        cache: inout ()
    ) {
        var x = bounds.minX
        var y = bounds.minY
        var lineHeight: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x > bounds.minX, x + size.width > bounds.maxX {
                x = bounds.minX
                y += lineHeight + lineSpacing
                lineHeight = 0
            }
            subview.place(
                at: CGPoint(x: x, y: y),
                anchor: .topLeading,
                proposal: ProposedViewSize(size)
            )
            x += size.width + spacing
            lineHeight = max(lineHeight, size.height)
        }
    }
}
