import SwiftUI

struct MarbleRootView: View {
    @EnvironmentObject private var store: MarbleStore
    @State private var showsSettings = false
    @State private var showsIntroduction = false
    @State private var greeted = false

    var body: some View {
        NavigationSplitView {
            ConversationListView()
                .navigationSplitViewColumnWidth(min: 236, ideal: 286, max: 340)
        } detail: {
            MarbleChatView()
        }
        .navigationSplitViewStyle(.balanced)
        .background(MarbleBackdrop())
        .tint(MarblePalette.accent)
        .task { store.start() }
        .onAppear {
            guard !greeted else { return }
            greeted = true
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
        .alert("Access token required", isPresented: $store.needsToken) {
            Button("Open host settings") { showsSettings = true }
        } message: {
            Text("This Marble host is shared over a private network. Paste the token configured on the host machine.")
        }
    }
}

struct MarbleChatView: View {
    @EnvironmentObject private var store: MarbleStore
    @State private var draft = ""
    @State private var showsSettings = false

    var body: some View {
        VStack(spacing: 0) {
            approvalRail
            TranscriptView()
            ComposerBar(
                draft: $draft,
                isWorking: store.isWorking,
                onSend: send,
                onStop: { Task { await store.cancelActiveTask() } }
            )
        }
        .background(Color.clear)
        .navigationTitle(store.selectedConversation?.title ?? "Marble")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarLeading) {
                MarbleStatusPill(
                    label: store.statusLabel,
                    tone: store.connection == .live
                        ? MarblePalette.success
                        : (store.connection == .offline ? MarblePalette.danger : MarblePalette.muted)
                )
            }
            ToolbarItemGroup(placement: .topBarTrailing) {
                MarbleIconButton(systemName: "square.and.pencil", label: "New conversation") {
                    store.selectedConversationId = nil
                }
                MarbleIconButton(systemName: "antenna.radiowaves.left.and.right", label: "Host settings") {
                    showsSettings = true
                }
            }
        }
        .sheet(isPresented: $showsSettings) {
            ConnectSettingsView()
                .environmentObject(store)
        }
        .overlay(alignment: .bottom) {
            if let message = store.errorMessage {
                Text(message)
                    .font(.caption)
                    .foregroundStyle(MarblePalette.ink)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 9)
                    .marbleGlass(cornerRadius: 18)
                    .padding(.bottom, 92)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
    }

    @ViewBuilder
    private var approvalRail: some View {
        if !store.state.approvals.isEmpty {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 12) {
                    ForEach(store.state.approvals) { approval in
                        ApprovalCard(approval: approval) { allow in
                            Task { await store.decide(approval, allow: allow) }
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
        Task { await store.send(text) }
    }
}
