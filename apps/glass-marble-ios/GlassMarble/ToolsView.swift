import SwiftUI

/// Everything this marble can reach for, split by who owns the call.
///
/// Core executes its own coding tools inside the session; Marble shows what it
/// has seen them do but cannot approve them. The `glass.*` integrations are the
/// opposite: Marble classifies each one through the control layer before it
/// runs. Listing both is the difference between a capability claim and a record.
struct WhatMarbleCanUseView: View {
    @EnvironmentObject private var store: MarbleStore
    @Environment(\.dismiss) private var dismiss

    var body: some View {
            List {
                // A sheet covers the chat's error strip, so a failed load
                // has to say so here.
                if let error = store.errorMessage {
                    Section {
                        Text(error)
                            .font(.footnote)
                            .foregroundStyle(.red)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }

                Section(header: Text("Tools this host has used")) {
                    if store.toolInventory.observed.isEmpty {
                        Text("No Core tool has run on this host yet.")
                            .foregroundStyle(.secondary)
                    }
                    ForEach(store.toolInventory.observed) { tool in
                        ToolRow(
                            tag: "core",
                            title: tool.name,
                            detail: "\(tool.runs) run\(tool.runs == 1 ? "" : "s")"
                                + (tool.errors > 0 ? " · \(tool.errors) failed" : ""),
                            tone: .secondary
                        )
                    }
                }

                Section(header: Text("Workspace actions Marble manages")) {
                    if store.toolInventory.governed.isEmpty {
                        Text("No workspace actions are registered.")
                            .foregroundStyle(.secondary)
                    }
                    ForEach(store.toolInventory.governed) { tool in
                        ToolRow(
                            tag: tool.effect,
                            title: tool.name,
                            detail: tool.description,
                            tone: tool.effect == "write" ? .red : .green
                        )
                    }
                }
            }
            .refreshable { await store.loadTools() }
            .navigationTitle("What Marble can use")
            .navigationBarTitleDisplayMode(.inline)
            
            .task { await store.loadTools() }
    }
}

private struct ToolRow: View {
    var tag: String
    var title: String
    var detail: String
    var tone: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            MarbleAdaptiveRow(spacing: 4) {
                Text(tag)
                    .font(.footnote)
                    .foregroundStyle(tone)
                Text(title)
                    .font(.subheadline.monospaced())
                    .fixedSize(horizontal: false, vertical: true)
            }
            if !detail.isEmpty {
                Text(detail)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}
