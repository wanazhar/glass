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
    func openMarbleInfo(_ id: String, fromEveryone: Bool = false) {
        state.openMarbleInfo(id, fromEveryone: fromEveryone)
    }
    func setEveryonePath(_ path: [ChatRoute]) { state.everyonePath = path }
    func newChat() { state.newChat() }
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
    @Environment(\.scenePhase) private var scenePhase
    @State private var showsIntroduction = false
    @AppStorage("marble.introduced") private var introduced = false
    @SceneStorage("marble.tab") private var storedTab = MarbleTab.everyone.rawValue

    var body: some View {
        Group {
            if store.onboarded && !store.needsToken {
                // Three tabs, icons only; each keeps its spoken name for
                // VoiceOver and UI tests. Activity and Scheduled live in Settings.
                TabView(selection: tabBinding) {
                    Tab(value: MarbleTab.everyone) {
                        EveryoneTabView()
                    } label: {
                        MarbleTabIcon(title: "Everyone", systemImage: "bubble.left.and.bubble.right")
                    }
                    Tab(value: MarbleTab.marbles) {
                        MarblesTabView()
                    } label: {
                        MarbleTabIcon(title: "Marbles", systemImage: "circle.hexagongrid")
                    }
                    Tab(value: MarbleTab.settings) {
                        NavigationStack { SettingsView() }
                    } label: {
                        MarbleTabIcon(title: "Settings", systemImage: "gearshape")
                    }
                    .badge(store.state.approvals.count + store.memory.suggestions.count)
                }
            } else {
                // No usable host yet: onboarding is the whole screen, never an
                // empty product UI pretending it is connected.
                MarbleOnboardingView(expiredToken: store.needsToken)
            }
        }
        .task { store.start() }
        .onChange(of: scenePhase) { _, phase in
            switch phase {
            case .active: store.appBecameActive()
            case .background, .inactive: store.appBecameInactive()
            @unknown default: break
            }
        }
        .onAppear {
            if let restored = MarbleTab(rawValue: storedTab) {
                navigation.selectTab(restored)
            }
            guard !introduced else { return }
            introduced = true
            showsIntroduction = true
        }
        .onReceive(NotificationCenter.default.publisher(for: .marbleOpenConversation)) { note in
            guard let id = note.object as? String else { return }
            navigation.notificationTapped(id)
        }
        .onChange(of: store.onboarded) { _, onboarded in
            if !onboarded { navigation.signOut() }
        }
        .sheet(isPresented: $showsIntroduction) {
            ConnectIntroductionView { showsIntroduction = false }
                .ignoresSafeArea()
        }
    }

    private var tabBinding: Binding<MarbleTab> {
        Binding(
            get: { navigation.state.tab },
            set: { newTab in
                navigation.selectTab(newTab)
                storedTab = newTab.rawValue
            }
        )
    }
}

private struct MarbleTabIcon: View {
    var title: String
    var systemImage: String

    var body: some View {
        Image(systemName: systemImage)
            .accessibilityLabel(Text(title))
    }
}

/// The Everyone tab: the shared chat; avatar taps push a marble's info.
struct EveryoneTabView: View {
    @EnvironmentObject private var navigation: MarbleNavigationModel

    var body: some View {
        NavigationStack(path: everyonePath) {
            EveryoneView()
                .navigationDestination(for: ChatRoute.self) { route in
                    switch route {
                    case .marbleInfo(let id): MarbleInfoView(marbleId: id)
                    default: EveryoneView()
                    }
                }
        }
    }

    private var everyonePath: Binding<[ChatRoute]> {
        Binding(
            get: { navigation.state.everyonePath },
            set: { navigation.setEveryonePath($0) }
        )
    }
}

/// The Marbles tab: the roster on top; rows push a marble's chat — or, for
/// the default Marble, the chats list that every conversation lives in.
struct MarblesTabView: View {
    @EnvironmentObject private var store: MarbleStore
    @EnvironmentObject private var navigation: MarbleNavigationModel

    var body: some View {
        NavigationStack(path: chatPath) {
            MarblesView()
                .navigationDestination(for: ChatRoute.self) { route in
                    switch route {
                    case .marbleChats:
                        ConversationListView()
                    case .marbleChat(let id):
                        MarbleChatScreen(marbleId: id)
                    case .marbleInfo(let id):
                        MarbleInfoView(marbleId: id)
                    default:
                        MarbleChatView(route: route)
                    }
                }
        }
        .onAppear {
            // A notification that launched the app arrived before this stack
            // existed; honour it now that the path can hold it.
            if let id = navigation.drainPendingOpen() {
                navigation.notificationTapped(id)
            }
        }
    }

    private var chatPath: Binding<[ChatRoute]> {
        Binding(
            get: { navigation.state.chatPath },
            set: { navigation.setChatPath($0) }
        )
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
        TranscriptView()
        .safeAreaInset(edge: .top) {
            approvalRail
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            ComposerBar(
                draft: $draft,
                isWorking: store.isWorking,
                isDictating: dictation.isRecording,
                onSend: send,
                onSteer: steer,
                onStop: { Task { await store.cancelActiveTask() } },
                onDictate: { dictation.toggle() }
            )
            .marbleComposerBackground()
        }
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
                MarbleTitleView(title: conversationId.flatMap { store.state.conversation($0) }?.title ?? "Marble")
            }
            if conversationId != nil {
                ToolbarItem(placement: .topBarTrailing) {
                    Menu {
                        Button("Rename") { renameDraft = currentTitle; renaming = true }
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
                if let conversation = store.state.conversation(conversationId ?? "") {
                    let id = conversation.id
                    Task {
                        await store.delete(conversation)
                        if !store.state.conversations.contains(where: { $0.id == id }) {
                            navigation.deleted(id)
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

    @ViewBuilder
    private var approvalRail: some View {
        if !store.memory.suggestions.isEmpty || !store.state.approvals.isEmpty {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 12) {
                    ForEach(store.memory.suggestions) { suggestion in
                        SuggestionCard(suggestion: suggestion) { keep in
                            Task {
                                if keep {
                                    await store.accept(suggestion)
                                } else {
                                    await store.dismiss(suggestion)
                                }
                            }
                        }
                        .frame(width: 300)
                    }
                    ForEach(store.state.approvals) { approval in
                        ApprovalCard(approval: approval) { allow, remember in
                            Task {
                                await store.decide(
                                    approval,
                                    allow: allow,
                                    remember: remember
                                )
                            }
                        }
                        .frame(width: 300)
                    }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
            }
            .background(.bar)
        }
    }

    private func send() {
        let text = draft
        draft = ""
        if dictation.isRecording { dictation.stop() }
        Task {
            await store.send(text)
            if case .draft = route, let id = store.selectedConversationId {
                navigation.draftResolved(id)
            }
        }
    }

    /// Send the draft into the running turn, keeping its context.
    private func steer() {
        let text = draft
        if dictation.isRecording { dictation.stop() }
        Task {
            let accepted = await store.steer(text)
            draft = MarbleSteer.draft(after: text, current: draft, accepted: accepted)
        }
    }
}

/// The share sheet needs an Identifiable payload.
private struct ExportFile: Identifiable {
    var url: URL
    var id: String { url.absoluteString }
}
