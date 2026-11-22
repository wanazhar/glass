import SwiftUI

struct MarbleThreadInboxLink: View {
    let conversationId: String

    var body: some View {
        NavigationLink(value: ChatRoute.threadInbox(conversationId)) {
            Label("Threads", systemImage: "bubble.left.and.text.bubble.right")
        }
        .accessibilityLabel("Threads")
    }
}

struct MarbleThreadDestination: View {
    let route: ChatRoute

    var body: some View {
        switch route {
        case .threadInbox(let conversationId):
            MarbleThreadInbox(conversationId: conversationId)
        case .thread(let id):
            MarbleThreadView(id: id)
        case .searchHit(_, let threadId?, let entryId):
            MarbleThreadView(id: threadId, initialEntryId: entryId)
        case .searchHit(let conversationId, nil, let entryId?):
            MarbleHistoryView(conversationId: conversationId, entryId: entryId)
        case .searchHit(let conversationId, nil, nil):
            MarbleLatestChat(conversationId: conversationId)
        default:
            EmptyView()
        }
    }
}

private struct MarbleThreadInbox: View {
    @EnvironmentObject private var store: MarbleStore
    let conversationId: String
    @State private var threads: [MarbleThread]?
    @State private var failed = false

    var body: some View {
        List {
            if let threads {
                ForEach(threads) { thread in
                    NavigationLink(value: ChatRoute.thread(thread.id)) {
                        VStack(alignment: .leading, spacing: 5) {
                            Text(thread.parentPreview.isEmpty ? "Thread" : thread.parentPreview)
                                .lineLimit(2)
                            HStack {
                                if !thread.parentSender.isEmpty { Text(thread.parentSender) }
                                Text(thread.summary.replyLabel)
                                Spacer()
                                Text(marbleMessageDate(thread.summary.lastActivityAt), style: .relative)
                            }
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        }
                        .padding(.vertical, 2)
                    }
                }
                if threads.isEmpty && !failed {
                    ContentUnavailableView("No threads yet", systemImage: "text.bubble",
                                           description: Text("Touch and hold a message to reply in a thread."))
                }
            } else if !failed {
                ProgressView()
            }
            if failed {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Couldn’t load threads.")
                    Button("Retry") { Task { await load() } }
                }
            }
        }
        .navigationTitle("Threads")
        .toolbarVisibility(.hidden, for: .tabBar)
        .task(id: store.threadUpdates.filter { $0.value.conversationId == conversationId }) {
            await load()
        }
        .refreshable { await load() }
    }

    private func load() async {
        do {
            threads = try await store.threads(in: conversationId)
                .sorted { $0.summary.lastActivityAt > $1.summary.lastActivityAt }
            failed = false
        } catch is CancellationError {
        } catch {
            failed = true
        }
    }
}

private struct MarbleThreadView: View {
    @EnvironmentObject private var store: MarbleStore
    let id: String
    var initialEntryId: String?
    @State private var page: MarbleThreadPage?
    @State private var query = ""
    @State private var searching = false
    @State private var draft = ""
    @State private var jump: MarbleMessageJump?
    @State private var sending = false
    @State private var loadFailed = false
    @State private var sendFailed = false
    @State private var loadGeneration = UUID()

