import SwiftUI

struct ConversationListView: View {
    @EnvironmentObject private var store: MarbleStore

    var body: some View {
        List(selection: $store.selectedConversationId) {
            Section("Conversations") {
                if store.state.conversations.isEmpty {
                    Text("No conversations yet")
                        .font(.footnote)
                        .foregroundStyle(MarblePalette.muted)
                }
                ForEach(sortedConversations) { conversation in
                    ConversationRow(conversation: conversation)
                        .tag(Optional(conversation.id))
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
        .navigationTitle("Marble")
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
        .safeAreaInset(edge: .top) {
            HStack(spacing: 10) {
                MarbleMarbleMark()
                VStack(alignment: .leading, spacing: 0) {
                    Text("Glass Marble")
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(MarblePalette.ink)
                    Text(store.endpoint.displayHost)
                        .font(.caption2)
                        .foregroundStyle(MarblePalette.muted)
                        .lineLimit(1)
                }
                Spacer()
                MarbleStatusPill(
                    label: store.statusLabel,
                    tone: store.connection == .live ? MarblePalette.success : MarblePalette.muted
                )
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
        }
    }

    private var sortedConversations: [MarbleConversation] {
        store.state.conversations.sorted { $0.updatedAt > $1.updatedAt }
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
