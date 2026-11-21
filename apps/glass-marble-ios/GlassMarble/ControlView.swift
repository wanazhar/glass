import SwiftUI

/// Marble's own layer of control: posture, the pause switch, path rules, forms
/// of standing consent, and the decisions already journalled.
struct ControlView: View {
    @EnvironmentObject private var store: MarbleStore
    @Environment(\.dismiss) private var dismiss

    private var control: MarbleControl { store.control }

    var body: some View {
        NavigationStack {
            Form {
                if control.paused {
                    Section {
                        Text("Paused. New messages wait below until you resume.")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                }

                Section("Posture") {
                    ForEach(control.profiles) { option in
                        Button {
                            Task { await store.setProfile(option.id) }
                        } label: {
                            HStack(alignment: .top) {
                                VStack(alignment: .leading, spacing: 3) {
                                    Text(option.label)
                                        .foregroundStyle(.primary)
                                    Text(option.description)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                        .multilineTextAlignment(.leading)
                                    if !option.allowsWrites {
                                        Text("Read only")
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                    }
                                }
                                Spacer(minLength: 8)
                                if control.profile == option.id {
                                    Image(systemName: "checkmark")
                                        .foregroundStyle(.tint)
                                }
                            }
                        }
                    }
                }

                Section {
                    Button {
                        Task { await store.setPaused(!control.paused) }
                    } label: {
                        Label(
                            control.paused ? "Resume queue" : "Pause queue",
                            systemImage: control.paused ? "play.fill" : "pause.fill"
                        )
                    }
                    Button(role: .destructive) {
                        Task { await store.stopEverything() }
                    } label: {
                        Label("Stop all", systemImage: "stop.fill")
                    }
                } header: {
                    Text("Admission")
                } footer: {
                    Text("Pause holds new messages. Stop cancels anything running.")
                }

                Section("Queued messages") {
                    if control.queue.isEmpty {
                        Text("Nothing waiting.")
                            .foregroundStyle(.secondary)
                    } else {
                        ForEach(control.queue) { entry in
                            VStack(alignment: .leading, spacing: 2) {
                                Text(entry.text)
                                Text(entry.conversationId ?? "new conversation")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                }

                Section("Standing grants") {
                    if control.grants.isEmpty {
                        Text("No standing grants. Every call asks first.")
                            .foregroundStyle(.secondary)
                    } else {
                        ForEach(control.grants) { grant in
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(grant.tool)
                                    Text("\(grant.scopeLabel) · \(grant.uses == 1 ? "1 use" : "\(grant.uses) uses")")
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                                Spacer(minLength: 8)
                                Button("Revoke", role: .destructive) {
                                    Task { await store.revokeGrant(grant.id) }
                                }
                                .buttonStyle(.borderless)
                            }
                        }
                    }
                }

                Section("Denied paths") {
                    if control.deniedPaths.isEmpty {
                        Text("No denied path rules.")
                            .foregroundStyle(.secondary)
                    } else {
                        ForEach(control.deniedPaths, id: \.self) { rule in
                            Text(rule)
                                .font(.footnote.monospaced())
                        }
                    }
                }

                Section("Workspace crates") {
                    cratesRows
                }

                Section("Recent decisions") {
                    if control.ledger.isEmpty {
                        Text("No governed decisions yet.")
                            .foregroundStyle(.secondary)
                    } else {
                        ForEach(control.ledger.suffix(25).reversed()) { entry in
                            VStack(alignment: .leading, spacing: 2) {
                                Text(entry.tool.isEmpty ? entry.reason : entry.tool)
                                if !entry.path.isEmpty {
                                    Text(entry.path)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                                Text([entry.decision, entry.reason]
                                    .filter { !$0.isEmpty }
                                    .joined(separator: " · "))
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                    }
                }
            }
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

    /// Control decides what may touch the workspace; this says what the
    /// workspace contains, so a grant can be scoped against the real crates.
    @ViewBuilder
    private var cratesRows: some View {
        if store.cratesLoading {
            Text("Loading the crate map…")
                .foregroundStyle(.secondary)
                .redacted(reason: .placeholder)
        } else if let error = store.cratesError {
            Text(error)
                .foregroundStyle(.red)
        } else if let map = store.crates {
            Text(
                "\(map.workspace.members) crate\(map.workspace.members == 1 ? "" : "s")"
                    + (map.workspace.marble.map { " · \($0)" } ?? "")
                    + (map.truncated ? " · truncated" : "")
            )
            .font(.caption)
            .foregroundStyle(.secondary)
            if map.crates.isEmpty {
                Text("The crate map is empty.")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(map.crates) { entry in
                    VStack(alignment: .leading, spacing: 2) {
                        HStack {
                            Text(entry.name)
                            if !entry.version.isEmpty {
                                Text(entry.version)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer(minLength: 4)
                            Text(entry.reachLabel)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Text(entry.path)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        if !entry.description.isEmpty {
                            Text(entry.description)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }
        } else {
            Text("No crate map on this host.")
                .foregroundStyle(.secondary)
        }
    }
}
