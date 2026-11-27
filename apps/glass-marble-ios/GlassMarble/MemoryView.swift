import SwiftUI

struct MemoryView: View {
    @EnvironmentObject private var store: MarbleStore
    var marbleId: String?
    @State private var draft = ""
    @State private var editing: MarbleMemoryFact?
    @State private var forgetting: MarbleMemoryFact?

    var body: some View {
        List {
            if let marbleId {
                factSection(owner: marbleId, facts: store.memory.facts.filter { $0.owner == marbleId })
            } else {
                ForEach(store.prism.marbles.filter { marble in
                    store.memory.facts.contains { $0.owner == marble.id }
                }) { marble in
                    factSection(owner: marble.id, facts: store.memory.facts.filter { $0.owner == marble.id })
                }
            }
            factSection(owner: "", facts: store.memory.facts.filter { $0.owner.isEmpty })
            Section("Suggestions · Not saved") {
                let suggestions = store.memory.suggestions.filter {
                    marbleId == nil || store.owner(of: $0) == marbleId
                }
                if suggestions.isEmpty {
                    Text("No suggestions waiting.").foregroundStyle(MarbleTheme.muted)
                }
                ForEach(suggestions) { suggestion in
                    SuggestionCard(suggestion: suggestion)
                }
            }
            .listRowBackground(MarbleTheme.card)
            Section(marbleId.map { store.memoryScope($0) } ?? "Add a shared memory") {
                TextField("What should Marble remember?", text: $draft, axis: .vertical)
                Button("Save memory") {
                    let text = draft
                    draft = ""
                    Task { await store.remember(text, owner: marbleId ?? "") }
                }
                .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
            .listRowBackground(MarbleTheme.card)
        }
        .scrollContentBackground(.hidden)
        .background(MarbleTheme.paper)
        .navigationTitle("Memory")
        .task { await store.refresh() }
        .sheet(item: $editing) { fact in
            MemoryDecisionView(title: "Edit memory", scope: store.memoryScope(fact.owner), text: fact.text, editable: true) {
                await store.update(fact, text: $0)
            }
        }
        .confirmationDialog("Forget this memory?", isPresented: Binding(
            get: { forgetting != nil }, set: { if !$0 { forgetting = nil } }
        ), titleVisibility: .visible, presenting: forgetting) { fact in
            Button("Forget", role: .destructive) { Task { await store.forget(fact) } }
        } message: { fact in
            Text(store.memoryScope(fact.owner) + " · " + fact.text)
        }
    }

    private func factSection(owner: String, facts: [MarbleMemoryFact]) -> some View {
        Section(store.memoryScope(owner)) {
            if facts.isEmpty {
                Text("Nothing remembered yet.").foregroundStyle(MarbleTheme.muted)
            }
            ForEach(facts) { fact in
                Button {
                    editing = fact
                } label: {
                    HStack(alignment: .firstTextBaseline) {
                        Text(fact.text).foregroundStyle(MarbleTheme.ink)
                        Spacer()
                        Image(systemName: "pencil").foregroundStyle(MarbleTheme.tint)
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityHint("Edit memory")
                .swipeActions {
                    Button("Forget", role: .destructive) { forgetting = fact }
                }
            }
        }
        .listRowBackground(MarbleTheme.card)
    }
}

private struct MemoryDecisionView: View {
    @EnvironmentObject private var store: MarbleStore
    @Environment(\.dismiss) private var dismiss
    let title: String
    let scope: String
    @State var text: String
    var editable = false
    var save: (String) async -> Void
    @State private var saving = false

    var body: some View {
        NavigationStack {
            Form {
                Section("Who can use this") { Text(scope) }
                    .listRowBackground(MarbleTheme.card)
                Section("Memory") {
                    if editable {
                        TextField("Memory", text: $text, axis: .vertical).lineLimit(3...9)
                    } else {
                        Text(text)
                    }
                }
                .listRowBackground(MarbleTheme.card)
                if let error = store.errorMessage {
                    Text(error).foregroundStyle(MarbleTheme.problem)
                }
            }
            .scrollContentBackground(.hidden)
            .background(MarbleTheme.paper)
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        saving = true
                        Task {
                            await save(text)
                            saving = false
                            if store.errorMessage == nil { dismiss() }
                        }
                    }
                    .disabled(saving || text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
        }
    }
}

struct SuggestionCard: View {
    @EnvironmentObject private var store: MarbleStore
    var suggestion: MarbleMemorySuggestion
    @State private var review = false

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(store.memoryScope(store.owner(of: suggestion)))
                .font(.caption)
                .foregroundStyle(MarbleTheme.muted)
            Text(suggestion.text)
                .fixedSize(horizontal: false, vertical: true)
            ViewThatFits {
                HStack { actions }
                VStack(alignment: .leading) { actions }
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(MarbleTheme.card, in: RoundedRectangle(cornerRadius: 14))
        .sheet(isPresented: $review) {
            MemoryDecisionView(title: "Review suggestion", scope: store.memoryScope(store.owner(of: suggestion)),
                               text: suggestion.text) { _ in await store.accept(suggestion) }
        }
    }

    @ViewBuilder private var actions: some View {
        Button("Not now") { Task { await store.dismiss(suggestion) } }
            .buttonStyle(.glass)
        Button("Review") { review = true }
            .buttonStyle(.glassProminent)
    }
}

extension MarbleStore {
    func owner(of suggestion: MarbleMemorySuggestion) -> String {
        guard let id = suggestion.conversationId, !id.isEmpty else { return "marble" }
        return prism.marbles.first { $0.conversationId == id }?.id ?? "marble"
    }

    func memoryScope(_ owner: String) -> String {
        if owner.isEmpty { return "Shared with everyone" }
        return "Only " + (prism.marbles.first { $0.id == owner }?.name ?? owner)
    }
}
