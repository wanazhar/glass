import GlassModels
import SwiftUI

/// Conversation sidebar. `NavigationSplitView` collapses to a stack on iPhone and adopts Liquid Glass bars on iOS 26.
struct ThreadsView: View {
    @Environment(AppModel.self) private var app
    @State private var threads: ThreadsModel?

    var body: some View {
        Group {
            if let threads {
                ThreadSplitView(threads: threads)
            } else {
                ProgressView()
            }
        }
        .task { if threads == nil { threads = ThreadsModel(app: app) } }
    }
}

private struct ThreadSplitView: View {
    @Environment(AppModel.self) private var app
    @Bindable var threads: ThreadsModel
    @State private var compactColumn: NavigationSplitViewColumn = .sidebar

    /// The conversation on screen: open, and on the Conversations tab.
    private var onScreen: String? { app.selectedTab == .threads ? threads.selection : nil }

    var body: some View {
        NavigationSplitView(preferredCompactColumn: $compactColumn) {
            ThreadListView(threads: threads)
        } detail: {
            if let id = threads.selection, let thread = threads.thread(id) {
                ConversationView(conversation: threads.conversation(for: thread), threads: threads) {
                    threads.selection = nil
                    compactColumn = .sidebar
                }
                .id(id)
            } else {
                WelcomeView(newThread: { Task { await threads.newThread() } })
            }
        }
        .onChange(of: onScreen, initial: true) { _, id in Attention.visibleThread = id }
    }
}

struct ThreadListView: View {
    @Bindable var threads: ThreadsModel
    @State private var searching = false

    var body: some View {
        List(selection: $threads.selection) {
            if let error = threads.error {
                Label(error, systemImage: "exclamationmark.triangle.fill")
                    .foregroundStyle(.red)
                    .font(.footnote)
                    .accessibilityIdentifier("threads.error")
            }
            Section {
                ForEach(threads.threads) { thread in
                    ThreadRow(thread: thread)
                        .tag(thread.id)
                        .swipeActions {
                            Button("Close", systemImage: "xmark.circle") { Task { await threads.close(thread.id) } }
                                .tint(.orange)
                        }
                        .contextMenu {
                            Button("Close Conversation", systemImage: "xmark.circle") { Task { await threads.close(thread.id) } }
                        }
                        .accessibilityIdentifier("thread.\(thread.id)")
                }
                if threads.nextOffset != nil {
                    Button {
                        Task { await threads.loadMore() }
                    } label: {
                        Label("More conversations", systemImage: "ellipsis.circle")
                    }
                    .accessibilityIdentifier("threads.more")
                }
            } header: {
                if threads.loaded, !threads.threads.isEmpty {
                    Text(threads.countLabel).accessibilityIdentifier("threads.count")
                }
            }
        }
        .listStyle(.sidebar)
        .overlay {
            if threads.loaded, threads.threads.isEmpty {
                ContentUnavailableView {
                    Label(threads.query.isEmpty ? "No conversations yet" : "No matches", systemImage: "bubble.left.and.text.bubble.right")
                } description: {
                    Text(threads.query.isEmpty ? "Start one and it will appear here and in the web UI." : "Try another search or project.")
                }
                .accessibilityIdentifier("threads.empty")
            } else if !threads.loaded {
                ProgressView()
            }
        }
        .navigationTitle("Conversations")
        .searchable(text: $threads.query, isPresented: $searching, prompt: "Search conversations")
        .background {
            Button("Search Conversations") { searching = true }
                .keyCommand(.searchConversations)
                .hidden()
        }
        .task(id: "\(threads.query)\u{1}\(threads.project)") {
            if !threads.query.isEmpty { try? await Task.sleep(for: .milliseconds(250)) }
            guard !Task.isCancelled else { return }
            await threads.reload()
        }
        .refreshable { await threads.reload() }
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                Menu {
                    Picker("Project", selection: $threads.project) {
                        Text("All projects").tag("")
                        ForEach(threads.projects) { project in
                            Text("\(project.name) (\(project.count))").tag(project.workspace)
                        }
                    }
                } label: {
                    Label("Project", systemImage: threads.project.isEmpty ? "folder" : "folder.fill")
                }
                .accessibilityIdentifier("threads.project")
            }
            ToolbarItem(placement: .primaryAction) {
                Button("New conversation", systemImage: "square.and.pencil") { Task { await threads.newThread() } }
                    .keyCommand(.newConversation)
                    .accessibilityIdentifier("threads.new")
            }
        }
    }
}

private struct ThreadRow: View {
    var thread: ThreadSummary

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                if thread.writerActive == true {
                    Circle().fill(.green).frame(width: 7, height: 7).accessibilityLabel("Active")
                }
                Text(thread.displayTitle).font(.body.weight(.medium)).lineLimit(2)
            }
            metadata
                .font(.caption)
                .foregroundStyle(.secondary)
                .lineLimit(2)
        }
        .accessibilityElement(children: .combine)
        .padding(.vertical, 2)
    }

    /// Folder, age and origin in one line, so it wraps as a sentence instead of truncating each piece.
    private var metadata: Text {
        var parts: [Text] = []
        if let workspace = thread.workspace, !workspace.isEmpty {
            parts.append(Text((workspace as NSString).lastPathComponent))
        }
        if let date = thread.modifiedDate { parts.append(Text(date, format: .relative(presentation: .named))) }
        if let origin = thread.originNote { parts.append(Text(origin)) }
        return parts.dropFirst().reduce(parts.first ?? Text("")) { $0 + Text(" · ") + $1 }
    }
}

struct WelcomeView: View {
    var newThread: () -> Void

    var body: some View {
        ContentUnavailableView {
            Text("No conversation selected").accessibilityIdentifier("welcome.title")
        } description: {
            Text("Choose a conversation, or start one in this workspace. The web UI sees the same conversations.")
        } actions: {
            Button("New Conversation", action: newThread)
                .glassButtonStyle(prominent: true)
        }
    }
}
