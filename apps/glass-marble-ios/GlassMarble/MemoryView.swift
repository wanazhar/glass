import SwiftUI

struct MemoryView: View {
    @EnvironmentObject private var store: MarbleStore
    var marbleId: String?
    @State private var draft = ""
    @State private var editing: MarbleMemoryFact?
    @State private var forgetting: MarbleMemoryFact?
    @State private var skill: MarbleSkill?

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
            Section("Skills") {
                let skills = (store.memory.skills ?? []).filter { marbleId == nil || $0.owner == marbleId || $0.sharedWith.contains(marbleId ?? "") }
                if skills.isEmpty {
                    Text("No skills saved yet.").foregroundStyle(MarbleTheme.muted)
                }
                ForEach(skills) { item in
                    Button { skill = item } label: {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(item.name).foregroundStyle(MarbleTheme.ink)
                            Text(item.whenToUse).font(.subheadline).foregroundStyle(MarbleTheme.muted)
                            if let teacher = item.learnedFrom {
                                Text("Learned from \(store.knowledgeAgentName(teacher)), \(knowledgeDate(item.createdAt))")
                                    .font(.caption).foregroundStyle(MarbleTheme.muted)
                            }
                        }
                        .frame(minHeight: 44, alignment: .leading)
                    }
                }
            }
            .listRowBackground(MarbleTheme.card)
            Section {
                NavigationLink("Recently Deleted") { RecentlyDeletedView(owner: marbleId) }
            }
            .listRowBackground(MarbleTheme.card)
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
                TextField("What should \(marbleId.map(store.knowledgeAgentName) ?? "Marble") remember?", text: $draft, axis: .vertical)
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
        .navigationTitle("Memory & Skills")
        .task { await store.refresh() }
        .sheet(item: $editing) { fact in
            SavedKnowledgeView(item: .memory(fact), titleOverride: "Edit memory")
        }
        .sheet(item: $skill) { item in SavedKnowledgeView(item: .skill(item), readOnly: marbleId != nil && item.owner != marbleId) }
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
                        VStack(alignment: .leading, spacing: 4) {
                            Text(fact.text).foregroundStyle(MarbleTheme.ink)
                            Text(store.knowledgeSource(conversationId: fact.conversationId, threadId: fact.threadId, date: fact.createdAt))
                                .font(.caption).foregroundStyle(MarbleTheme.muted)
                        }
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

func knowledgeDate(_ milliseconds: UInt64) -> String {
    Date(timeIntervalSince1970: Double(milliseconds) / 1_000)
        .formatted(.dateTime.day().month(.abbreviated).year())
}

struct RecentlyDeletedView: View {
    @EnvironmentObject private var store: MarbleStore
    var owner: String?

    var body: some View {
        List {
            Section {
                let deleted = (store.memory.recentlyDeleted ?? []).filter { owner == nil || $0.saved.owner == owner }
                let chats = owner == nil ? store.state.deletedChats ?? [] : []
                if deleted.isEmpty && chats.isEmpty { Text("Nothing recently deleted.").foregroundStyle(MarbleTheme.muted) }
                ForEach(chats) { chat in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(chat.title)
                        Text("Deleted \(knowledgeDate(chat.deletedAt))").font(.caption).foregroundStyle(MarbleTheme.muted)
                        MarbleInlineButton(title: "Restore") { Task { await store.restoreDeletedChat(chat.id) } }
                            .accessibilityLabel("Restore \(chat.title)")
                    }
                }
                ForEach(deleted) { entry in
                    VStack(alignment: .leading, spacing: 4) {
                        Text(entry.saved.title)
                        Text("Deleted \(knowledgeDate(entry.deletedAt))").font(.caption).foregroundStyle(MarbleTheme.muted)
                        MarbleInlineButton(title: "Restore") { Task { await store.updateKnowledge(id: entry.id, .restore) } }
                    }
                }
            } footer: {
                Text("Items can be restored for seven days.")
            }
            .listRowBackground(MarbleTheme.card)
        }
        .scrollContentBackground(.hidden)
        .background(MarbleTheme.paper)
        .navigationTitle("Recently Deleted")
    }
}

struct SavedKnowledgeView: View {
    @EnvironmentObject private var store: MarbleStore
    @Environment(\.dismiss) private var dismiss
    var item: MarbleSavedItem
    var readOnly = false
    var titleOverride: String? = nil
    @State private var text = ""
    @State private var name = ""
    @State private var use = ""
    @State private var steps = ""
    @State private var shared = false
    @State private var recipients: Set<String> = []
    @State private var showsSharing = false

