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
                        Text(MarbleDecisionWords.action(tool: approval.tool))
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
                if !runningTasks.isEmpty {
                    Button(role: .destructive) {
                        confirmStop = true
                    } label: {
                        Label("Stop everything", systemImage: "stop.circle")
                            .foregroundStyle(.red)
                    }
                }
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
                    NavigationLink {
                        DecisionDetailView(entry: entry, chat: entry.conversationId.isEmpty ? nil : chatTitle(entry.conversationId))
                    } label: {
                        DecisionRow(entry: entry)
                    }
                }
            }
        }
        .navigationTitle("Activity")
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

/// One recent decision in plain words: what Marble wanted, how it was settled.
private struct DecisionRow: View {
    var entry: MarbleControlEntry

    private var wentAhead: Bool { MarbleDecisionWords.wentAhead(decision: entry.decision) }

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: wentAhead ? "checkmark.circle.fill" : "xmark.circle.fill")
                .foregroundStyle(wentAhead ? Color.green : Color.secondary)
                .imageScale(.large)
            VStack(alignment: .leading, spacing: 2) {
                Text(MarbleDecisionWords.action(tool: entry.tool))
                    .font(.callout)
                    .lineLimit(1)
                Text(MarbleDecisionWords.outcome(decision: entry.decision, reason: entry.reason)
                     + " · " + decisionDate(entry.at).formatted(.relative(presentation: .named)))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

/// The full record behind a decision row, with the host's own words.
private struct DecisionDetailView: View {
    var entry: MarbleControlEntry
    var chat: String?

    var body: some View {
        Form {
            Section {
                LabeledContent("What", value: MarbleDecisionWords.action(tool: entry.tool))
                LabeledContent("Outcome", value: MarbleDecisionWords.outcome(decision: entry.decision, reason: entry.reason))
                LabeledContent("When", value: decisionDate(entry.at).formatted(date: .abbreviated, time: .shortened))
                if let chat { LabeledContent("Chat", value: chat) }
                if !entry.path.isEmpty { LabeledContent("Path", value: entry.path) }
            }
            Section {
                if !entry.reason.isEmpty { detail("Host's reason", entry.reason) }
                detail("Tool", entry.tool)
                detail("Decision", entry.decision)
                if !entry.taskId.isEmpty { detail("Task", entry.taskId) }
            } header: {
                Text("Details")
            }
        }
        .navigationTitle("Decision")
        .navigationBarTitleDisplayMode(.inline)
    }

    private func detail(_ label: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label)
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(value)
                .font(.callout.monospaced())
                .textSelection(.enabled)
        }
    }
}

private func decisionDate(_ at: UInt64) -> Date {
    Date(timeIntervalSince1970: Double(at) / 1000)
}
