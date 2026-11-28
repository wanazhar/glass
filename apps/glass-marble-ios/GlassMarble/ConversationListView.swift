import SwiftUI

struct ConversationListView: View {
    @EnvironmentObject private var store: MarbleStore
    @EnvironmentObject private var navigation: MarbleNavigationModel
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var searchText = ""
    @State private var renaming: MarbleConversation?
    @State private var renameDraft = ""
    @State private var deleting: MarbleConversation?
    @State private var groupChats: [MarbleThread] = []

    var body: some View {
        List {
            searchSection
            if !visibleGroups.isEmpty {
                Section("Group chats") {
                    ForEach(visibleGroups) { thread in
                        NavigationLink(value: ChatRoute.thread(thread.id)) {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(thread.title)
                                Text(thread.group?.closed == true ? "Completed" : "Group chat")
                                    .font(.footnote)
                                    .foregroundStyle(.secondary)
                            }
                            .fixedSize(horizontal: false, vertical: true)
                            .frame(minHeight: 44)
                        }
                    }
                }
            }
            conversationSection
            Section {
                NavigationLink(value: ChatRoute.archivedChats) {
                    Label("Archived", systemImage: "archivebox").frame(minHeight: 44)
                }
                .listRowBackground(MarbleTheme.card)
                NavigationLink {
                    RecentlyDeletedView(owner: nil)
                } label: {
                    Label("Recently Deleted", systemImage: "trash").frame(minHeight: 44)
                }
                .listRowBackground(MarbleTheme.card)
            }
        }
        .navigationTitle("Conversations")
        .navigationBarTitleDisplayMode(.inline)
        .searchable(text: $searchText, prompt: "Search conversations")
        .alert("Rename conversation", isPresented: renameBinding) {
            TextField("Title", text: $renameDraft)
            Button("Cancel", role: .cancel) { renaming = nil }
            Button("Save") {
                guard let conversation = renaming else { return }
                let title = renameDraft
                renaming = nil
                Task { await store.rename(conversation, title: title) }
            }
        }
        .confirmationDialog(
            "Delete this conversation?",
            isPresented: deleteBinding,
            titleVisibility: .visible
        ) {
            Button("Delete", role: .destructive) {
                guard let conversation = deleting else { return }
                deleting = nil
                Task {
                    if await store.delete(conversationId: conversation.id) {
                        navigation.deleted(conversation.id)
                    }
                }
            }
            Button("Cancel", role: .cancel) { deleting = nil }
        } message: {
            Text("You can restore it from Recently Deleted for seven days. A chat with a running turn can’t be deleted.")
        }
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) { MarbleNavConnection() }
            ToolbarItem(placement: .topBarTrailing) { MarbleNewChatButton() }
        }
        .animation(reduceMotion ? nil : .default, value: filteredConversations)
        .task(id: store.threadHostRevision) {
            groupChats = (try? await store.threads(in: marbleGroupConversationId)) ?? []
        }
        .onAppear {
            Task { if let chats = try? await store.threads(in: marbleGroupConversationId) { groupChats = chats } }
        }
    }

    private var visibleGroups: [MarbleThread] {
        let needle = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        return groupChats.filter { !$0.archived && (needle.isEmpty || $0.title.localizedCaseInsensitiveContains(needle)) }
    }

    private var renameBinding: Binding<Bool> {
        Binding(
            get: { renaming != nil },
            set: { presented in if !presented { renaming = nil } }
        )
    }

    private var deleteBinding: Binding<Bool> {
        Binding(
            get: { deleting != nil },
            set: { presented in if !presented { deleting = nil } }
        )
    }

    @ViewBuilder
    private var searchSection: some View {
        if !searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            MarbleSearchSection(query: searchText) { hit in
                NavigationLink(value: ChatRoute.searchHit(
                    conversationId: hit.conversationId, threadId: hit.threadId, entryId: hit.entryId
                )) { MarbleSearchRow(hit: hit) }
            }
        }
    }

    private var conversationSection: some View {
        Section {
            ForEach(filteredConversations) { conversation in
                conversationRow(conversation)
            }
            if !store.loaded && store.state.conversations.isEmpty {
                MarbleListSkeleton()
            } else if filteredConversations.isEmpty && visibleGroups.isEmpty {
                MarbleEmptyState("No chats yet", description: Text("Start one with the compose button."))
            }
        }
    }

    private func conversationRow(_ conversation: MarbleConversation) -> some View {
        NavigationLink(value: ChatRoute.conversation(conversation.id)) {
            ConversationRow(
                conversation: conversation,
                line: store.stateLine(for: conversation)
            )
        }
        .contextMenu {
            Button("Rename") {
                renaming = conversation
                renameDraft = conversation.title
            }
            Button("Delete", role: .destructive) {
                deleting = conversation
            }
            Button("Archive", systemImage: "archivebox") {
                Task { await store.archiveConversation(conversation.id, archived: true) }
            }
        }
        .swipeActions(edge: .leading, allowsFullSwipe: false) {
            Button("Archive", systemImage: "archivebox") {
                Task { await store.archiveConversation(conversation.id, archived: true) }
            }
            .tint(MarbleTheme.tint)
        }
    }

    private var sortedConversations: [MarbleConversation] {
        // Everyone and each marble's own chat have their own tabs; this list
        // is the default assistant's conversations.
        store.state.conversations
            .filter { conversation in
                !conversation.isEveryoneSession && conversation.archived != true && conversation.marbleId == nil
                    && !store.prism.marbles.contains {
                        !$0.isDefault && $0.conversationId == conversation.id
                    }
            }
            .sorted { $0.updatedAt > $1.updatedAt }
    }

    private var filteredConversations: [MarbleConversation] {
        let needle = searchText.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !needle.isEmpty else { return sortedConversations }
        return sortedConversations.filter { conversation in
            if conversation.displayTitle.lowercased().contains(needle) {
                return true
            }
            return store.state.tasks.contains { task in
                task.conversationId == conversation.id && task.prompt.lowercased().contains(needle)
            }
        }
    }

}

