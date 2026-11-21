import SwiftUI

struct ConversationListView: View {
    @EnvironmentObject private var store: MarbleStore
    @State private var searchText = ""
    @State private var renaming: MarbleConversation?
    @State private var renameDraft = ""
    @State private var deleting: MarbleConversation?

    var body: some View {
        List(selection: $store.selectedConversationId) {
            Section("Conversations") {
                if store.state.conversations.isEmpty {
                    Text("No conversations yet")
                        .font(.footnote)
                        .foregroundStyle(MarblePalette.muted)
                }
                ForEach(filteredConversations) { conversation in
                    ConversationRow(conversation: conversation)
                        .tag(Optional(conversation.id))
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
            }

            Section("Durable tasks") {
                if store.state.tasks.isEmpty {
                    Text("No durable tasks yet")
                        .font(.footnote)
                        .foregroundStyle(MarblePalette.muted)
                }
                ForEach(recentTasks) { task in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(task.prompt.isEmpty ? task.id : task.prompt)
                            .font(.footnote)
                            .lineLimit(2)
                        Text("\(task.status) · \(task.owner)")
                            .font(.caption2)
                            .foregroundStyle(statusTone(task.status))
                    }
                    .padding(.vertical, 2)
                }
            }
        }
        .scrollContentBackground(.hidden)
        .background(MarbleBackdrop())
        .navigationTitle("Glass Marble")
        .navigationBarTitleDisplayMode(.inline)
        .searchable(text: $searchText, placement: .sidebar, prompt: "Search conversations")
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
                Task { await store.delete(conversation) }
            }
            Button("Cancel", role: .cancel) { deleting = nil }
        } message: {
            Text("Its transcript is removed from the host. A conversation with a running turn cannot be deleted.")
        }
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    store.selectedConversationId = nil
                } label: {
                    Image(systemName: "square.and.pencil")
                }
                .accessibilityLabel("New conversation")
            }
        }
        // The navigation title already says which app this is; the strip below
        // only has to say which host is answering and what it is doing.
        .safeAreaInset(edge: .top) {
            HStack(spacing: 8) {
                MarbleMarbleMark()
                Text(store.endpoint.displayHost)
                    .font(.caption)
                    .foregroundStyle(MarblePalette.muted)
                    .lineLimit(1)
                Spacer(minLength: 8)
                MarbleStatusPill(
                    label: store.statusLabel,
                    tone: store.connection == .live ? MarblePalette.success : MarblePalette.muted
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

    private var sortedConversations: [MarbleConversation] {
        store.state.conversations.sorted { $0.updatedAt > $1.updatedAt }
    }

    private var filteredConversations: [MarbleConversation] {
        let needle = searchText.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !needle.isEmpty else { return sortedConversations }
        return sortedConversations.filter { conversation in
            if conversation.title.lowercased().contains(needle) {
                return true
            }
            return store.state.tasks.contains { task in
                task.conversationId == conversation.id && task.prompt.lowercased().contains(needle)
            }
        }
    }

    private var recentTasks: [MarbleTask] {
        Array(store.state.tasks.sorted { $0.at > $1.at }.prefix(8))
    }

    private func statusTone(_ status: String) -> Color {
        switch status {
        case "completed": return MarblePalette.success
        case "failed": return MarblePalette.danger
        case "running": return MarblePalette.accent
        default: return MarblePalette.muted
        }
    }
}

private struct ConversationRow: View {
    var conversation: MarbleConversation

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(conversation.title.isEmpty ? conversation.id : conversation.title)
                .font(.callout)
                .lineLimit(1)
            Text(conversation.taskIds.count == 1 ? "1 turn" : "\(conversation.taskIds.count) turns")
                .font(.caption2)
                .foregroundStyle(MarblePalette.muted)
        }
        .padding(.vertical, 2)
    }
}