    init(item: MarbleSavedItem, readOnly: Bool = false, titleOverride: String? = nil) {
        self.item = item
        self.readOnly = readOnly
        self.titleOverride = titleOverride
        switch item {
        case let .memory(fact):
            _text = State(initialValue: fact.text)
            _shared = State(initialValue: fact.shared)
        case let .skill(skill):
            _name = State(initialValue: skill.name)
            _use = State(initialValue: skill.whenToUse)
            _steps = State(initialValue: skill.steps.joined(separator: "\n"))
            _recipients = State(initialValue: Set(skill.sharedWith))
        }
    }

    var body: some View {
        NavigationStack {
            Form {
                switch item {
                case let .memory(fact):
                    Section("Memory") {
                        TextField("Memory", text: $text, axis: .vertical)
                            .accessibilityIdentifier("knowledge.memory.text")
                        Text(store.knowledgeSource(conversationId: fact.conversationId, threadId: fact.threadId, date: fact.createdAt))
                            .font(.caption).foregroundStyle(MarbleTheme.muted)
                    }
                    Section("Who can use this") {
                        if fact.owner.isEmpty {
                            Text("Shared with all agents")
                        } else {
                            Toggle("Share with all agents", isOn: $shared)
                            Text("Private memories still need permission before use in a group.")
                                .font(.caption).foregroundStyle(MarbleTheme.muted)
                        }
                    }
                case let .skill(skill):
                    Section("Skill") {
                        TextField("Name", text: $name)
                        TextField("When to use it", text: $use, axis: .vertical)
                        TextField("Steps, one per line", text: $steps, axis: .vertical)
                            .lineLimit(3...10)
                        if let teacher = skill.learnedFrom {
                            Text("Learned from \(store.knowledgeAgentName(teacher)), \(knowledgeDate(skill.createdAt))")
                                .font(.caption).foregroundStyle(MarbleTheme.muted)
                        }
                    }
                    if !readOnly {
                        Section("Sharing") {
                            Button(recipients.isEmpty ? "Share with agents" : "Share with \(recipients.count) \(recipients.count == 1 ? "agent" : "agents")") {
                                showsSharing = true
                            }
                            .frame(minHeight: 44)
                        }
                    }
                }
                if !readOnly {
                    Section {
                        Button("Move to Recently Deleted", role: .destructive) {
                            Task {
                                await store.updateKnowledge(id: item.id, .delete)
                                if store.errorMessage == nil { dismiss() }
                            }
                        }
                        .frame(minHeight: 44)
                    }
                }
                if let error = store.errorMessage { Text(error).foregroundStyle(MarbleTheme.problem) }
            }
            .disabled(readOnly)
            .scrollContentBackground(.hidden)
            .background(MarbleTheme.paper)
            .navigationTitle(titleOverride ?? title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button(readOnly ? "Done" : "Cancel") { dismiss() } }
                if !readOnly {
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Save") {
                            Task {
                                await save()
                                if store.errorMessage == nil { dismiss() }
                            }
                        }
                        .disabled(!canSave)
                    }
                }
            }
            .navigationDestination(isPresented: $showsSharing) {
                List(store.prism.marbles.filter { $0.id != item.owner }) { agent in
                    Toggle(agent.name, isOn: Binding(
                        get: { recipients.contains(agent.id) },
                        set: { selected in
                            if selected { recipients.insert(agent.id) } else { recipients.remove(agent.id) }
                        }
                    ))
                    .frame(minHeight: 44)
                    .listRowBackground(MarbleTheme.card)
                }
                .scrollContentBackground(.hidden)
                .background(MarbleTheme.paper)
                .navigationTitle("Share skill")
            }
        }
    }

    private var title: String {
        switch item { case .memory: "Remembered"; case .skill: "Skill saved" }
    }

    private var canSave: Bool {
        if case .memory = item { return !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
        return true
    }

    private func save() async {
        switch item {
        case let .memory(fact):
            if text != fact.text { await store.update(fact, text: text) }
            if store.errorMessage == nil && shared != fact.shared {
                await store.updateKnowledge(id: fact.id, .shareMemory, .init(shared: shared))
            }
        case let .skill(skill):
            let lines = steps.split(whereSeparator: \.isNewline).map(String.init)
            if name != skill.name || use != skill.whenToUse || lines != skill.steps {
                await store.updateKnowledge(id: skill.id, .editSkill, .init(name: name, whenToUse: use, steps: lines))
            }
            if store.errorMessage == nil && recipients != Set(skill.sharedWith) {
                await store.updateKnowledge(id: skill.id, .shareSkill, .init(agents: recipients.sorted()))
            }
        }
    }
}