    var body: some View {
        Group {
            if searching && !query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                List {
                    MarbleSearchSection(query: query, threadId: id) { hit in
                        Button {
                            if let entryId = hit.entryId { jump = MarbleMessageJump(entryId: entryId) }
                            searching = false
                            query = ""
                            if page?.entries.contains(where: { $0.id == hit.entryId }) != true {
                                Task { await load() }
                            }
                        } label: {
                            MarbleSearchRow(hit: hit)
                        }
                    }
                }
            } else if let page {
                MarbleJumpTimeline(entries: page.entries, jump: jump, parentId: page.thread.parentEntryId)
            } else if loadFailed {
                ContentUnavailableView {
                    Label("Couldn’t load thread", systemImage: "text.bubble")
                } actions: {
                    Button("Retry") { Task { await load() } }
                }
            } else {
                ProgressView()
            }
        }
        .navigationTitle("Thread")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarVisibility(.hidden, for: .tabBar)
        .searchable(text: $query, isPresented: $searching, prompt: "Search this thread")
        .marbleComposerInset {
            if page != nil && !searching {
                VStack(spacing: 0) {
                    if sendFailed {
                        Text("Couldn’t send the reply. Try again.")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                            .padding(.top, 8)
                    }
                    ComposerBar(draft: $draft, isWorking: false, placeholder: "Reply in thread",
                                onSend: { Task { await send() } }, onStop: {})
                        .disabled(sending)
                }
                .marbleComposerBackground()
            }
        }
        .task(id: store.threadUpdates[id]) {
            if jump == nil, let initialEntryId { jump = MarbleMessageJump(entryId: initialEntryId) }
            await load()
        }
    }

    private func load() async {
        let generation = UUID()
        loadGeneration = generation
        do {
            let result = try await store.thread(id: id)
            guard generation == loadGeneration else { return }
            page = result
            loadFailed = false
        } catch is CancellationError {
        } catch {
            if generation == loadGeneration { loadFailed = true }
        }
    }

    private func send() async {
        guard let thread = page?.thread, !sending else { return }
        let text = draft
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        sending = true
        sendFailed = false
        defer { sending = false }
        do {
            try await store.sendInThread(trimmed, thread: thread)
            if text == draft { draft = "" }
            await load()
            if let entry = page?.entries.last { jump = MarbleMessageJump(entryId: entry.id) }
        } catch is CancellationError {
        } catch {
            sendFailed = true
        }
    }
}

struct MarbleSearchSection<Row: View>: View {
    @EnvironmentObject private var store: MarbleStore
    let query: String
    var threadId: String?
    @ViewBuilder var row: (MarbleSearchResult) -> Row
    @State private var hits: [MarbleSearchResult] = []
    @State private var loadedQuery: String?
    @State private var failed = false

    var body: some View {
        Section("Message matches") {
            if loadedQuery != query {
                ProgressView()
            } else if failed {
                Text("Couldn’t search messages.")
            } else if hits.isEmpty {
                Text("No messages found.")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(hits) { row($0) }
            }
        }
        .task(id: query) {
            do {
                try await Task.sleep(for: .milliseconds(300))
                hits = try await store.search(query: query.trimmingCharacters(in: .whitespacesAndNewlines), threadId: threadId)
                failed = false
                loadedQuery = query
            } catch is CancellationError {
            } catch {
                guard !Task.isCancelled else { return }
                failed = true
                loadedQuery = query
            }
        }
    }
}

struct MarbleSearchRow: View {
    let hit: MarbleSearchResult

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(hit.sender.isEmpty ? "Message" : hit.sender)
                    .fontWeight(.semibold)
                if hit.threadId != nil {
                    Text("Thread").foregroundStyle(.secondary)
                }
                Spacer()
                Text(marbleMessageDate(hit.createdAt), format: .dateTime.month().day().hour().minute())
                    .foregroundStyle(.secondary)
            }
            .font(.caption)
            Text(highlightedSnippet)
                .font(.footnote)
                .lineLimit(3)
        }
        .padding(.vertical, 2)
        .foregroundStyle(.primary)
    }

    private var highlightedSnippet: AttributedString {
        var result = AttributedString()
        var start = hit.snippet.startIndex
        for range in hit.highlights {
            result.append(AttributedString(String(hit.snippet[start..<range.lowerBound])))
            var match = AttributedString(String(hit.snippet[range]))
            match.font = .footnote.bold()
            match.backgroundColor = Color.accentColor.opacity(0.16)
            result.append(match)
            start = range.upperBound
        }
        result.append(AttributedString(String(hit.snippet[start...])))
        return result
    }
}