struct ArchivedChatsView: View {
    @EnvironmentObject private var store: MarbleStore

    var body: some View {
        List {
            let archived = store.state.conversations.filter { $0.archived == true }
                .sorted { $0.updatedAt > $1.updatedAt }
            if archived.isEmpty { MarbleEmptyState("No archived chats", systemImage: "archivebox") }
            ForEach(archived) { conversation in
                MarbleAdaptiveRow {
                    NavigationLink(value: ChatRoute.conversation(conversation.id)) {
                        ConversationRow(conversation: conversation, line: store.stateLine(for: conversation))
                    }
                    .buttonStyle(.plain)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .navigationLinkIndicatorVisibility(.hidden)
                    MarbleInlineButton(title: "Restore") {
                        Task { await store.archiveConversation(conversation.id, archived: false) }
                    }
                    .accessibilityLabel("Restore \(conversation.displayTitle)")
                }
                .listRowBackground(MarbleTheme.card)
            }
        }
        .scrollContentBackground(.hidden)
        .background(MarbleTheme.paper)
        .navigationTitle("Archived")
        .navigationBarTitleDisplayMode(.inline)
    }
}

struct MarbleNewChatButton: View {
    private enum ChatKind { case direct, group }

    @EnvironmentObject private var store: MarbleStore
    @EnvironmentObject private var navigation: MarbleNavigationModel
    @State private var creating = false
    @State private var failure: String?
    @State private var makingGroup = false
    @State private var choosingChat = false
    @State private var choice: ChatKind?

    var body: some View {
        Button { choosingChat = true } label: {
            Label("New chat", systemImage: "square.and.pencil")
                .labelStyle(.iconOnly)
                .frame(minWidth: 44, minHeight: 44)
        }
        .accessibilityIdentifier("chat.compose")
        .disabled(creating)
        .sheet(isPresented: $choosingChat, onDismiss: openChoice) {
            NavigationStack {
                ScrollView {
                    VStack(spacing: 12) {
                        MarbleInlineButton(title: "Private chat", prominent: true, expands: true) { choose(.direct) }
                        MarbleInlineButton(title: "Group chat", expands: true) { choose(.group) }
                    }
                    .padding(16)
                    .frame(maxWidth: 420)
                    .frame(maxWidth: .infinity)
                }
                .marbleNewChatBackground()
                .navigationTitle("New chat")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Cancel") { choosingChat = false }
                    }
                }
            }
            .presentationDetents([.medium, .large])
        }
        .sheet(isPresented: $makingGroup) {
            MarbleGroupComposer { thread in
                makingGroup = false
                navigation.openThread(thread.id)
            }
        }
        .alert("Couldn’t create chat", isPresented: Binding(
            get: { failure != nil },
            set: { if !$0 { failure = nil } }
        )) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(failure ?? "")
        }
    }

    private func choose(_ kind: ChatKind) {
        choice = kind
        choosingChat = false
    }

    private func openChoice() {
        defer { choice = nil }
        switch choice {
        case .direct: create()
        case .group: makingGroup = true
        case nil: break
        }
    }

    private func create() {
        let origin = navigation.state
        creating = true
        Task {
            defer { creating = false }
            do {
                let conversation = try await store.createConversation()
                open(conversation.id, from: origin)
            } catch MarbleClientError.notFound {
                // Older hosts create a chat when its first message is sent.
                open(nil, from: origin)
            } catch is CancellationError {
            } catch {
                failure = error.localizedDescription
            }
        }
    }

    private func open(_ id: String?, from origin: MarbleNavigation) {
        let current = navigation.state
        guard current.tab == origin.tab,
              current.everyonePath == origin.everyonePath,
              current.chatPath == origin.chatPath else { return }
        navigation.openNewChat(id)
    }
}

