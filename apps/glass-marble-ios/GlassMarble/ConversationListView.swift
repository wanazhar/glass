import SwiftUI

struct ConversationListView: View {
    @EnvironmentObject private var store: MarbleStore
    @EnvironmentObject private var navigation: MarbleNavigationModel
    @State private var searchText = ""
    @State private var renaming: MarbleConversation?
    @State private var renameDraft = ""
    @State private var deleting: MarbleConversation?

    var body: some View {
        List {
            searchSection
            conversationSection
        }
        .navigationTitle("Glass Marble")
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
            Text("Its transcript is removed from the host. A conversation with a running turn cannot be deleted.")
        }
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) { MarbleNewChatButton() }
        }
        // The navigation title already says which app this is; the strip below
        // only has to say which host is answering and what it is doing.
        .safeAreaInset(edge: .top) {
            HStack(spacing: 8) {
                Text(store.endpoint?.displayHost ?? "not connected")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                Spacer(minLength: 8)
                MarbleStatusText(
                    label: store.statusLabel,
                    tone: store.connection == .live ? .green : .secondary
                )
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 6)
        }
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
            if filteredConversations.isEmpty {
                VStack(spacing: 8) {
                    Text("No chats yet")
                        .font(.callout)
                    Text("Start one with the compose button.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, 24)
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
        }
    }

    private var sortedConversations: [MarbleConversation] {
        // Everyone and each marble's own chat have their own tabs; this list
        // is the default assistant's conversations.
        store.state.conversations
            .filter { conversation in
                conversation.id != everyoneConversationId
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

struct MarbleNewChatButton: View {
    @EnvironmentObject private var store: MarbleStore
    @EnvironmentObject private var navigation: MarbleNavigationModel
    @State private var creating = false
    @State private var failure: String?

    var body: some View {
        Button(action: create) {
            Label("New Chat", systemImage: "square.and.pencil")
                .labelStyle(.iconOnly)
        }
        .accessibilityIdentifier("chat.compose")
        .disabled(creating)
        .alert("Couldn’t create chat", isPresented: Binding(
            get: { failure != nil },
            set: { if !$0 { failure = nil } }
        )) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(failure ?? "")
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

private struct ConversationRow: View {
    var conversation: MarbleConversation
    var line: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(conversation.displayTitle)
                .font(.callout)
                .lineLimit(1)
            if let line {
                Text(line)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
        .padding(.vertical, 2)
    }
}