private struct MarbleMessageJump: Equatable {
    let entryId: String
    let requestId = UUID()
}

private struct MarbleJumpTimeline: View {
    @EnvironmentObject private var store: MarbleStore
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let entries: [MarbleEntry]
    let jump: MarbleMessageJump?
    var parentId: String?
    @State private var highlight: String?

    var body: some View {
        let transcript = MarbleReasoningAssociation(entries, keepVisible: jump?.entryId)
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 14) {
                    ForEach(transcript.standalone) { entry in
                        VStack(alignment: .leading, spacing: 10) {
                            MarbleEntryView(entry: entry, speaker: store.speakerStyle(for: entry),
                                            reasoning: transcript.attached[entry.id], allowsThreads: parentId == nil)
                            if entry.id == parentId && entries.count > 1 { Divider() }
                        }
                        .padding(.vertical, 3)
                        .background(highlight == entry.id ? Color.accentColor.opacity(0.15) : .clear,
                                    in: RoundedRectangle(cornerRadius: 8))
                        .id(entry.id)
                    }
                }
                .padding(16)
                .frame(maxWidth: 820)
                .frame(maxWidth: .infinity)
            }
            .scrollDismissesKeyboard(.interactively)
            .defaultScrollAnchor(.bottom, for: .sizeChanges)
            .task(id: jump.flatMap { target in entries.contains { $0.id == target.entryId } ? target : nil }) {
                guard let jump, entries.contains(where: { $0.id == jump.entryId }) else { return }
                highlight = jump.entryId
                await Task.yield()
                proxy.scrollTo(jump.entryId, anchor: .center)
                try? await Task.sleep(for: .seconds(1.6))
                guard !Task.isCancelled else { return }
                withAnimation(reduceMotion ? nil : .easeOut) { highlight = nil }
            }
        }
    }
}

private struct MarbleHistoryView: View {
    @EnvironmentObject private var store: MarbleStore
    let conversationId: String
    let entryId: String
    @State private var entries: [MarbleEntry]?
    @State private var failed = false
    @State private var jump: MarbleMessageJump?
    @State private var latest = false

    var body: some View {
        if latest {
            MarbleLatestChat(conversationId: conversationId)
        } else {
            Group {
                if let entries {
                    MarbleJumpTimeline(entries: entries, jump: jump)
                } else if failed {
                    ContentUnavailableView {
                        Label("Couldn’t find that message", systemImage: "magnifyingglass")
                    } actions: {
                        Button("Retry") { Task { await load() } }
                    }
                } else {
                    ProgressView()
                }
            }
            .navigationTitle(conversationId == everyoneConversationId ? "Everyone" : store.state.conversation(conversationId)?.title ?? "Chat")
            .navigationBarTitleDisplayMode(.inline)
            .toolbarVisibility(.hidden, for: .tabBar)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Latest") { latest = true }
                }
            }
            .task { await load() }
        }
    }

    private func load() async {
        do {
            let result = try await store.transcript(conversationId: conversationId, around: entryId)
            guard result.contains(where: { $0.id == entryId }) else { failed = true; return }
            entries = result
            jump = MarbleMessageJump(entryId: entryId)
        } catch is CancellationError {
        } catch {
            failed = true
        }
    }
}

private struct MarbleLatestChat: View {
    @EnvironmentObject private var store: MarbleStore
    let conversationId: String

    var body: some View {
        if conversationId == everyoneConversationId {
            EveryoneView()
        } else if let marble = store.prism.marbles.first(where: { !$0.isDefault && $0.conversationId == conversationId }) {
            MarbleChatScreen(marbleId: marble.id)
        } else {
            MarbleChatView(route: .conversation(conversationId))
        }
    }
}

private func marbleMessageDate(_ milliseconds: UInt64) -> Date {
    Date(timeIntervalSince1970: Double(milliseconds) / 1_000)
}
