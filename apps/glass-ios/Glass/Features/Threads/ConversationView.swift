import GlassModels
import SwiftUI

struct ConversationView: View {
    @Bindable var conversation: ConversationModel
    var threads: ThreadsModel
    var goBack: () -> Void
    var openAgent: (Teammate) -> Void = { _ in }
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
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
                        if conversation.parent == nil { emptyState }
                    }
                    if conversation.parent != nil, conversation.timeline.isEmpty {
                        TimelineRow(entry: .user(id: "assignment", text: conversation.agent?.task.flatMap { $0.isEmpty ? nil : $0 }
                            ?? "Assignment sent to \(conversation.title)."), endsTurn: false)
                    }
                    let rows = rows
                    let turnEnds = Self.turnEnds(rows, active: conversation.timeline.isActive)
                    ForEach(rows) { row in
                        switch row {
                        case .message(let entry): TimelineRow(entry: entry, endsTurn: turnEnds.contains(row.id))
                        case .work(let group):
                            WorkRow(group: group, durations: conversation.timeline.toolDurations, open: { detail in
                                activityDetail = detail
                            })
                        case .agents(let entry):
                            AgentGroupView(entry: entry, conversation: conversation, open: openAgent)
                        }
                    }
                    Color.clear.frame(height: Design.transcriptEnd)
                        .id(Self.bottom)
                }
                .padding(.horizontal, Design.padding)
                .padding(.top, Design.gap)
                .frame(maxWidth: Design.readingWidth)
                .frame(maxWidth: .infinity)
            }
            .defaultScrollAnchor(.bottom, for: .initialOffset)
            .defaultScrollAnchor(conversation.timeline.isEmpty ? .center : .bottom, for: .alignment)
            .scrollEdgeEffectStyle(.soft, for: .top)
            .scrollDismissesKeyboard(.interactively)
            .onScrollGeometryChange(for: Bool.self) { geometry in
                geometry.visibleRect.maxY >= geometry.contentSize.height - Design.gap
            } action: { _, bottom in
                atBottom = bottom
            }
            // The bottom stays pinned while the reply grows, every frame, without an animated scroll that can
            // land on a lazy row's estimated position and leave the transcript blank.
            .defaultScrollAnchor(following ? .bottom : nil, for: .sizeChanges)
            .onScrollGeometryChange(for: CGFloat.self) { geometry in
                geometry.containerSize.height - geometry.contentInsets.bottom
            } action: { _, _ in
                if following { proxy.scrollTo(Self.bottom, anchor: .bottom) }
            }
            .onScrollPhaseChange { old, phase in
                if phase == .interacting { following = false }
                if phase == .idle, old != .animating { following = atBottom }
            }
            .onChange(of: conversation.timeline.interaction?.id) { _, id in
                if id != nil { resume(proxy) }
            }
            .onChange(of: rows.last?.id) { _, _ in
                if following { proxy.scrollTo(Self.bottom, anchor: .bottom) }
            }
            .onChange(of: conversation.timeline.isActive) { _, _ in
                // The finished reply replaces the streamed one; settle on its real height, not the estimate.
                guard following else { return }
                Task { @MainActor in proxy.scrollTo(Self.bottom, anchor: .bottom) }
            }
            .safeAreaInset(edge: .top, spacing: Design.gap) {
                header
            }
            .safeAreaInset(edge: .bottom, spacing: 0) {
                VStack(spacing: Design.gap) {
                    if conversation.parent != nil {
                        agentFooter
                    } else {
                        checklistButton
                        bottomBar(onFieldFocus: { resume(proxy) })
                            .overlay(alignment: .top) { jumpToLatest(proxy) }
                    }
                }
                .frame(maxWidth: .infinity)
                .hiddenUnderSidebar()
            }
            .onChange(of: conversation.firstRequest) { _, request in
                guard conversation.parent == nil, let request else { return }
                Task { await threads.retitle(conversation.thread.id, from: request) }
            }
        }
        .background(Design.background.ignoresSafeArea())
        .foregroundStyle(Design.ink)
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
        .sheet(isPresented: $showingDetails) {
            DetailsView(conversation: conversation) { agent in
                showingDetails = false
                openAgent(agent)
            }
        }
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
        .task { if conversation.parent == nil { await conversation.start() } }
        .task(id: conversation.agent?.status) {
            if conversation.parent != nil { await conversation.watchChild() }
        }
        #if DEBUG
        .onChange(of: conversation.canWrite, initial: true) { _, writable in
            guard writable, let preview = LensPreview.current else { return }
            if preview == .modelPicker { picker = .model }
            if preview == .threadInfo { showingInfo = true }
            if preview == .diff, let diff = LensPreview.firstDiff(in: conversation.timeline) { activityDetail = diff }
        }
        #endif
        .onChange(of: scenePhase) { _, phase in
            guard conversation.parent == nil else { return }
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
        VStack(spacing: Design.gap) {
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
                .frame(height: min(max(interactionHeight, Design.control), 340))
            }
            if let notice = conversation.timeline.notice.flatMap({ $0.isError && latestFailure != nil ? nil : $0 })
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
                    .font(Design.footnote)
                    .frame(minWidth: Design.control, minHeight: Design.control)
                    .contentShape(Rectangle())
                }
                .padding(12)
                .glassPanel(cornerRadius: 18)
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("conversation.notice")
            }
            if let failure = latestFailure, conversation.canWrite {
                RunRecovery(group: failure, open: { activityDetail = $0 }) {
                    Task { await conversation.composer.continueFromFailure(canWrite: conversation.canWrite) }
                }
            }
            ComposerView(conversation: conversation, onFieldFocus: onFieldFocus) { picker = .model }
        }
        .padding(.horizontal, Design.padding)
        .padding(.bottom, 6)
        .frame(maxWidth: Design.readingWidth)
    }

    private var latestFailure: WorkGroup? {
        guard conversation.canWrite, !conversation.timeline.isActive, !conversation.timeline.disconnected else { return nil }
        let notice = conversation.timeline.notice.flatMap { $0.isError ? $0.text : nil }
        for row in rows.reversed() {
            switch row {
            case .work(let group):
                if group.runFailed { return group }
                if let notice {
                    return WorkGroup(id: group.id,
                                     steps: group.steps + [.notice(id: "latest-error", text: notice, isError: true)],
                                     activity: nil, seconds: group.seconds)
                }
                return nil
            case .message(let entry):
                if case .user = entry { break }
                continue
            case .agents:
                continue
            }
            break
        }
        guard let notice else { return nil }
        return WorkGroup(id: "latest-error", steps: [.notice(id: "latest-error", text: notice, isError: true)],
                         activity: nil, seconds: nil)
    }

    @ViewBuilder
    private var checklistButton: some View {
        let tasks = conversation.composer.checklist
        if !tasks.isEmpty {
            Button("\(tasks.filter(\.isDone).count) of \(tasks.count)", systemImage: "checklist") { showingChecklist = true }
                .font(.caption)
                .glassButtonStyle()
                .accessibilityIdentifier("conversation.checklist")
        }
    }

    /// Shown only once the reader has scrolled away from the newest message, floating clear of the composer
    /// so it never changes the transcript's layout.
    private func jumpToLatest(_ proxy: ScrollViewProxy) -> some View {
        let shown = !following && !atBottom
        return ZStack {
            if shown {
                Button { resume(proxy) } label: {
                    Image(systemName: "arrow.down")
                        .font(.system(size: 16, weight: .semibold))
                        .foregroundStyle(Design.ink)
                        .frame(width: Design.control, height: Design.control)
                        .contentShape(.circle)
                        // Tinted with the page colour so the arrow reads over text and code in both modes.
                        .lensGlass(Circle(), tint: Design.background.opacity(0.6), interactive: true)
                        .shadow(color: .black.opacity(0.18), radius: 8, y: 2)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Jump to latest")
                .accessibilityIdentifier("conversation.latest")
                .accessibilityHint("Returns to the newest message")
                .transition(.opacity)
            }
        }
        .alignmentGuide(.top) { $0[.bottom] + Design.padding }
        .animation(reduceMotion ? nil : .easeOut(duration: 0.2), value: shown)
    }

    private func resume(_ proxy: ScrollViewProxy) {
        following = true
        withAnimation(reduceMotion ? nil : .smooth(duration: 0.3)) { proxy.scrollTo(Self.bottom, anchor: .bottom) }
    }

    private var secretsBlocked: String? {
        guard app.endpoint?.isSecure == true else { return "Secrets are only sent over HTTPS. Use the Tailscale HTTPS link for this host." }
        guard app.secretEntryUnlocked else { return "Secret entry is locked. Pair again with a secret-entry link from the host to enable it." }
        return nil
    }

    @ViewBuilder
    private var header: some View {
        if conversation.parent != nil {
            ChatToolbar(menu: goBack, newChat: nil) {
                VStack(spacing: 1) {
                    Text(conversation.title).font(Design.headline).lineLimit(1)
                    if let model = conversation.agent?.friendlyModel {
                        Text(model).font(.caption2).foregroundStyle(Design.muted)
                    }
                }
                .frame(maxWidth: .infinity, minHeight: Design.control)
                .accessibilityIdentifier("agent.title")
            }
        } else {
            ChatToolbar(menu: goBack, newChat: { Task { await threads.newThread() } }) { titleButton }
        }
    }

    private var agentFooter: some View {
        HStack(spacing: Design.padding) {
            Button(action: goBack) {
                Label("Back to parent", systemImage: "chevron.left")
                    .frame(minHeight: Design.control)
                    .contentShape(.capsule)
            }
            .accessibilityIdentifier("agent.parent")
            Spacer(minLength: 0)
            Button {
                guard let parent = conversation.parent, let agent = conversation.agent else { return }
                Task {
                    do { try await parent.controlAgent(agent) }
                    catch { actionError = error.localizedDescription }
                }
            } label: {
                Label("Stop", systemImage: "stop.fill")
                    .frame(minWidth: Design.control, minHeight: Design.control)
                    .contentShape(.capsule)
            }
            .foregroundStyle(Design.failure)
            .disabled(conversation.agent?.isWorking != true)
            .accessibilityIdentifier("agent.stop")
        }
        .font(.subheadline.weight(.semibold))
        .buttonStyle(.plain)
        .padding(.horizontal, Design.padding)
        .glassEffect(.regular, in: Capsule())
        .padding(.horizontal, Design.padding)
        .padding(.bottom, Design.gap)
    }

    private var titleButton: some View {
        Button { showingInfo = true } label: {
            Text(threads.title(for: conversation.thread.id)
                 ?? conversation.thread.lensTitle(firstRequest: conversation.firstRequest))
                .font(Design.headline)
                .lineLimit(1)
                .frame(maxWidth: .infinity, minHeight: Design.control)
                .contentShape(.capsule)
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("conversation.connection")
        .accessibilityValue(connectionStatus)
        .accessibilityHint("Opens thread info")
    }

    private var threadInfo: some View {
        ThreadInfoView(conversation: conversation, title: threads.title(for: conversation.thread.id) ?? conversation.title) { action in
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

    /// Replies followed by the next request, or ending a finished conversation: the last word of each turn.
    static func turnEnds(_ rows: [ChatRow], active: Bool) -> Set<String> {
        var ends: Set<String> = []
        for (index, row) in rows.enumerated() {
            guard case .message(let entry) = row, case .assistant = entry else { continue }
            let next = rows.indices.contains(index + 1) ? rows[index + 1] : nil
            if next == nil { if !active { ends.insert(row.id) } }
            else if case .message(let following)? = next, case .user = following { ends.insert(row.id) }
        }
        return ends
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
