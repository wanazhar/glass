import SwiftUI

/// What Marble remembers about you, and what it would like permission to keep.
struct MemoryView: View {
    @EnvironmentObject private var store: MarbleStore
    @Environment(\.dismiss) private var dismiss
    @State private var draft = ""
    @State private var editing: MarbleMemoryFact?
    @State private var editDraft = ""

    var body: some View {
            Form {
                Section {
                    TextField("Remember that…", text: $draft, axis: .vertical)
                        .lineLimit(1...4)
                    Button("Save") {
                        let text = draft
                        draft = ""
                        Task { await store.remember(text) }
                    }
                    .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }

                Section(header: Text("Shared with every Marble")) {
                    if sharedFacts.isEmpty && ownedFacts.isEmpty {
                        Text("Nothing remembered yet.")
                            .foregroundStyle(.secondary)
                    }
                    ForEach(sharedFacts) { fact in
                        factRow(fact)
                    }
                }

                if !ownedFacts.isEmpty {
                    Section(header: Text("By Marble")) {
                        ForEach(ownedGroups, id: \.owner) { group in
                            Section(header: Text(group.name)) {
                                ForEach(group.facts) { fact in
                                    factRow(fact)
                                }
                            }
                        }
                    }
                }

                Section(header: Text("Suggested")) {
                    if store.memory.suggestions.isEmpty {
                        Text("No suggestions waiting.")
                            .foregroundStyle(.secondary)
                    }
                    ForEach(store.memory.suggestions) { suggestion in
                        SuggestionCard(suggestion: suggestion) { keep in
                            Task {
                                if keep {
                                    await store.accept(suggestion)
                                } else {
                                    await store.dismiss(suggestion)
                                }
                            }
                        }
                    }
                }

                Section {
                    Button {
                        Task { await store.toggleVoice() }
                    } label: {
                        Label(
                            store.voice.enabled ? "Stop reading replies" : "Read replies aloud",
                            systemImage: store.voice.enabled ? "speaker.slash" : "speaker.wave.2"
                        )
                    }
                } header: {
                    Text("Voice and reach")
                } footer: {
                    Text("Speech stays on this device.")
                }

                Section {
                    Label(
                        store.notificationState.apnsConfigured
                            ? "Push is configured. This phone registered when it connected."
                            : "No push yet. Add an APNs key in the host's Control panel.",
                        systemImage: store.notificationState.apnsConfigured
                            ? "bell.badge"
                            : "bell.slash"
                    )
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                }
            }
            .navigationTitle("Memory")
            .navigationBarTitleDisplayMode(.inline)
            
            .task { await store.refresh() }
            .alert("Edit memory", isPresented: editingBinding) {
                TextField("Memory", text: $editDraft, axis: .vertical)
                Button("Cancel", role: .cancel) { editing = nil }
                Button("Save") {
                    guard let fact = editing else { return }
                    let text = editDraft
                    editing = nil
                    Task { await store.update(fact, text: text) }
                }
            }
    }

    private var sharedFacts: [MarbleMemoryFact] {
        store.memory.facts.filter { $0.owner.isEmpty }
    }

    private var ownedFacts: [MarbleMemoryFact] {
        store.memory.facts.filter { !$0.owner.isEmpty }
    }

    /// Owner id → (name, facts), in roster order so groups read like the
    /// Marbles tab.
    private var ownedGroups: [(owner: String, name: String, facts: [MarbleMemoryFact])] {
        var order: [String] = []
        var grouped: [String: [MarbleMemoryFact]] = [:]
        for fact in ownedFacts {
            if grouped[fact.owner] == nil { order.append(fact.owner) }
            grouped[fact.owner, default: []].append(fact)
        }
        return order.map { owner in
            let name = store.prism.marbles.first { $0.id == owner }?.name ?? owner
            return (owner, name, grouped[owner] ?? [])
        }
    }

    @ViewBuilder
    private func factRow(_ fact: MarbleMemoryFact) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(fact.text)
                .fixedSize(horizontal: false, vertical: true)
            HStack {
                Text(fact.kind)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                Button("Edit") {
                    editing = fact
                    editDraft = fact.text
                }
                .buttonStyle(.borderless)
                Button("Forget", role: .destructive) {
                    Task { await store.forget(fact) }
                }
                .buttonStyle(.borderless)
            }
        }
    }

    private var editingBinding: Binding<Bool> {
        Binding(
            get: { editing != nil },
            set: { presented in
                if !presented { editing = nil }
            }
        )
    }
}

/// One memory Marble noticed, waiting for a decision. Used in the Memory
/// sheet and in the horizontal rail above the transcript.
struct SuggestionCard: View {
    var suggestion: MarbleMemorySuggestion
    var onDecision: (Bool) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Remember this?")
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(suggestion.text)
                .font(.callout)
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 10) {
                Button("Not now") { onDecision(false) }
                    .buttonStyle(.bordered)
                Spacer()
                Button("Save") { onDecision(true) }
                    .buttonStyle(.borderedProminent)
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            Color(.secondarySystemGroupedBackground),
            in: RoundedRectangle(cornerRadius: 12, style: .continuous)
        )
    }
}
