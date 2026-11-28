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
                    ApprovalCard(approval: approval) { allow, remember in
                        Task { await store.decide(approval, allow: allow, remember: remember) }
                    }
                    .padding(.vertical, 4)
                }
                ForEach(store.memory.suggestions) { suggestion in
                    SuggestionCard(suggestion: suggestion)
                }
            }

            Section(header: Text("Running now")) {
                if !runningTasks.isEmpty {
                    Button(role: .destructive) {
                        confirmStop = true
                    } label: {
                        Label("Stop everything", systemImage: "stop.circle")
                            .foregroundStyle(.red)
                            .frame(minHeight: 44)
                    }
                    .disabled(store.changingHostPause)
                }
                if runningTasks.isEmpty {
                    Text("Nothing is running.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                ForEach(runningTasks) { task in
                    MarbleAdaptiveRow {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(task.prompt.isEmpty ? task.id : task.prompt)
                                .font(.subheadline)
                            Text(chatTitle(task.conversationId))
                                .font(.footnote)
                                .foregroundStyle(MarbleTheme.muted)
                        }
                        .fixedSize(horizontal: false, vertical: true)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        MarbleInlineButton(title: "Stop", role: .destructive) {
                            Task { await store.cancel(task) }
                        }
                    }
                }
            }

            Section {
                Toggle(isOn: pausedBinding) {
                    Text("Pause new work")
                }
                .disabled(store.changingHostPause)
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
        .scrollContentBackground(.hidden)
        .background(MarbleTheme.paper)
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
            Text("This cancels every running turn and pauses the host. New messages wait until you resume.")
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
                    .font(.subheadline)
                Text(MarbleDecisionWords.outcome(decision: entry.decision, reason: entry.reason)
                     + " · " + decisionDate(entry.at).formatted(.relative(presentation: .named)))
                    .font(.footnote)
                    .foregroundStyle(MarbleTheme.muted)
            }
            .fixedSize(horizontal: false, vertical: true)
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
                MarbleValueRow("What", value: MarbleDecisionWords.action(tool: entry.tool))
                MarbleValueRow("Outcome", value: MarbleDecisionWords.outcome(decision: entry.decision, reason: entry.reason))
                MarbleValueRow("When", value: decisionDate(entry.at).formatted(date: .abbreviated, time: .shortened))
                if let chat { MarbleValueRow("Chat", value: chat) }
                if !entry.path.isEmpty { MarbleValueRow("Path", value: entry.path) }
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
                .font(.footnote)
                .foregroundStyle(.secondary)
            Text(value)
                .font(.subheadline.monospaced())
                .fixedSize(horizontal: false, vertical: true)
                .textSelection(.enabled)
        }
    }
}

private func decisionDate(_ at: UInt64) -> Date {
    Date(timeIntervalSince1970: Double(at) / 1000)
}
