import GlassModels
import SwiftUI

struct ConversationView: View {
    @Bindable var conversation: ConversationModel
    var threads: ThreadsModel
    var goBack: () -> Void
    @Environment(\.scenePhase) private var scenePhase
    /// The chat moves with a streaming reply until you touch it, and picks it up again after you've
    /// left it alone for a few seconds or tap Latest.
    @State private var following = true
    @State private var atBottom = true
    @State private var touching = false
    @State private var lastTouch = ContinuousClock.now
    @State private var resumeFollowing: Task<Void, Never>?
    @State private var inspectingReasoning: String?
    @State private var reasoningTopToReveal: CGFloat?
    @State private var expandedReasoning: Set<String> = []
    @State private var firstReasoningBounds: HistoryBounds?
    @State private var restoreHistoryBounds: HistoryBounds?
    @State private var historyScroll = ScrollPosition()
    @State private var historyTopInset: CGFloat = 0
    @State private var interactionHeight: CGFloat = 0
    @State private var picker: ComposerModel.Picker?
    @State private var showingChecklist = false
    @State private var showingDetails = false
    @State private var savedRecord: SavedRecord?
    @State private var actionError: String?
    @State private var pendingEdit: TimelineEntry?
    @State private var previewPage: PreviewPage?
    @State private var keyboardShown = false
    @State private var dragging = false
    @State private var dragStartOffset: CGFloat?
    @AppStorage(ToolDensity.key) private var density: ToolDensity = .compact
    @Environment(AppModel.self) private var app

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 16) {
                    if conversation.hasEarlier {
                        Button {
                            Task {
                                let first = conversation.timeline.entries.first
                                let previous = firstReasoningBounds.flatMap { bounds in
                                    bounds.entry == first && expandedReasoning.contains(bounds.entry.id) ? bounds : nil
                                }
                                restoreHistoryBounds = previous
                                await conversation.loadOlder()
                                guard let first else { return }
                                let extended = previous != nil
                                    && conversation.timeline.entries.contains { $0.id == first.id && $0 != first }
                                if !extended {
                                    restoreHistoryBounds = nil
                                    proxy.scrollTo(first.id, anchor: .top)
                                } else if restoreHistoryBounds != nil {
                                    if let bounds = firstReasoningBounds, bounds.entry.id == first.id, bounds.entry != first {
                                        restoreReasoningPosition(bounds)
                                    } else {
                                        proxy.scrollTo(first.id, anchor: .top)
                                    }
                                }
                            }
                        } label: {
                            if conversation.loadingOlder { ProgressView() } else { Label("Earlier messages", systemImage: "arrow.up.circle") }
                        }
                        .glassButtonStyle()
                        .frame(maxWidth: .infinity)
                        .accessibilityIdentifier("conversation.older")
                    }
                    if conversation.loadedHistory, conversation.timeline.isEmpty {
                        emptyState
                    }
                    ForEach(conversation.timeline.entries) { entry in
                        TimelineRow(entry: entry, liveReasoning: entry.id == conversation.timeline.liveReasoningID) { expanded, top in
                            if expanded { expandedReasoning.insert(entry.id) } else { expandedReasoning.remove(entry.id) }
                            if expanded {
                                resumeFollowing?.cancel()
                                inspectingReasoning = entry.id
                                following = false
                                reasoningTopToReveal = top
                                historyScroll.scrollTo(y: top)
                            } else if inspectingReasoning == entry.id {
                                inspectingReasoning = nil
                                reasoningTopToReveal = nil
                                lastTouch = .now
                                if atBottom { following = true } else { scheduleResume(proxy) }
                            }
                        }
                        .background {
                            if case .reasoning = entry, entry.id == conversation.timeline.entries.first?.id
                                || entry.id == restoreHistoryBounds?.entry.id {
                                Color.clear.onGeometryChange(for: HistoryBounds.self) { geometry in
                                    HistoryBounds(entry: entry, viewportBottom: geometry.frame(in: .scrollView(axis: .vertical)).maxY,
                                                  contentBottom: geometry.frame(in: .named(Self.historyContent)).maxY)
                                } action: { bounds in
                                    firstReasoningBounds = bounds
                                    restoreReasoningPosition(bounds)
                                }
                            }
                        }
                        .id(entry.id)
                    }
                    if let working = workingLabel {
                        WorkingBubble(label: working).id("timeline.working")
                    }
                    Color.clear.frame(height: 1).id(Self.bottom)
                    // A lazy stack's content size is only an estimate, so "at the bottom" means this
                    // strip covering the last 80 points is on screen.
                    Color.clear.frame(height: 80).padding(.top, -96)
                        .onScrollVisibilityChange(threshold: 0.01) { reachedBottom($0) }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
                .frame(maxWidth: 760)
                .frame(maxWidth: .infinity)
                .coordinateSpace(name: Self.historyContent)
            }
            .scrollPosition($historyScroll)
            .onScrollGeometryChange(for: CGFloat.self) { $0.contentInsets.top } action: { _, inset in historyTopInset = inset }
            .onScrollGeometryChange(for: CGFloat.self) { $0.contentSize.height } action: { _, _ in
                guard let top = reasoningTopToReveal else { return }
                reasoningTopToReveal = nil
                historyScroll.scrollTo(y: top)
            }
            .defaultScrollAnchor(.bottom)
            .defaultScrollAnchor(following ? .bottom : .top, for: .sizeChanges)
            .softTopScrollEdge()
            .scrollDismissesKeyboard(.interactively)
            .onScrollPhaseChange { _, phase in
                dragging = phase == .interacting
                if !dragging { dragStartOffset = nil }
                switch phase {
                case .tracking, .interacting:
                    touching = true
                    following = false
                    reasoningTopToReveal = nil
                    resumeFollowing?.cancel()
                case .idle where touching:
                    touching = false
                    lastTouch = .now
                    if atBottom {
                        inspectingReasoning = nil
                        following = true
                    } else {
                        scheduleResume(proxy)
                    }
                default: break
                }
            }
            .onScrollGeometryChange(for: CGFloat.self) { $0.contentOffset.y } action: { _, offset in
                // Pulling the chat down to read back puts the keyboard away, as in Messages.
                guard dragging, keyboardShown else { return }
                let start = dragStartOffset ?? offset
                dragStartOffset = start
                if start - offset > 16 { hideKeyboard() }
            }
            .onChange(of: conversation.timeline.interaction?.id) { _, id in
                guard id != nil else { return }
                inspectingReasoning = nil
                reasoningTopToReveal = nil
                following = true
                Task {
                    try? await Task.sleep(for: .milliseconds(400))
                    proxy.scrollTo(Self.bottom, anchor: .bottom)
                }
            }
            .onChange(of: conversation.timeline.entries) { old, new in
                if following {
                    // Text growing in place stays pinned by the bottom size-change anchor; jumping on
                    // every delta would fight a finger that is just landing on the chat.
                    if new.count != old.count { pinToBottom(proxy) } else if !touching, !atBottom { proxy.scrollTo(Self.bottom, anchor: .bottom) }
                } else if inspectingReasoning == nil, !touching, conversation.timeline.isActive,
                          ContinuousClock.now - lastTouch >= Self.resumeAfter {
                    resume(proxy)
                }
            }
            .overlay(alignment: .bottomTrailing) { floatingControls(proxy) }
            .floatingBar(edge: .top) { header }
            .floatingBar {
                bottomBar(proxy)
                    .onGeometryChange(for: CGFloat.self, of: \.size.height) { _, _ in
                        guard following else { return }
                        proxy.scrollTo(Self.bottom, anchor: .bottom)
                    }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("conversation")
        .accessibilityValue("\(conversation.timeline.entries.count) entries\(conversation.hasEarlier ? ", earlier available" : "")")
        .toolbarVisibility(.hidden, for: .navigationBar)
        .sheet(item: $picker) { ChoicePicker(picker: $0, composer: conversation.composer) }
        .sheet(isPresented: $showingDetails) { DetailsView(conversation: conversation) }
        .sheet(item: $savedRecord) { SavedActivityView(conversation: conversation, record: $0.id) }
        .environment(\.openSavedActivity) { savedRecord = SavedRecord(id: $0) }
        .environment(\.messageAction) { entry, action in Task { await perform(action, on: entry) } }
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
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillShowNotification)) { _ in keyboardShown = true }
        .onReceive(NotificationCenter.default.publisher(for: UIResponder.keyboardWillHideNotification)) { _ in keyboardShown = false }
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
        Text("No messages yet. Messages sent from the web UI or the host appear here as they arrive.")
            .font(.footnote)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.top, 24)
            .accessibilityIdentifier("conversation.empty")
    }

    @ViewBuilder
    private func bottomBar(_ proxy: ScrollViewProxy) -> some View {
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
            ComposerView(conversation: conversation)
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
                Button("Latest", systemImage: "arrow.down") { resume(proxy) }
                    .glassButtonStyle()
                    .accessibilityIdentifier("conversation.latest")
                    .transition(.opacity.combined(with: .scale(scale: 0.9)))
            }
            if keyboardShown {
                Button("Hide Keyboard", systemImage: "keyboard.chevron.compact.down") { hideKeyboard() }
                    .labelStyle(.iconOnly)
                    .glassButtonStyle()
                    .accessibilityIdentifier("conversation.hideKeyboard")
            }
        }
        .padding(.horizontal, 16)
        .padding(.bottom, 8)
        .animation(.easeOut(duration: 0.2), value: following || atBottom)
    }

    private static var resumeAfter: Duration {
        let seconds = UserDefaults.standard.double(forKey: "GlassFollowResumeSeconds")
        return .seconds(seconds > 0 ? seconds : 3)
    }

    static let historyContent = "conversation.historyContent"

    private struct HistoryBounds: Equatable {
        let entry: TimelineEntry
        let viewportBottom: CGFloat
        let contentBottom: CGFloat
    }

    private func restoreReasoningPosition(_ bounds: HistoryBounds) {
        guard let previous = restoreHistoryBounds, previous.entry.id == bounds.entry.id,
              previous.entry != bounds.entry else { return }
        restoreHistoryBounds = nil
        // A page can add text inside this row, so pin its unchanged trailing edge rather than its top.
        historyScroll.scrollTo(y: bounds.contentBottom - previous.viewportBottom + historyTopInset)
    }

    private func scheduleResume(_ proxy: ScrollViewProxy) {
        resumeFollowing?.cancel()
        resumeFollowing = Task {
            try? await Task.sleep(for: Self.resumeAfter)
            guard !Task.isCancelled, inspectingReasoning == nil, !touching, conversation.timeline.isActive else { return }
            resume(proxy)
        }
    }

    /// Lazy rows below the fold only get their real height once laid out, so one jump can land short of
    /// the end. Jump again as they settle, unless you've touched the chat since.
    private func pinToBottom(_ proxy: ScrollViewProxy) {
        guard !touching else { return }
        proxy.scrollTo(Self.bottom, anchor: .bottom)
        Task {
            for delay in [100, 400] {
                try? await Task.sleep(for: .milliseconds(delay))
                guard following, !touching, !atBottom else { return }
                proxy.scrollTo(Self.bottom, anchor: .bottom)
            }
        }
    }

    private func reachedBottom(_ bottom: Bool) {
        atBottom = bottom
        if bottom {
            if inspectingReasoning == nil, !touching { following = true }
        }
    }

    private func resume(_ proxy: ScrollViewProxy) {
        resumeFollowing?.cancel()
        inspectingReasoning = nil
        reasoningTopToReveal = nil
        following = true
        withAnimation(.smooth(duration: 0.35)) { proxy.scrollTo(Self.bottom, anchor: .bottom) }
    }

    private var secretsBlocked: String? {
        guard app.endpoint?.isSecure == true else { return "Secrets are only sent over HTTPS. Use the Tailscale HTTPS link for this host." }
        guard app.secretEntryUnlocked else { return "Secret entry is locked. Pair again with a secret-entry link from the host to enable it." }
        return nil
    }

    private func hideKeyboard() {
        UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
    }

    private var header: some View {
        HStack(spacing: 8) {
            Button(action: goBack) {
                headerIcon("chevron.backward")
            }
            .accessibilityLabel("Back to conversations")
            .accessibilityIdentifier("conversation.back")
            .floatingGlass()

            titleView
                .padding(.horizontal, 12)
                .frame(maxWidth: .infinity, minHeight: 44)
                .floatingGlass()
                .layoutPriority(1)

            HStack(spacing: 0) {
                Button {
                    conversation.composer.showingCommands = true
                } label: {
                    headerIcon("command")
                }
                .accessibilityLabel("Commands")
                .accessibilityIdentifier("conversation.commands")
                headerMenu
            }
            .floatingGlass()
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 16)
        .padding(.vertical, 4)
        .frame(maxWidth: 760)
        .frame(maxWidth: .infinity)
    }

    private func headerIcon(_ icon: String) -> some View {
        Image(systemName: icon)
            .font(.body.weight(.semibold))
            .frame(width: 44, height: 44)
            .contentShape(Rectangle())
    }

    private var headerMenu: some View {
        Menu {
            if let workspace = conversation.thread.workspace, !workspace.isEmpty {
                Section(workspace) {}
            }
            Button("Details", systemImage: "info.circle") { showingDetails = true }
                .accessibilityIdentifier("conversation.details")
            Button("Model…", systemImage: "cpu") { picker = .model }
                .disabled(!conversation.canWrite)
                .accessibilityIdentifier("conversation.model")
            Button("Permissions…", systemImage: "slider.horizontal.3") { picker = .profiles }
                .disabled(!conversation.canWrite)
                .accessibilityIdentifier("conversation.profile")
            if !conversation.composer.checklist.isEmpty {
                Button("Checklist", systemImage: "checklist") { showingChecklist = true }
            }
            if conversation.canWrite {
                Button("Release control", systemImage: "hand.raised") { Task { await conversation.composer.releaseControl() } }
                    .accessibilityIdentifier("conversation.releaseControl")
            }
            Button("Close conversation", systemImage: "xmark.circle") {
                Task { await threads.close(conversation.thread.id) }
            }
            .accessibilityIdentifier("conversation.close")
        } label: {
            headerIcon("ellipsis")
        }
        .accessibilityLabel("More")
        .accessibilityIdentifier("conversation.menu")
    }

    private var titleView: some View {
        let (title, tint): (String, Color) = switch conversation.connection {
        case .live: ("Live", .green)
        case .connecting, .idle: ("Connecting", .secondary)
        case .reconnecting: ("Reconnecting", .orange)
        case .readOnly: ("Read-only", .secondary)
        }
        let subtitle = conversation.connection == .live
            ? conversation.composer.model ?? app.endpoint?.host
            : title
        return VStack(spacing: 1) {
            HStack(spacing: 6) {
                Circle().fill(tint).frame(width: 7, height: 7)
                    .accessibilityHidden(true)
                Text(conversation.title).font(.headline).lineLimit(1)
            }
            if let subtitle, !subtitle.isEmpty {
                Text(subtitle).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel([conversation.title, conversation.connection == .live ? subtitle : nil].compactMap { $0 }.joined(separator: ", "))
        .accessibilityValue("\(title), \(conversation.resets) resets\(conversation.lastReconnect.map { ", reconnect \($0) ms" } ?? "")")
        .accessibilityIdentifier("conversation.connection")
    }

    /// Plain words for what Glass is doing, shown in the chat where the reply will appear.
    private var workingLabel: String? {
        let timeline = conversation.timeline
        guard timeline.isActive, timeline.interaction == nil else { return nil }
        if case .assistant(_, _, true)? = timeline.entries.last { return nil }
        switch timeline.activity {
        case "Reasoning": return "Glass is thinking"
        case "Running command": return "Glass is running a command"
        case let activity? where activity.hasPrefix("Using "): return "Glass is using " + activity.dropFirst(6)
        default: return "Glass is working"
        }
    }
}

/// A small grey bubble with three softly pulsing dots, like someone typing in Messages.
struct WorkingBubble: View {
    var label: String
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        HStack(spacing: 10) {
            TimelineView(.animation(minimumInterval: 0.05, paused: reduceMotion)) { context in
                let phase = context.date.timeIntervalSinceReferenceDate
                HStack(spacing: 5) {
                    ForEach(0..<3) { dot in
                        Circle()
                            .fill(Color(.secondaryLabel))
                            .frame(width: 7, height: 7)
                            .opacity(reduceMotion ? 0.6 : 0.35 + 0.65 * max(0, sin((phase * 2.4 - Double(dot) * 0.5) * .pi)))
                    }
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 12)
            .background(Color(.secondarySystemBackground), in: Capsule())
            Text(label)
                .font(.footnote)
                .foregroundStyle(.secondary)
        }
        .padding(.top, 2)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(label)
        .accessibilityIdentifier("conversation.activity")
    }
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
                    Drafts.shared.write(opened, text)
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
