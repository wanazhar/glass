import GlassModels
import SwiftUI

struct ConversationView: View {
    @Bindable var conversation: ConversationModel
    var threads: ThreadsModel
    var goBack: () -> Void
    @Environment(\.scenePhase) private var scenePhase
    @State private var following = true
    @State private var atBottom = true
    @State private var showingInfo = false
    @State private var activityDetail: ActivityDetail?
    @State private var interactionHeight: CGFloat = 0
    @State private var picker: ComposerModel.Picker?
    @State private var showingChecklist = false
    @State private var showingDetails = false
    @State private var savedRecord: SavedRecord?
    @State private var actionError: String?
    @State private var pendingEdit: TimelineEntry?
    @State private var previewPage: PreviewPage?
    @State private var referencedFile: ReferencedFile?
    @AppStorage(ToolDensity.key) private var density: ToolDensity = .compact
    @Environment(AppModel.self) private var app

    var body: some View {
        ScrollViewReader { proxy in
            VStack(spacing: 0) {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: Design.padding) {
                        if conversation.hasEarlier {
                            Button {
                                Task { await conversation.loadOlder() }
                            } label: {
                                if conversation.loadingOlder { ProgressView() }
                                else { Label("Earlier messages", systemImage: "arrow.up") }
                            }
                            .glassButtonStyle()
                            .frame(maxWidth: .infinity)
                            .accessibilityIdentifier("conversation.older")
                        }
                        if conversation.loadedHistory, conversation.timeline.isEmpty {
                            emptyState
                        }
                        ForEach(rows) { row in
                            switch row {
                            case .message(let entry): TimelineRow(entry: entry)
                            case .work(let group):
                                WorkRow(group: group, durations: conversation.timeline.toolDurations, open: { detail in
                                    activityDetail = detail
                                }, resume: conversation.canWrite && !conversation.timeline.isActive ? {
                                    Task { await conversation.composer.continueFromFailure(canWrite: conversation.canWrite) }
                                } : nil)
                            }
                        }
                        Color.clear.frame(height: 1)
                            .id(Self.bottom)
                    }
                    .padding(.horizontal, Design.padding)
                    .padding(.vertical, Design.gap)
                    .frame(maxWidth: Design.readingWidth)
                    .frame(maxWidth: .infinity)
                }
                .defaultScrollAnchor(.bottom, for: .initialOffset)
                .defaultScrollAnchor(conversation.timeline.isEmpty ? .center : .bottom, for: .alignment)
                .softTopScrollEdge()
                .scrollDismissesKeyboard(.interactively)
                .onScrollGeometryChange(for: Bool.self) { geometry in
                    geometry.visibleRect.maxY >= geometry.contentSize.height - Design.gap
                } action: { _, bottom in
                    atBottom = bottom
                }
                .onScrollGeometryChange(for: CGSize.self) { geometry in
                    CGSize(width: geometry.contentSize.height, height: geometry.containerSize.height - geometry.contentInsets.bottom)
                } action: { _, _ in
                    guard following else { return }
                    Task { @MainActor in
                        if following { proxy.scrollTo(Self.bottom, anchor: .bottom) }
                    }
                }
                .onScrollPhaseChange { old, phase in
                    if phase == .interacting { following = false }
                    if phase == .idle, old != .animating { following = atBottom }
                }
                .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardDidShowNotification)) { _ in
                    if following { proxy.scrollTo(Self.bottom, anchor: .bottom) }
                }
                .onChange(of: conversation.timeline.interaction?.id) { _, id in
                    if id != nil { resume(proxy) }
                }
                .onChange(of: rows.last?.id) { _, _ in
                    if following { proxy.scrollTo(Self.bottom, anchor: .bottom) }
                }
                .overlay(alignment: .bottomTrailing) { floatingControls(proxy) }
                .floatingBar(edge: .top) { header }
                bottomBar(onFieldFocus: { resume(proxy) })
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("conversation")
        .accessibilityValue("\(conversation.timeline.entries.count) entries\(conversation.hasEarlier ? ", earlier available" : "")")
        .toolbarVisibility(.hidden, for: .navigationBar)
        .sheet(item: $picker) { ChoicePicker(picker: $0, composer: conversation.composer) }
        .sheet(isPresented: $showingInfo) { threadInfo }
        .sheet(item: $activityDetail) { detail in
            if detail.command == nil, !detail.isDiff, let offset = conversation.timeline.savedOutputs[detail.id] {
                SavedActivityView(conversation: conversation, record: offset, title: detail.title)
            } else {
                ActivityDetailView(detail: detail, savedOutput: conversation.timeline.savedOutputs[detail.id].map { offset in
                    SavedActivityView(conversation: conversation, record: offset, title: detail.title)
                })
            }
        }
        .sheet(isPresented: $showingDetails) { DetailsView(conversation: conversation) }
        .sheet(item: $savedRecord) { SavedActivityView(conversation: conversation, record: $0.id) }
        .environment(\.openSavedActivity) { savedRecord = SavedRecord(id: $0) }
        .environment(\.messageAction) { entry, action in Task { await perform(action, on: entry) } }
        .environment(\.openReferencedFile) { referencedFile = ReferencedFile(id: $0) }
        .sheet(item: $referencedFile) { file in
            NavigationStack {
                FileView(conversation: conversation, name: file.id)
                    .toolbar {
                        ToolbarItem(placement: .confirmationAction) { Button("Done") { referencedFile = nil } }
                    }
            }
        }
        .environment(\.previewHTML) { previewPage = PreviewPage(html: $0) }
        .environment(\.composeText) { text in
            let current = conversation.composer.text.trimmingCharacters(in: .whitespacesAndNewlines)
            conversation.composer.text = current.isEmpty ? text : current + "\n\n" + text
            UIAccessibility.post(notification: .announcement, argument: "Added to your message")
        }
        .sheet(item: $previewPage) { ArtifactView(conversation: conversation, html: Data($0.html.utf8), title: "Preview") }
        .alert("Couldn’t do that", isPresented: Binding(get: { actionError != nil }, set: { if !$0 { actionError = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(actionError ?? "")
        }
        .confirmationDialog("Edit in a new branch?", isPresented: Binding(get: { pendingEdit != nil }, set: { if !$0 { pendingEdit = nil } }),
                            titleVisibility: .visible) {
            Button("Create branch") {
                if let entry = pendingEdit { Task { await perform(.editInBranch, on: entry, confirmed: true) } }
            }
        } message: {
            Text("Glass makes a branch before this message and puts it in the message box to edit. Files stay as they are.")
        }
        .sheet(isPresented: $showingChecklist) { ChecklistSheet(tasks: conversation.composer.checklist) }
        .sheet(isPresented: Bindable(conversation.composer).showingCommands) {
            CommandPaletteView(conversation: conversation) { perform($0) }
        }
        .background {
            // Hardware-keyboard shortcuts, as in the web UI: ⌘K and ⌘/ open the palette.
            Group {
                Button("Commands") { conversation.composer.showingCommands = true }.keyCommand(.commands)
                Button("Commands") { conversation.composer.showingCommands = true }.keyCommand(.commandsAlternate)
            }
            .opacity(0)
            .allowsHitTesting(false)
            .accessibilityHidden(true)
        }
        .environment(\.attachmentLoader) { [composer = conversation.composer] id in await composer.attachment(id) }
        .task { await conversation.start() }
        .onChange(of: scenePhase) { _, phase in
            // iOS suspends sockets in the background. The cached chat stays on screen and a fresh
            // stream resumes from the last event id the moment Glass is active again.
            switch phase {
            case .active: Task { await conversation.resumeNow() }
            case .background: conversation.suspend()
            default: conversation.composer.flushDraft()
            }
        }
    }


    private func perform(_ local: LocalCommand) {
        switch local {
        case .help: conversation.composer.showingCommands = true
        case .diagnostics, .layout: app.show(.diagnostics)
        case .clear: Task { await threads.newThread() }
        case .tools: density = density == .compact ? .expanded : .compact
        case .editor: conversation.composer.showingEditor = true
        case .animation, .theme: app.show(.appearance)
        case .settings: app.show(nil)
        case .conversations, .search: goBack()
        }
    }
    private static let bottom = "timeline.bottom"

    private var emptyState: some View {
        NewChatView(workspace: conversation.thread.workspace) { suggestion in
            conversation.composer.text = suggestion
        }
        .accessibilityIdentifier("conversation.empty")
    }

    @ViewBuilder
    private func bottomBar(onFieldFocus: @escaping () -> Void) -> some View {
        VStack(spacing: 8) {
            if let interaction = conversation.timeline.interaction {
                // The decision is docked above the composer so it stays in view,
                // scrolling on its own when it is taller than the space allows.
                ScrollView {
                    InteractionView(interaction: interaction, composer: conversation.composer,
                                    canWrite: conversation.canWrite, secretsAllowed: secretsBlocked)
                        .id(interaction.id)
                        .onGeometryChange(for: CGFloat.self, of: \.size.height) { interactionHeight = $1 }
                }
                .scrollBounceBehavior(.basedOnSize)
                .frame(height: min(max(interactionHeight, 44), 340))
            }
            if let notice = conversation.timeline.notice
                ?? (conversation.error ?? conversation.composer.error).map({ Timeline.Notice(text: $0, isError: true) })
                ?? conversation.composer.confirmation.map({ Timeline.Notice(text: $0, isError: false) }) {
                HStack(alignment: .top, spacing: 10) {
                    Image(systemName: notice.isError ? "exclamationmark.triangle.fill" : "info.circle.fill")
                        .foregroundStyle(notice.isError ? .red : .accentColor)
                    Text(notice.text).font(.footnote).frame(maxWidth: .infinity, alignment: .leading)
                    Button("Dismiss", systemImage: "xmark") {
                        conversation.dismissNotice()
                        conversation.error = nil
                        conversation.composer.error = nil
                        conversation.composer.confirmation = nil
                    }
                    .labelStyle(.iconOnly)
                    .font(.footnote)
                }
                .padding(12)
                .glassPanel(cornerRadius: 18)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("conversation.notice")
            }
            if conversation.timeline.isActive {
                HStack {
                    StatusLabel(title: conversation.timeline.activity ?? "Working…", color: Design.accent)
                        .font(.footnote).foregroundStyle(.secondary).lineLimit(1)
                    Spacer()
                    Button("Stop", systemImage: "stop.fill") {
                        UIImpactFeedbackGenerator(style: .light).impactOccurred()
                        Task { await conversation.composer.stop() }
                    }
                        .font(.footnote)
                        .keyCommand(.stop)
                        .frame(minWidth: Design.control, minHeight: Design.control)
                        .disabled(!conversation.canWrite)
                        .accessibilityIdentifier("composer.stop")
                }
                .padding(.horizontal, Design.gap)
            }
            ComposerView(conversation: conversation, onFieldFocus: onFieldFocus) { picker = .model }
        }
        .padding(.horizontal, 16)
        .padding(.bottom, 6)
        .frame(maxWidth: 760)
    }

    /// Floats on its own over the chat, just above the composer, with no panel behind it.
    private func floatingControls(_ proxy: ScrollViewProxy) -> some View {
        HStack(spacing: 8) {
            if let tasks = Optional(conversation.composer.checklist), !tasks.isEmpty {
                Button("\(tasks.filter(\.isDone).count) of \(tasks.count)", systemImage: "checklist") { showingChecklist = true }
                    .font(.caption)
                    .glassButtonStyle()
                    .accessibilityIdentifier("conversation.checklist")
            }
            if !following, !atBottom {
                Button("Jump to latest", systemImage: "arrow.down") { resume(proxy) }
                    .glassButtonStyle()
                    .accessibilityIdentifier("conversation.latest")
                    .transition(.opacity.combined(with: .scale(scale: 0.9)))
            }
        }
        .padding(.horizontal, 16)
        .padding(.bottom, 8)
        .animation(.easeOut(duration: 0.2), value: following || atBottom)
    }

    private func resume(_ proxy: ScrollViewProxy) {
        following = true
        withAnimation(.smooth(duration: 0.3)) { proxy.scrollTo(Self.bottom, anchor: .bottom) }
    }

    private var secretsBlocked: String? {
        guard app.endpoint?.isSecure == true else { return "Secrets are only sent over HTTPS. Use the Tailscale HTTPS link for this host." }
        guard app.secretEntryUnlocked else { return "Secret entry is locked. Pair again with a secret-entry link from the host to enable it." }
        return nil
    }

    private var header: some View {
        GlassGroup {
            HStack(spacing: Design.gap) {
                Button("Conversations", systemImage: "sidebar.left", action: goBack)
                    .labelStyle(.iconOnly)
                    .glassButtonStyle()
                    .accessibilityIdentifier("conversation.back")
                Button { showingInfo = true } label: {
                    Text(threads.thread(conversation.thread.id)?.displayTitle ?? conversation.title)
                        .font(Design.headline)
                        .lineLimit(1)
                        .frame(maxWidth: .infinity, minHeight: Design.control)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("conversation.connection")
                .accessibilityValue(connectionStatus)
                Button("New Chat", systemImage: "square.and.pencil") {
                    Task { await threads.newThread() }
                }
                .labelStyle(.iconOnly)
                .glassButtonStyle()
                .accessibilityIdentifier("conversation.new")
            }
        }
        .padding(.horizontal, Design.padding)
        .padding(.vertical, Design.small)
        .frame(maxWidth: Design.readingWidth)
        .frame(maxWidth: .infinity)
    }

    private var threadInfo: some View {
        ThreadInfoView(conversation: conversation, title: threads.thread(conversation.thread.id)?.displayTitle ?? conversation.title) { action in
            showingInfo = false
            Task { @MainActor in
                try? await Task.sleep(for: .milliseconds(350))
                switch action {
                case .details: showingDetails = true
                case .commands: conversation.composer.showingCommands = true
                case .permissions: picker = .profiles
                }
            }
        }
    }

    private var rows: [ChatRow] {
        ChatRow.make(from: conversation.timeline)
    }

    private var connectionStatus: String {
        switch conversation.connection {
        case .live: "Live"
        case .idle, .connecting: "Connecting"
        case .reconnecting: "Reconnecting"
        case .disconnected: "Disconnected"
        }
    }

    private struct ReferencedFile: Identifiable { let id: String }
}

extension ConversationView {
    fileprivate func perform(_ action: MessageAction, on entry: TimelineEntry, confirmed: Bool = false) async {
        guard let offset = entry.recordOffset, let api = app.api else { return }
        guard conversation.canWrite else {
            actionError = "Take control of the conversation first."
            return
        }
        if action == .editInBranch && !confirmed {
            pendingEdit = entry
            return
        }
        do {
            let thread = conversation.thread
            let location = try await api.activityLocation(record: offset, thread: thread.id, sourceRef: thread.sourceRef)
            let command = action.command(at: location)
            let result = try await conversation.composer.command(command.name, command.values)
            let opened = result["body"]?["open_thread"]?.stringValue ?? result["open_thread"]?.stringValue
            if let opened {
                await threads.reload()
                threads.selection = opened
                if action == .editInBranch, case .user(_, let text) = entry {
                    app.drafts.write(opened, text)
                }
            } else if action == .quote {
                conversation.composer.confirmation = "Quoted. It goes with your next messages."
            }
        } catch {
            actionError = error.localizedDescription
        }
    }
}

private struct SavedRecord: Identifiable { let id: Int }

private struct PreviewPage: Identifiable {
    let id = UUID()
    let html: String
}
