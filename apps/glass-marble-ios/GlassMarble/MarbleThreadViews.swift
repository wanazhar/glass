import SwiftUI

struct EveryoneView: View {
    @EnvironmentObject private var store: MarbleStore
    @EnvironmentObject private var navigation: MarbleNavigationModel
    @State private var threads: [MarbleThread] = []
    @State private var naming = false
    @State private var title = ""
    @State private var deleting: MarbleThread?
    @State private var failed = false
    @State private var archived = false

    var body: some View {
        List {
            let active = threads.filter { !$0.archived }
            if active.isEmpty {
                ContentUnavailableView("No threads yet", systemImage: "bubble.left.and.bubble.right",
                                       description: Text("Start a thread to talk with Everyone."))
            }
            ForEach(active) { thread in
                NavigationLink(value: ChatRoute.thread(thread.id)) {
                    row(thread)
                }
                .swipeActions {
                    Button("Delete", systemImage: "trash", role: .destructive) { deleting = thread }
                    Button("Archive", systemImage: "archivebox") { update(thread, action: "archive") }
                        .tint(MarbleTheme.tint)
                }
                .listRowBackground(MarbleTheme.card)
            }
            Section {
                NavigationLink {
                    EveryoneHistoryView()
                } label: {
                    Label("Earlier messages", systemImage: "clock")
                        .frame(minHeight: 44)
                }
                Button {
                    archived = true
                } label: {
                    Label("Archived", systemImage: "archivebox")
                        .frame(minHeight: 44)
                }
                .buttonStyle(.plain)
            }
            .listRowBackground(MarbleTheme.card)
        }
        .scrollContentBackground(.hidden)
        .background(MarbleTheme.paper)
        .navigationTitle("Everyone")
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    title = ""
                    naming = true
                } label: {
                    Image(systemName: "bubble.left.and.bubble.right")
                        .frame(minWidth: 44, minHeight: 44)
                }
                .accessibilityLabel("New Everyone thread")
            }
        }
        .alert("New Everyone thread", isPresented: $naming) {
            TextField("Thread title", text: $title)
            Button("Cancel", role: .cancel) {}
            Button("Create") { create() }
                .disabled(title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
        .confirmationDialog("Delete this thread?", isPresented: Binding(
            get: { deleting != nil }, set: { if !$0 { deleting = nil } }
        ), titleVisibility: .visible, presenting: deleting) { thread in
            Button("Delete thread", role: .destructive) { update(thread, action: "delete") }
        }
        .alert("Couldn’t update threads", isPresented: $failed) {
            Button("OK", role: .cancel) {}
        }
        .navigationDestination(isPresented: $archived) {
            List(threads.filter(\.archived)) { thread in
                HStack {
                    row(thread)
                    Button("Restore") { update(thread, action: "restore") }
                        .buttonStyle(.borderless)
                        .frame(minHeight: 44)
                }
                .listRowBackground(MarbleTheme.card)
            }
            .scrollContentBackground(.hidden)
            .background(MarbleTheme.paper)
            .navigationTitle("Archived")
        }
        .task(id: store.threadUpdates) { await load() }
        .refreshable { await load() }
    }

    private func row(_ thread: MarbleThread) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(thread.displayTitle).font(.headline)
            Text(marbleMessageDate(thread.summary.lastActivityAt), style: .relative)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
    }

    private func load() async {
        do {
            threads = try await store.threads(in: everyoneConversationId)
                .sorted { $0.summary.lastActivityAt > $1.summary.lastActivityAt }
        } catch is CancellationError {
        } catch {
            failed = true
        }
    }

    private func create() {
        Task {
            do {
                let thread = try await store.startEveryoneThread(title: title)
                navigation.setEveryonePath([.thread(thread.id)])
            } catch {
                failed = true
            }
        }
    }

    private func update(_ thread: MarbleThread, action: String) {
        Task {
            do {
                try await store.updateEveryoneThread(thread.id, action: action)
                await load()
            } catch {
                failed = true
            }
        }
    }
}

private extension MarbleThread {
    var displayTitle: String {
        if !title.isEmpty { return title }
        return parentPreview.isEmpty ? "Thread" : parentPreview
    }
}

