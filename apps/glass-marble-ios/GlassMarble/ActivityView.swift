import SwiftUI

/// What Marble is doing and what it needs: pending approvals, memory it wants
/// to keep, work running right now, and the recent decisions it already made.
struct ActivityView: View {
    @EnvironmentObject private var store: MarbleStore
    @State private var confirmStop = false

    var body: some View {
        List {
            Section(header: Text("Needs you")) {
                if store.state.approvals.isEmpty && store.memory.suggestions.isEmpty {
                    Text("Nothing is waiting on you.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                ForEach(store.state.approvals) { approval in
                    VStack(alignment: .leading, spacing: 8) {
                        Text(approval.tool)
                            .font(.callout.weight(.semibold))
                        Text(approval.argumentsSummary)
                            .font(.caption.monospaced())
                            .foregroundStyle(.secondary)
                            .lineLimit(3)
                        HStack(spacing: 12) {
                            Button("Deny", role: .destructive) {
                                Task { await store.decide(approval, allow: false) }
                            }
                            .buttonStyle(.bordered)
                            Spacer()
                            Button("Always allow") {
                                Task { await store.decide(approval, allow: true, remember: true) }
                            }
                            .buttonStyle(.bordered)
                            Button("Allow") {
                                Task { await store.decide(approval, allow: true) }
                            }
                            .buttonStyle(.borderedProminent)
                        }
                    }
                    .padding(.vertical, 4)
                }
                ForEach(store.memory.suggestions) { suggestion in
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Marble wants to remember")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        Text(suggestion.text)
                            .font(.callout)
                        HStack(spacing: 12) {
                            Button("Dismiss") {
                                Task { await store.dismiss(suggestion) }
                            }
                            .buttonStyle(.bordered)
                            Spacer()
                            Button("Keep") {
                                Task { await store.accept(suggestion) }
                            }
                            .buttonStyle(.borderedProminent)
                        }
                    }
                    .padding(.vertical, 4)
                }
            }

            Section(header: Text("Running now")) {
                if runningTasks.isEmpty {
                    Text("Nothing is running.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                ForEach(runningTasks) { task in
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(task.prompt.isEmpty ? task.id : task.prompt)
                                .font(.callout)
                                .lineLimit(2)
                            Text(chatTitle(task.conversationId))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button("Stop", role: .destructive) {
                            Task { await store.cancel(task) }
                        }
                        .buttonStyle(.bordered)
                    }
                }
            }

            Section {
                Toggle(isOn: pausedBinding) {
                    Text("Pause new work")
                }
            } footer: {
                Text("New messages wait until you resume. What is already running keeps going.")
            }

            Section(header: Text("Recent decisions")) {
                if store.control.ledger.isEmpty {
                    Text("Marble has not asked to do anything yet.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                ForEach(store.control.ledger.suffix(20).reversed()) { entry in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(entry.tool + (entry.path.isEmpty ? "" : " · " + entry.path))
                            .font(.callout)
                            .lineLimit(1)
                        Text(entry.decision + (entry.reason.isEmpty ? "" : " · " + entry.reason))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }
        .navigationTitle("Activity")
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button("Stop everything", role: .destructive) {
                    confirmStop = true
                }
            }
        }
        .confirmationDialog(
            "Stop everything?",
            isPresented: $confirmStop,
            titleVisibility: .visible
        ) {
            Button("Stop everything", role: .destructive) {
                Task { await store.stopEverything() }
            }
        } message: {
            Text("Every running turn is cancelled.")
        }
    }

    private var runningTasks: [MarbleTask] {
        store.state.tasks.filter { $0.isRunning }
    }

    private func chatTitle(_ id: String) -> String {
        store.state.conversation(id)?.title ?? id
    }

    private var pausedBinding: Binding<Bool> {
        Binding(
            get: { store.control.paused },
            set: { paused in Task { await store.setPaused(paused) } }
        )
    }
}