private struct MarbleGroupComposer: View {
    @EnvironmentObject private var store: MarbleStore
    @Environment(\.dismiss) private var dismiss
    let onCreate: (MarbleThread) -> Void
    @State private var title = ""
    @State private var goal = ""
    @State private var doneWhen = ""
    @State private var members: Set<String> = ["lead"]
    @State private var saving = false
    @State private var failed = false

    var body: some View {
        NavigationStack {
            Form {
                Section("Chat") { TextField("Title", text: $title) }
                    .listRowBackground(MarbleTheme.card)
                Section("Crew presets") {
                    preset("Refactor", ["engineer", "reviewer"])
                    preset("Research", ["researcher", "reviewer"])
                    preset("Launch", ["designer", "engineer", "reviewer"])
                }
                .listRowBackground(MarbleTheme.card)
                Section("Members") { MarbleGroupMemberPicker(members: $members) }
                    .listRowBackground(MarbleTheme.card)
                Section("Task (optional)") {
                    TextField("Goal", text: $goal, axis: .vertical)
                        .accessibilityIdentifier("group.goal")
                    TextField("Done when", text: $doneWhen, axis: .vertical)
                        .accessibilityIdentifier("group.doneWhen")
                }
                .listRowBackground(MarbleTheme.card)
            }
            .scrollContentBackground(.hidden)
            .marbleNewChatBackground()
            .navigationTitle("New group chat")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Create", action: create)
                        .disabled(saving || title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
            .alert("Couldn’t create group chat", isPresented: $failed) { Button("OK", role: .cancel) {} }
        }
    }

    private func preset(_ label: String, _ agents: Set<String>) -> some View {
        Button {
            members = agents.intersection(store.prism.marbles.map(\.id)).union(["lead"])
        } label: {
            HStack {
                Text(label)
                Spacer()
                if members == agents.union(["lead"]) { Image(systemName: "checkmark") }
            }
            .frame(minHeight: 44)
            .contentShape(Rectangle())
        }
    }

    private func create() {
        saving = true
        Task {
            defer { saving = false }
            do {
                let ordered = store.prism.marbles.map(\.id).filter(members.contains)
                let group = MarbleGroupThread(members: ordered, goal: goal, doneWhen: doneWhen)
                onCreate(try await store.startGroupThread(title: title, group: group))
            } catch { failed = true }
        }
    }
}

struct MarbleGroupMemberPicker: View {
    @EnvironmentObject private var store: MarbleStore
    @Binding var members: Set<String>

    var body: some View {
        Label("Lead · Required", systemImage: "lock")
            .frame(minHeight: 44)
            .listRowBackground(MarbleTheme.card)
        ForEach(store.prism.marbles.filter { $0.id != "lead" }) { agent in
            Toggle(agent.name, isOn: Binding(
                get: { members.contains(agent.id) },
                set: { selected in
                    if selected { members.insert(agent.id) } else { members.remove(agent.id) }
                }
            ))
            .frame(minHeight: 44)
            .listRowBackground(MarbleTheme.card)
        }
    }
}

private struct ConversationRow: View {
    var conversation: MarbleConversation
    var line: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(conversation.displayTitle)
                .font(.subheadline)
            if let line {
                Text(line)
                    .font(.footnote)
                    .foregroundStyle(MarbleTheme.muted)
            }
        }
        .padding(.vertical, 2)
        .fixedSize(horizontal: false, vertical: true)
    }
}
