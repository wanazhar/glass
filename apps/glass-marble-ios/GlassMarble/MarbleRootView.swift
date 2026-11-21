import SwiftUI

struct MarbleRootView: View {
    @EnvironmentObject private var store: MarbleStore
    @Environment(\.scenePhase) private var scenePhase
    @State private var showsSettings = false
    @State private var showsIntroduction = false
    @AppStorage("marble.introduced") private var introduced = false

    var body: some View {
        Group {
            if store.onboarded && !store.needsToken {
                NavigationSplitView {
                    ConversationListView()
                        .navigationSplitViewColumnWidth(min: 236, ideal: 286, max: 340)
                } detail: {
                    MarbleChatView()
                }
                .navigationSplitViewStyle(.balanced)
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
            guard !introduced else { return }
            introduced = true
            showsIntroduction = true
        }
        .sheet(isPresented: $showsSettings) {
            ConnectSettingsView()
                .environmentObject(store)
        }
        .sheet(isPresented: $showsIntroduction) {
            ConnectIntroductionView { showsIntroduction = false }
                .ignoresSafeArea()
        }
    }
}

struct MarbleChatView: View {
    @EnvironmentObject private var store: MarbleStore
    @State private var draft = ""
    @State private var showsSettings = false
    @State private var showsControl = false
    @State private var showsMemory = false
    @State private var showsRoutines = false
    @State private var showsPrism = false
    @State private var showsTools = false
    @StateObject private var dictation = MarbleDictation()

    var body: some View {
        VStack(spacing: 0) {
            approvalRail
            TranscriptView()
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
        .onChange(of: dictation.transcript) { _, transcript in
            guard !transcript.isEmpty else { return }
            draft = transcript
        }
        .navigationTitle(store.selectedConversation?.title ?? "Marble")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                MarbleStatusText(
                    label: store.statusLabel,
                    tone: store.connection == .live
                        ? .green
                        : (store.connection == .offline ? .red : .secondary)
                )
            }
            ToolbarItemGroup(placement: .topBarTrailing) {
                Button {
                    store.selectedConversationId = nil
                } label: {
                    Image(systemName: "square.and.pencil")
                }
                .accessibilityLabel("New conversation")
                // Five icon buttons crowded most phones; the work that is not
                // done every turn lives behind one menu instead.
                Menu {
                    Button {
                        showsControl = true
                    } label: {
                        Label("Marble control", systemImage: "shield.lefthalf.filled")
                    }
                    Button {
                        showsMemory = true
                    } label: {
                        Label("Memory", systemImage: "brain")
                    }
                    Button {
                        showsRoutines = true
                    } label: {
                        Label("Routines", systemImage: "clock.arrow.circlepath")
                    }
                    Button {
                        showsPrism = true
                    } label: {
                        Label("Prism protocol", systemImage: "hexagon")
                    }
                    Button {
                        showsTools = true
                    } label: {
                        Label("Tooling", systemImage: "wrench.and.screwdriver")
                    }
                    Button {
                        Task { await store.toggleVoice() }
                    } label: {
                        Label(
                            store.voice.enabled ? "Stop reading replies" : "Read replies aloud",
                            systemImage: store.voice.enabled ? "speaker.slash" : "speaker.wave.2"
                        )
                    }
                    Divider()
                    Button {
                        showsSettings = true
                    } label: {
                        Label("Host settings", systemImage: "antenna.radiowaves.left.and.right")
                    }
                } label: {
                    Image(systemName: "ellipsis")
                }
                .accessibilityLabel("Tools and settings")
            }
        }
        .sheet(isPresented: $showsSettings) {
            ConnectSettingsView()
                .environmentObject(store)
        }
        .sheet(isPresented: $showsControl) {
            ControlView()
                .environmentObject(store)
        }
        .sheet(isPresented: $showsMemory) {
            MemoryView()
                .environmentObject(store)
        }
        .sheet(isPresented: $showsRoutines) {
            RoutinesView()
                .environmentObject(store)
        }
        .sheet(isPresented: $showsPrism) {
            PrismView()
                .environmentObject(store)
        }
        .sheet(isPresented: $showsTools) {
            ToolsView()
                .environmentObject(store)
        }
        .overlay(alignment: .bottom) {
            if let message = store.errorMessage ?? dictation.errorMessage {
                Text(message)
                    .font(.caption)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 9)
                    .marbleFloating()
                    .padding(.bottom, 92)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
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
        }
    }

    private func send() {
        let text = draft
        draft = ""
        if dictation.isRecording { dictation.stop() }
        Task { await store.send(text) }
    }

    /// Send the draft into the running turn, keeping its context.
    private func steer() {
        let text = draft
        draft = ""
        if dictation.isRecording { dictation.stop() }
        Task { await store.steer(text) }
    }
}
