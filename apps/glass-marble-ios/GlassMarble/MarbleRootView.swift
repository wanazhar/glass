import SwiftUI

/// The app's navigation, owned here and nowhere else: a store refresh can
/// update data but can never push a chat back on screen.
final class MarbleNavigationModel: ObservableObject {
    @Published var state = MarbleNavigation()

    func setChatPath(_ path: [ChatRoute]) {
        // The NavigationStack writes its path back on user pops; that is the
        // one legitimate external writer.
        state.chatPath = path
    }

    func selectTab(_ tab: MarbleTab) { state.selectTab(tab) }
    func openConversation(_ id: String) { state.openConversation(id) }
    func openMarble(_ id: String) { state.openMarble(id) }
    func openThread(_ id: String) { state.openThread(id) }
    func openMarbleInfo(_ id: String, fromEveryone: Bool = false) {
        state.openMarbleInfo(id, fromEveryone: fromEveryone)
    }
    func setEveryonePath(_ path: [ChatRoute]) { state.everyonePath = path }
    func openNewChat(_ id: String?) { state.openNewChat(id) }
    func draftResolved(_ id: String) { state.draftResolved(id) }
    func deleted(_ id: String) { state.deleted(id) }
    func notificationTapped(_ id: String) { state.notificationTapped(id) }
    func queuePendingOpen(_ id: String) { state.queuePendingOpen(id) }
    func drainPendingOpen() -> String? { state.drainPendingOpen() }
    func signOut() { state.signOut() }
}

struct MarbleRootView: View {
    @EnvironmentObject private var store: MarbleStore
    @EnvironmentObject private var navigation: MarbleNavigationModel

    var body: some View {
        ZStack {
            MarbleTheme.paper.ignoresSafeArea()
            if store.onboarded {
                TabView(selection: Binding(
                    get: { navigation.state.tab }, set: { navigation.selectTab($0) }
                )) {
                    Tab("Agents", systemImage: "person.2", value: MarbleTab.marbles) {
                        MarblesTabView()
                    }
                    .accessibilityIdentifier("tab.marbles")
                    Tab("Everyone", systemImage: "bubble.left.and.bubble.right", value: MarbleTab.everyone) {
                        EveryoneTabView()
                    }
                    .accessibilityIdentifier("tab.everyone")
                    Tab("Activity", systemImage: "waveform.path", value: MarbleTab.activity) {
                        NavigationStack { ActivityView() }
                    }
                    .accessibilityIdentifier("tab.activity")
                    .badge(store.state.approvals.count + store.memory.suggestions.count)
                    Tab("Settings", systemImage: "gearshape", value: MarbleTab.settings) {
                        NavigationStack { SettingsView() }
                    }
                    .accessibilityIdentifier("tab.settings")
                }
            } else {
                MarbleOnboardingView(expiredToken: store.needsToken)
            }
        }
        .tint(MarbleTheme.tint)
        .task { store.start() }
        .onReceive(NotificationCenter.default.publisher(for: .marbleOpenConversation)) { note in
            guard let id = note.object as? String, !store.isDeletedConversation(id) else { return }
            navigation.notificationTapped(id)
        }
        .onChange(of: store.onboarded) { _, onboarded in
            if !onboarded { navigation.signOut() }
        }
    }
}

struct EveryoneTabView: View {
    @EnvironmentObject private var navigation: MarbleNavigationModel

    var body: some View {
        NavigationStack(path: Binding(
            get: { navigation.state.everyonePath }, set: { navigation.setEveryonePath($0) }
        )) {
            EveryoneView()
                .navigationDestination(for: ChatRoute.self) { route in
                    switch route {
                    case .marbleInfo(let id):
                        MarbleInfoView(marbleId: id)
                            .toolbarVisibility(.hidden, for: .tabBar)
                    case .conversation, .draft:
                        MarbleChatView(route: route)
                    default:
                        MarbleThreadDestination(route: route)
                    }
                }
        }
    }
}

struct MarblesTabView: View {
    @EnvironmentObject private var store: MarbleStore
    @EnvironmentObject private var navigation: MarbleNavigationModel
    @Environment(\.horizontalSizeClass) private var sizeClass

    var body: some View {
        Group {
            if sizeClass == .regular {
                NavigationSplitView {
                    MarblesView(
                        selection: navigation.state.chatPath.first,
                        onSelect: { navigation.setChatPath([$0]) }
                    )
                    .navigationSplitViewColumnWidth(min: 260, ideal: 320, max: 380)
                } detail: {
                    stack(root: {
                        VStack(spacing: 12) {
                            PocketPair().frame(width: 80, height: 80)
                            Text("Start a chat").font(.largeTitle.weight(.semibold))
                            Text("Choose an agent to start a conversation.")
                                .foregroundStyle(MarbleTheme.muted)
                        }
                        .multilineTextAlignment(.center)
                        .padding(24)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .background(MarbleTheme.paper)
                    })
                }
                .navigationSplitViewStyle(.balanced)
            } else {
                stack(root: { MarblesView() })
            }
        }
        .onAppear {
            if let id = navigation.drainPendingOpen(), !store.isDeletedConversation(id) {
                navigation.notificationTapped(id)
            }
        }
    }

