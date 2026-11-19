import SwiftUI

/// Everything this marble can reach for, split by who owns the call.
///
/// Core executes its own coding tools inside the session; Marble shows what it
/// has seen them do but cannot approve them. The `glass.*` integrations are the
/// opposite: Marble classifies each one through the control layer before it
/// runs. Listing both is the difference between a capability claim and a record.
struct ToolsView: View {
    @EnvironmentObject private var store: MarbleStore
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    Text(
                        "Core owns the coding tools and runs them inside the session, so they appear here after the fact. Marble governs the integrations below under the \(store.toolInventory.profile ?? store.control.profile) posture, and only those ask you first."
                    )
                    .font(.footnote)
                    .foregroundStyle(MarblePalette.muted)

                    // A sheet covers the chat's error strip, so a failed load
                    // has to say so here.
                    if let error = store.errorMessage {
                        Text(error)
                            .font(.caption)
                            .foregroundStyle(MarblePalette.danger)
                            .fixedSize(horizontal: false, vertical: true)
                    }

                    section(
                        "Core tools this host has used",
                        empty: "No Core tool has run on this host yet.",
                        rows: store.toolInventory.observed.map { tool in
                            Row(
                                tag: "core",
                                title: tool.name,
                                detail: "\(tool.runs) run\(tool.runs == 1 ? "" : "s")"
                                    + (tool.errors > 0 ? " · \(tool.errors) failed" : ""),
                                tone: MarblePalette.muted
                            )
                        }
                    )

                    section(
                        "Governed by Marble",
                        empty: "No governed integrations are registered.",
                        rows: store.toolInventory.governed.map { tool in
                            Row(
                                tag: tool.effect,
                                title: tool.name,
                                detail: tool.description,
                                tone: tool.effect == "write"
                                    ? MarblePalette.danger
                                    : MarblePalette.success
                            )
                        }
                    )
                }
                .padding(20)
            }
            .background(MarbleBackdrop())
            .refreshable { await store.loadTools() }
            .navigationTitle("Tooling")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") { dismiss() }
                }
            }
            .task { await store.loadTools() }
        }
    }

    private struct Row: Identifiable {
        var tag: String
        var title: String
        var detail: String
        var tone: Color

        var id: String { tag + title + detail }
    }

    private func section(_ title: String, empty: String, rows: [Row]) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title)
                .font(.caption2.weight(.semibold))
                .textCase(.uppercase)
                .tracking(0.9)
                .foregroundStyle(MarblePalette.muted)
            if rows.isEmpty {
                Text(empty)
                    .font(.caption)
                    .foregroundStyle(MarblePalette.muted)
            }
            // Two tools can share a name across owners, so the row's position
            // is the stable identity rather than its text.
            ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 8) {
                        Text(row.tag)
                            .font(.caption2.weight(.semibold))
                            .textCase(.uppercase)
                            .foregroundStyle(row.tone)
                            .padding(.horizontal, 7)
                            .padding(.vertical, 2)
                            .overlay(
                                Capsule().stroke(row.tone.opacity(0.45), lineWidth: 0.8)
                            )
                        Text(row.title)
                            .font(.caption.monospaced())
                            .foregroundStyle(MarblePalette.ink)
                        Spacer(minLength: 0)
                    }
                    if !row.detail.isEmpty {
                        Text(row.detail)
                            .font(.caption2)
                            .foregroundStyle(MarblePalette.muted)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .marbleGlass(cornerRadius: 16)
            }
        }
    }
}