struct MarbleKnowledgeCards: View {
    @EnvironmentObject private var store: MarbleStore
    var entry: MarbleEntry
    var chooseMemory: ((MarbleMemoryConsent) -> Void)?
    @State private var item: MarbleSavedItem?

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            cards
        }
        .sheet(item: $item) { saved in SavedKnowledgeView(item: saved) }
    }

    @ViewBuilder private var cards: some View {
        ForEach(store.savedKnowledge(for: entry)) { saved in
            Button { item = saved } label: {
                Label(chipTitle(saved), systemImage: "bookmark")
                    .font(.caption.weight(.semibold))
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .background(MarbleTheme.wash, in: Capsule())
                    .padding(.vertical, 6)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .frame(minHeight: 44, alignment: .leading)
        }
        ForEach((store.memory.pruningProposals ?? []).filter { $0.taskId == entry.taskId }) { proposal in
            MarblePruningCard(proposal: proposal)
        }
        if let chooseMemory {
            consentCards(chooseMemory: chooseMemory)
        }
    }

    private func consentCards(chooseMemory: @escaping (MarbleMemoryConsent) -> Void) -> some View {
        ForEach((store.memory.consentRequests ?? []).filter { $0.taskId == entry.taskId && $0.threadId == entry.threadId }) { request in
            let hasMemory = store.memory.facts.contains { $0.owner == request.owner }
            VStack(alignment: .leading, spacing: 8) {
                Text("Use a private memory here?").font(.subheadline.weight(.semibold))
                Text(hasMemory ? "Only the memory you choose can be used in this chat." : "There are no saved memories to choose from.")
                    .font(.footnote).foregroundStyle(MarbleTheme.muted)
                if hasMemory {
                    MarbleInlineButton(title: "Choose memory", prominent: true) { chooseMemory(request) }
                }
                MarbleInlineButton(title: "Not now") { Task { await store.updateKnowledge(id: request.id, .resolveConsent) } }
            }
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(MarbleTheme.wash, in: MarbleTheme.panel)
        }
    }

    private func chipTitle(_ item: MarbleSavedItem) -> String {
        switch item { case .memory: "Remembered"; case .skill: "Skill saved" }
    }
}

struct MarbleMemoryConsentPicker: View {
    @EnvironmentObject private var store: MarbleStore
    @Environment(\.dismiss) private var dismiss
    let request: MarbleMemoryConsent
    let onSelect: (String) -> Void

    var body: some View {
        NavigationStack {
            List(store.memory.facts.filter { $0.owner == request.owner }) { fact in
                Button {
                    onSelect(fact.id)
                } label: {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(fact.text)
                        Text(store.knowledgeSource(conversationId: fact.conversationId, threadId: fact.threadId, date: fact.createdAt))
                            .font(.caption).foregroundStyle(MarbleTheme.muted)
                    }
                    .frame(minHeight: 44)
                }
                .listRowBackground(MarbleTheme.card)
            }
            .scrollContentBackground(.hidden)
            .background(MarbleTheme.paper)
            .navigationTitle("Allow in this chat")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
        }
    }
}

private struct MarblePruningCard: View {
    @EnvironmentObject private var store: MarbleStore
    var proposal: MarblePruningProposal
    @State private var unchecked: Set<String> = []
    private var items: [MarbleSavedItem] {
        store.memory.savedItems.filter { proposal.itemIds.contains($0.id) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Suggested cleanup").font(.subheadline.weight(.semibold))
            ForEach(items) { item in
                let selected = !unchecked.contains(item.id)
                Button {
                    if selected { unchecked.insert(item.id) } else { unchecked.remove(item.id) }
                } label: {
                    HStack {
                        Text(item.summary)
                        Spacer()
                        if selected {
                            Image(systemName: "checkmark").font(.body.weight(.semibold))
                                .foregroundStyle(MarbleTheme.tint)
                                .accessibilityHidden(true)
                        }
                    }
                        .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(selected ? .isSelected : [])
            }
            ViewThatFits {
                HStack { actions }
                VStack(alignment: .leading) { actions }
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(MarbleTheme.wash, in: MarbleTheme.panel)
    }

    @ViewBuilder private var actions: some View {
        let selected = items.map(\.id).filter { !unchecked.contains($0) }
        MarbleInlineButton(title: "Delete selected", role: .destructive, prominent: true) { resolve(selected) }
            .tint(MarbleTheme.problem)
            .disabled(selected.isEmpty)
        MarbleInlineButton(title: "Keep all") { resolve([]) }
    }

    private func resolve(_ selected: [String]) {
        Task { await store.updateKnowledge(id: proposal.id, .confirmPruning, .init(selected: selected)) }
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
        .background(MarbleTheme.card, in: MarbleTheme.panel)
        .sheet(isPresented: $review) {
            MemoryDecisionView(title: "Review suggestion", scope: store.memoryScope(store.owner(of: suggestion)),
                               text: suggestion.text) { _ in await store.accept(suggestion) }
        }
    }

    @ViewBuilder private var actions: some View {
        MarbleInlineButton(title: "Not now") { Task { await store.dismiss(suggestion) } }
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