    private func stack<Root: View>(@ViewBuilder root: () -> Root) -> some View {
        NavigationStack(path: Binding(
            get: { navigation.state.chatPath }, set: { navigation.setChatPath($0) }
        )) {
            root()
                .navigationDestination(for: ChatRoute.self) { route in
                    switch route {
                    case .marbleChats:
                        ConversationListView()
                    case .archivedChats:
                        ArchivedChatsView()
                    case .marbleChat(let id):
                        MarbleChatScreen(marbleId: id)
                    case .marbleInfo(let id):
                        MarbleInfoView(marbleId: id)
                    case .conversation, .draft:
                        MarbleChatView(route: route)
                    default:
                        MarbleThreadDestination(route: route)
                    }
                }
        }
    }
}
struct MarbleChatView: View {
    /// `.draft` for a not-yet-created chat, `.conversation(id)` otherwise.
    var route: ChatRoute

    @EnvironmentObject private var store: MarbleStore
    @EnvironmentObject private var navigation: MarbleNavigationModel
    @State private var draft = ""
    @StateObject private var dictation = MarbleDictation()

    private var conversationId: String? {
        if case let .conversation(id) = route { return id }
        return nil
    }
    @State private var renaming = false
    @State private var renameDraft = ""
    @State private var confirmDelete = false
    @State private var exportURL: ExportFile?

    private var currentTitle: String {
        conversationId.flatMap { store.state.conversation($0) }?.title ?? ""
    }

    var body: some View {
        TranscriptView(conversationId: conversationId)
        .marbleComposerInset {
            ComposerBar(
                draft: $draft,
                isWorking: store.isWorking,
                isDictating: dictation.isRecording,
                onSend: send,
                onSteer: steer,
                onStop: { Task { await store.cancelActiveTask() } },
                onDictate: { dictation.toggle() }
            )
        }
        .toolbarVisibility(.hidden, for: .tabBar)
        .onAppear { store.selectedConversationId = conversationId }
        .onDisappear {
            if store.selectedConversationId == conversationId {
                store.selectedConversationId = nil
            }
        }
        .onChange(of: dictation.transcript) { _, transcript in
            guard !transcript.isEmpty else { return }
            draft = transcript
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) {
                MarbleTitleView(title: conversationId.flatMap { store.state.conversation($0) }?.displayTitle ?? "New Chat")
                    .marbleToolbarTitle()
            }
            if conversationId != nil {
                ToolbarItem(placement: .topBarTrailing) {
                    Menu {
                        Button("Rename") { renameDraft = currentTitle; renaming = true }
                        if let conversationId {
                            let archived = store.state.conversation(conversationId)?.archived == true
                            Button(archived ? "Restore" : "Archive", systemImage: "archivebox") {
                                Task { await store.archiveConversation(conversationId, archived: !archived) }
                            }
                        }
                        Button("Export chat") {
                            Task { if let url = await store.exportFile(conversationId: conversationId) { exportURL = ExportFile(url: url) } }
                        }
                        Button("Delete", role: .destructive) { confirmDelete = true }
                    } label: {
                        Image(systemName: "ellipsis")
                    }
                    .accessibilityLabel("Chat actions")
                }
            }
            if let conversationId {
                ToolbarItem(placement: .topBarTrailing) {
                    MarbleThreadInboxLink(conversationId: conversationId)
                }
                ToolbarItem(placement: .topBarTrailing) {
                    NavigationLink(value: ChatRoute.find(conversationId)) {
                        Label("Find", systemImage: "magnifyingglass")
                    }
                }
            }
        }
        .alert("Rename chat", isPresented: $renaming) {
            TextField("Title", text: $renameDraft)
            Button("Cancel", role: .cancel) {}
            Button("Save") {
                if let conversation = store.state.conversation(conversationId ?? "") {
                    Task { await store.rename(conversation, title: renameDraft) }
                }
            }
        }
        .confirmationDialog("Delete this chat?", isPresented: $confirmDelete, titleVisibility: .visible) {
            Button("Delete", role: .destructive) {
                if let conversationId {
                    Task {
                        if await store.delete(conversationId: conversationId) {
                            navigation.deleted(conversationId)
                        }
                    }
                }
            }
        }
        .sheet(item: $exportURL) { file in
            ShareLink(item: file.url) { Label("Share \(file.url.lastPathComponent)", systemImage: "square.and.arrow.up") }
                .padding()
        }
        .overlay(alignment: .bottom) {
            if let message = store.errorMessage ?? dictation.errorMessage {
                Text(message)
                    .font(.caption)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 9)
                    .marbleFloating()
                    .padding(.bottom, 8)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                    .allowsHitTesting(false)
            }
        }
    }

    private func send(_ text: String) -> Bool {
        if dictation.isRecording { dictation.stop() }
        return store.send(text, to: .conversation(conversationId), entries: store.transcript(for: conversationId)) { [weak navigation] id in
            if case .draft = route { navigation?.draftResolved(id) }
        }
    }

    /// Send the draft into the running turn, keeping its context.
    private func steer(_ text: String) async -> Bool {
        if dictation.isRecording { dictation.stop() }
        return await store.steer(text, conversationId: store.selectedConversationId, taskId: store.activeTaskId)
    }
}

/// The share sheet needs an Identifiable payload.
private struct ExportFile: Identifiable {
    var url: URL
    var id: String { url.absoluteString }
}