struct MarbleThreadInboxLink: View {
    let conversationId: String

    var body: some View {
        NavigationLink(value: ChatRoute.threadInbox(conversationId)) {
            Label("Threads", systemImage: "bubble.left.and.text.bubble.right")
        }
        .accessibilityLabel("Threads")
    }
}

struct MarbleChatTools: View {
    let conversationId: String

    var body: some View {
        Menu("Chat options", systemImage: "ellipsis") {
            MarbleThreadInboxLink(conversationId: conversationId)
            NavigationLink(value: ChatRoute.find(conversationId)) {
                Label("Find", systemImage: "magnifyingglass")
            }
        }
    }
}

struct MarbleThreadDestination: View {
    let route: ChatRoute

    var body: some View {
        switch route {
        case .find(let conversationId):
            MarbleFindView(conversationId: conversationId)
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

private struct MarbleFindView: View {
    let conversationId: String
    @State private var query = ""

    var body: some View {
        List {
            if query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                Text("Search messages and threads in this chat.")
                    .foregroundStyle(MarbleTheme.muted)
                    .listRowBackground(MarbleTheme.card)
            } else {
                MarbleSearchSection(query: query, conversationId: conversationId) { hit in
                    NavigationLink(value: ChatRoute.searchHit(
                        conversationId: hit.conversationId, threadId: hit.threadId, entryId: hit.entryId
                    )) { MarbleSearchRow(hit: hit) }
                    .listRowBackground(MarbleTheme.card)
                }
            }
        }
        .scrollContentBackground(.hidden)
        .background(MarbleTheme.paper)
        .navigationTitle("Find")
        .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Find in this chat")
        .toolbarVisibility(.hidden, for: .tabBar)
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
        .scrollContentBackground(.hidden)
        .background(MarbleTheme.paper)
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
    @Environment(\.dismiss) private var dismiss
    let id: String
    var initialEntryId: String?
    private var page: MarbleThreadPage? { store.threadPage(id) }
    @State private var query = ""
    @State private var searching = false
    @State private var draft = ""
    @State private var jump: MarbleMessageJump?
    @State private var appliedInitialJump = false
    @State private var loadFailed = false
    @State private var changeFailed = false
    @State private var confirmDelete = false
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
                timeline(page)
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
        .navigationTitle(page?.thread.displayTitle ?? "Thread")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarVisibility(.hidden, for: .tabBar)
        .searchable(text: $query, isPresented: $searching, prompt: "Search this thread")
        .marbleComposerInset {
            if page != nil && page?.thread.archived == false && !searching {
                ComposerBar(draft: $draft, isWorking: false, placeholder: "Reply in thread",
                            onSend: send, onStop: {})
            }
        }
        .toolbar {
            if page?.thread.conversationId == everyoneConversationId {
                ToolbarItem(placement: .topBarTrailing) {
                    Menu {
                        Button("Archive", systemImage: "archivebox") { update("archive") }
                        Button("Delete", systemImage: "trash", role: .destructive) { confirmDelete = true }
                    } label: {
                        Image(systemName: "ellipsis").frame(minWidth: 44, minHeight: 44)
                    }
                    .accessibilityLabel("Thread options")
                }
            }
        }
        .confirmationDialog("Delete this thread?", isPresented: $confirmDelete, titleVisibility: .visible) {
            Button("Delete thread", role: .destructive) { update("delete") }
        }
        .alert("Couldn’t update this thread", isPresented: $changeFailed) {
            Button("OK", role: .cancel) {}
        }
        .task(id: [store.threadEventSequences[id], store.threadHostRevision]) {
            if !appliedInitialJump, let initialEntryId { jump = MarbleMessageJump(entryId: initialEntryId) }
            appliedInitialJump = true
            await load()
        }
    }

    private func update(_ action: String) {
        Task {
            do {
                try await store.updateEveryoneThread(id, action: action)
                dismiss()
            } catch {
                changeFailed = true
            }
        }
    }

    private func load() async {
        let generation = UUID()
        loadGeneration = generation
        do {
            try await store.loadThreadHistory(id)
            guard generation == loadGeneration else { return }
            if let jump {
                while let page, let cursor = page.nextBefore,
                      !page.entries.contains(where: { $0.id == jump.entryId }) {
                    await store.loadOlderThread(id)
                    guard generation == loadGeneration, !Task.isCancelled,
                          self.page?.nextBefore != cursor else { break }
                }
            }
            loadFailed = false
        } catch is CancellationError {
        } catch {
            if generation == loadGeneration { loadFailed = true }
        }
    }

    private func target(for page: MarbleThreadPage) -> MarbleSendTarget {
        let conversationId = page.thread.conversationId
        return conversationId == everyoneConversationId
            ? .everyone(thread: id) : .conversation(conversationId, thread: id)
    }

    private func timeline(_ page: MarbleThreadPage) -> some View {
        let target = target(for: page)
        let conversationId = page.thread.conversationId
        let presentation = store.outgoingPresentation(page.entries, to: target, conversationId: conversationId)
        return MarbleJumpTimeline(
            entries: store.withLiveToolRows(presentation.entries, conversationId: conversationId, threadId: id),
            jump: jump, parentId: page.thread.parentEntryId, outgoing: store.lastOutgoing(to: target),
            workingNames: presentation.responders.map { id in
                store.prism.marbles.first { $0.id == id }?.name ?? page.thread.participants.first ?? "Marble"
            },
            loadOlder: { await store.loadOlderThread(id, beforePrepend: $0) }
        )
    }

    private func send(_ text: String) -> Bool {
        guard let page, store.send(text, to: target(for: page), entries: page.entries) else { return false }
        jump = nil
        return true
    }
}

struct MarbleSearchSection<Row: View>: View {
    @EnvironmentObject private var store: MarbleStore
    let query: String
    var threadId: String?
    var conversationId: String?
    @ViewBuilder var row: (MarbleSearchResult) -> Row
    @State private var hits: [MarbleSearchResult] = []
    @State private var loadedQuery: String?
    @State private var failed = false

    var body: some View {
        let visibleHits = hits.filter {
            !store.isDeletedConversation($0.conversationId) && (conversationId == nil || conversationId == $0.conversationId)
        }
        Section("Message matches") {
            if loadedQuery != query {
                ProgressView()
            } else if failed {
                Text("Couldn’t search messages.")
            } else if visibleHits.isEmpty {
                Text("No messages found.")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(visibleHits) { row($0) }
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
    var outgoing: UUID? = nil
    var workingNames: [String] = []
    var loadOlder: MarbleOlderHistoryLoader? = nil
    @State private var highlight: String?

    var body: some View {
        let transcript = MarbleReasoningAssociation(entries, keepVisible: jump?.entryId)
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 14) {
                    ForEach(transcript.standalone, id: \.rowID) { entry in
                        VStack(alignment: .leading, spacing: 10) {
                            MarbleEntryView(entry: entry, speaker: store.speakerStyle(for: entry),
                                            reasoning: transcript.attached[entry.id], allowsThreads: parentId == nil)
                            if entry.id == parentId && entries.count > 1 { Divider() }
                        }
                        .padding(.vertical, 3)
                        .background(highlight == entry.id ? Color.accentColor.opacity(0.15) : .clear,
                                    in: RoundedRectangle(cornerRadius: 8))
                    }
                    ForEach(workingNames, id: \.self) { MarbleThinkingBubble(name: $0) }
                    MarbleTranscriptEnd(bottomPadding: 16)
                }
                .marbleHistoryScrollTargets()
                .padding([.horizontal, .top], 16)
                .frame(maxWidth: 820)
                .frame(maxWidth: .infinity)
            }
            .scrollDismissesKeyboard(.interactively)
            .marbleFollowsLatest(
                hasMessages: !entries.isEmpty, searchJump: jump?.requestId, outgoing: outgoing, loadOlder: loadOlder
            )
            .task(id: jump.flatMap { target in entries.contains { $0.id == target.entryId } ? target : nil }) {
                guard let jump, let row = transcript.standalone.first(where: { $0.id == jump.entryId }) else { return }
                highlight = jump.entryId
                await Task.yield()
                guard !Task.isCancelled else { return }
                proxy.scrollTo(row.rowID, anchor: .center)
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
