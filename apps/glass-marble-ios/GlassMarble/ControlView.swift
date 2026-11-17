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
                VStack(alignment: .leading, spacing: 24) {
                    Text(
                        control.paused
                            ? "Admission is paused. New messages wait below until you resume."
                            : "Marble decides which governed tools may run before Core gets an answer. Nothing here is inherited from the engine."
                    )
                    .font(.footnote)
                    .foregroundStyle(MarblePalette.muted)

                    posture
                    switches
                    queueSection
                    grantsSection
                    rulesSection
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
            Text("Pause holds new messages without losing them. Stop pauses admission and cancels anything running.")
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
                EmptyNote("No standing grants. Every governed call asks first.")
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
                        .buttonStyle(.bordered)
                        .tint(MarblePalette.danger)
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
            Text("Denied paths are refused whatever the posture or approval says.")
                .font(.caption2)
                .foregroundStyle(MarblePalette.muted)
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

private struct SectionTitle: View {
    var text: String

    init(_ text: String) { self.text = text }

    var body: some View {
        Text(text.uppercased())
            .font(.caption2.weight(.semibold))
            .tracking(0.9)
            .foregroundStyle(MarblePalette.muted)
    }
}

private struct Tag: View {
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

private struct EmptyNote: View {
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

/// Simple wrapping row for chips.
private struct FlowRow<Content: View>: View {
    @ViewBuilder var content: Content

    var body: some View {
        HStack(spacing: 6) {
            content
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
