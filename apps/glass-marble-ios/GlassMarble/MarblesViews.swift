import SwiftUI

/// One marble-to-marble ask as a quiet system row: who asked whom, then the
/// instruction or reply. Tapping a row with more to say opens
/// the whole reply in place.
struct HandoffLine: View {
    var handoff: MarbleHandoff
    @State private var expanded = false

    private var detail: String { MarbleHandoffLine.detail(handoff) }
    private var expandable: Bool { !detail.isEmpty }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button {
                guard expandable else { return }
                withAnimation(.snappy) { expanded.toggle() }
            } label: {
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    Image(systemName: symbol)
                        .font(.caption)
                        .foregroundStyle(tone)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(MarbleHandoffLine.title(handoff))
                            .font(.footnote.weight(.semibold))
                            .foregroundStyle(.secondary)
                        if !expanded && !detail.isEmpty {
                            Text(detail)
                                .font(.footnote)
                                .foregroundStyle(.secondary)
                        }
                    }
                    Spacer(minLength: 4)
                    if expandable {
                        Image(systemName: "chevron.down")
                            .font(.caption2.weight(.semibold))
                            .foregroundStyle(.tertiary)
                            .rotationEffect(.degrees(expanded ? 180 : 0))
                    }
                }
                .fixedSize(horizontal: false, vertical: true)
                .frame(minHeight: 44)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel(MarbleHandoffLine.render(handoff))
            .accessibilityHint(expandable ? (expanded ? "Hides the details" : "Shows the details") : "")
            if expanded {
                MarbleMarkdownView(text: detail, font: .footnote, color: .secondary)
                    .textSelection(.enabled)
                    .padding(.leading, 22)
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(
            MarbleTheme.card,
            in: MarbleTheme.panel
        )
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var symbol: String {
        switch handoff.status {
        case "asked": "arrow.turn.down.right"
        case "finished": "checkmark.circle"
        default: "exclamationmark.circle"
        }
    }

    private var tone: Color {
        switch handoff.status {
        case "asked", "finished": .secondary
        default: .orange
        }
    }
}

/// The shared chat: one timeline where the person talks to every marble at
/// once. The speaker label identifies whose reply you’re reading.
struct EveryoneHistoryView: View {
    @EnvironmentObject private var store: MarbleStore
    @EnvironmentObject private var navigation: MarbleNavigationModel
    @State private var draft = ""
    private let target = MarbleSendTarget.everyone()

    var body: some View {
        let presentation = store.outgoingPresentation(store.everyoneEntries, to: target, conversationId: everyoneConversationId)
        let entries = store.withLiveToolRows(presentation.entries, conversationId: everyoneConversationId)
        let serverWorking = Set(store.everyoneThinking.map(\.id))
        let reservedAgents = Set(entries.lazy.filter { $0.kind == "pending" }.map(\.speaker))
        let responders = (presentation.responders + store.prism.marbles.map(\.id).filter {
            serverWorking.contains($0) && !presentation.responders.contains($0)
        }).filter { !reservedAgents.contains($0) }
        let turns = store.everyoneTurns
        ScrollView {
            // Lay out the loaded page together so the newest edge has
            // its final height before the first frame of the push.
            VStack(alignment: .leading, spacing: 16) {
                if entries.isEmpty && store.everyoneHistoryLoading {
                    ProgressView().frame(maxWidth: .infinity).padding(.top, 60)
                        .accessibilityLabel("Loading messages")
                } else if entries.isEmpty && store.historyErrors.contains(everyoneConversationId) {
                    MarbleHistoryUnavailable { await store.openEveryoneHistory() }
                } else if entries.isEmpty {
                    MarbleEmptyState("No one has spoken yet",
                                     description: Text("Say something, or use @ to call on an agent."))
                }
                let transcript = MarbleReasoningAssociation(entries)
                ForEach(transcript.standalone, id: \.rowID) { entry in
                    MarbleEntryView(
                        entry: entry,
                        speaker: store.speakerStyle(for: entry),
                        reasoning: transcript.attached[entry.id],
                        toolRows: transcript.tools[entry.id] ?? [],
                        timestamp: transcript.timestamps[entry.id],
                        usesThreadActionsDialog: true
                    )
                }
                ForEach(turns.filter { $0.isRunning || $0.id == turns.last?.id }) { task in
                    let reasoning = store.reasoning(for: task.id)
                    if store.isTaskWorking(task) && !reasoning.isEmpty
                        && !entries.contains(where: {
                            $0.taskId == task.id && ["reasoning", "pending", "assistant", "failure"].contains($0.kind)
                        }) {
                        MarbleReasoningRow(text: reasoning, speaker: store.agentName(for: task.id))
                    }
                    MarbleTurnTail(task: task, entries: entries, showsLiveTools: true)
                }
                ForEach(responders, id: \.self) { id in
                    let agent = store.prism.marbles.first { $0.id == id }
                    MarbleThinkingBubble(name: agent?.name ?? "Marble", speaker: agent.map(store.rosterStyle(for:)))
                }
                MarbleTranscriptEnd(bottomPadding: 12)
            }
            .marbleHistoryScrollTargets()
            .padding(.horizontal, 16)
            .padding(.top, 12)
            .frame(maxWidth: 820, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .scrollDismissesKeyboard(.interactively)
        .marbleFollowsLatest(
            hasMessages: !entries.isEmpty, outgoing: store.lastOutgoing(to: target), startsAtLatest: true
        ) { beforePrepend in
            await store.loadOlderHistory(everyoneConversationId, beforePrepend: beforePrepend)
        }
        .marbleComposerInset { composer }
        .task { await store.openEveryoneHistory() }
        .background(MarbleTheme.paper)
        .navigationTitle("Everyone")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(MarbleTheme.paper, for: .navigationBar)
        .toolbarBackgroundVisibility(.visible, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    draft = MarbleMentions.askingEveryone(in: draft)
                } label: {
                    Image(systemName: "person.3").frame(minWidth: 44, minHeight: 44)
                }
                .accessibilityLabel("Ask everyone")
            }
            ToolbarItem(placement: .topBarTrailing) {
                NavigationLink(value: ChatRoute.find(everyoneConversationId)) {
                    Label("Find", systemImage: "magnifyingglass")
                }
            }
        }
    }

    /// The composer with @Name completion from the roster.
    private var composer: some View {
        let members = store.prism.marbles.filter(\.isEveryoneMember)
        return VStack(spacing: 0) {
            if !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                let ids = MarbleMentions.responders(in: draft, roster: members)
                let names = ids.compactMap { id in members.first { $0.id == id }?.name }
                if !names.isEmpty {
                    Text("To: \(names.formatted(.list(type: .and)))")
                        .font(.caption)
                        .foregroundStyle(MarbleTheme.muted)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 16)
                        .padding(.top, 8)
                }
            }
            let suggestions = MarbleMentions.completions(
                in: draft,
                names: members.map(\.name)
            )
            if !suggestions.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        ForEach(suggestions, id: \.self) { name in
                            Button {
                                draft = MarbleMentions.applying(name, to: draft)
                            } label: {
                                Text("@\(name)")
                                    .font(.callout)
                                    .padding(.horizontal, 10)
                                    .padding(.vertical, 6)
                                    .background(
                                        MarbleTheme.card,
                                        in: Capsule()
                                    )
                            }
                            .buttonStyle(.plain)
                            .foregroundStyle(MarbleTheme.tint)
                        }
                    }
                    .padding(.horizontal, 12)
                    .padding(.top, 8)
                }
            }
            ComposerBar(
                draft: $draft,
                isWorking: false,
                placeholder: "Message Everyone",
                onSend: { store.send($0, to: target, entries: store.everyoneEntries) },
                onStop: {}
            )
        }
    }

}

/// The roster: every marble with its look, one line of personality, and what
/// it's doing — plus the built-in Marble whose chat is the chats list.
struct MarblesView: View {
    @EnvironmentObject private var store: MarbleStore
    @EnvironmentObject private var navigation: MarbleNavigationModel
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var showsNew = false
    @State private var showsImport = false
    var selection: ChatRoute?
    var onSelect: ((ChatRoute) -> Void)?

    var body: some View {
        List {
            Section {
                Button {
                    navigation.setEveryonePath([])
                    navigation.selectTab(.everyone)
                } label: {
                    Label("Everyone", systemImage: "bubble.left.and.bubble.right")
                        .frame(minHeight: 44)
                }
                .listRowBackground(MarbleTheme.card)
            }
            ForEach(store.prism.marbles) { marble in
                let route = marble.isDefault ? ChatRoute.marbleChats : .marbleChat(marble.id)
                Group {
                    if let onSelect {
                        Button { onSelect(route) } label: { person(marble) }
                            .buttonStyle(.plain)
                            .accessibilityAddTraits(selection == route ? .isSelected : [])
                    } else {
                        NavigationLink(value: route) { person(marble) }
                    }
                }
                .listRowBackground(selection == route ? MarbleTheme.wash : MarbleTheme.card)
                .contextMenu {
                    NavigationLink(value: ChatRoute.marbleInfo(marble.id)) {
                        Label("Agent card", systemImage: "person.crop.rectangle")
                    }
                }
            }
            if !store.loaded && store.prism.marbles.isEmpty {
                MarbleListSkeleton()
            }
            Section {
                Button { showsNew = true } label: {
                    Label("New agent", systemImage: "plus.circle")
                        .font(.body)
                        .frame(minHeight: 44, alignment: .leading)
                }
                .listRowBackground(MarbleTheme.card)
                Button { showsImport = true } label: {
                    Label("Import agent", systemImage: "square.and.arrow.down")
                        .frame(minHeight: 44, alignment: .leading)
                }
                .listRowBackground(MarbleTheme.card)
            }
        }
        .navigationTitle("Agents")
        .toolbar { ToolbarItem(placement: .topBarTrailing) { MarbleNavConnection() } }
        .animation(reduceMotion ? nil : .default, value: store.prism.marbles)
        .contentMargins(.bottom, sizeClass == .compact ? 32 : 0, for: .scrollContent)
        .scrollContentBackground(.hidden)
        .background(MarbleTheme.paper)
        .sheet(isPresented: $showsNew) {
            NavigationStack { NewMarbleView() }
        }
        .sheet(isPresented: $showsImport) {
            NavigationStack { NewMarbleView(importing: true) }
        }
    }

    private func person(_ marble: MarblePrismMarble) -> some View {
        let style = store.rosterStyle(for: marble)
        let working = store.isAgentWorking(marble)
        return HStack(spacing: 12) {
            MarbleAvatar(glyph: style.glyph, hue: style.hue ?? marble.hue, size: 44, isWorking: working)
            VStack(alignment: .leading, spacing: 4) {
                Text(marble.name).font(.subheadline.weight(.semibold)).foregroundStyle(MarbleTheme.ink)
                if !marble.role.isEmpty {
                    Text(marble.displayRole)
                        .font(.footnote).foregroundStyle(MarbleTheme.muted)
                }
                if working {
                    Label("Working", systemImage: "circle.fill")
                        .font(.footnote)
                        .foregroundStyle(MarbleTheme.waiting)
                }
            }
            .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
        }
        .padding(.vertical, 6)
        .contentShape(Rectangle())
    }
}

struct MarbleNavConnection: View {
    @EnvironmentObject private var store: MarbleStore

    var body: some View {
        Image(systemName: store.connection == .live ? "circle.fill" : "circle.dotted")
            .font(.footnote)
            .foregroundStyle(store.connection == .live ? .green : .secondary)
            .accessibilityLabel(store.statusLabel)
    }
}

struct MarbleListSkeleton: View {
    var body: some View {
        ForEach(0..<3) { _ in
            HStack(spacing: 14) {
                Circle().fill(.quaternary).frame(width: 44, height: 44)
                VStack(alignment: .leading, spacing: 5) {
                    Text("Conversation").font(.subheadline.weight(.semibold))
                    Text("Your latest messages").font(.footnote)
                }
            }
            .redacted(reason: .placeholder)
            .accessibilityHidden(true)
        }
    }
}

/// The new-marble sheet: name, personality with starters to tap, and a look.
struct NewMarbleView: View {
    var importing = false
    var duplicate: MarblePrismMarble?
    @EnvironmentObject private var store: MarbleStore
    @EnvironmentObject private var navigation: MarbleNavigationModel
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var role = ""
    @State private var persona = ""
    @State private var card = MarbleAgentCard()
    @State private var importText = ""
    @State private var imported = false
    @State private var glyph = MarbleMentions.glyphs[0]
    @State private var hue = 42
    @State private var creating = false
    @State private var creationError: String?
    @State private var choseAvatar = false

    var body: some View {
        Form {
            if importing && !imported {
                Section("Agent card JSON") {
                    TextEditor(text: $importText)
                        .font(.body.monospaced())
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)
                        .accessibilityLabel("Agent card JSON")
                        .frame(minHeight: 160)
                    Button("Load card") { loadCard() }
                        .frame(minHeight: 44)
                }
            } else {
                Section("Name") {
                    TextField("Name your agent", text: $name)
                    .textInputAutocapitalization(.words)
                        .frame(minHeight: 44)
                    if duplicate == nil {
                        TextField("Role", text: $role).frame(minHeight: 44)
                    }
                }
                if duplicate == nil {
                    Section("Avatar") { MarbleLookPicker(glyph: $glyph, hue: $hue, choices: store.avatarChoices()) }
                    MarbleAgentCardFields(card: $card, persona: $persona)
                }
            }
            if let creationError {
                Text(creationError).foregroundStyle(MarbleTheme.problem)
            }
        }
        .scrollContentBackground(.hidden)
        .background(MarbleTheme.paper)
        .navigationTitle(duplicate != nil ? "Duplicate agent" : importing ? "Import agent" : "New agent")
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel") { dismiss() }
            }
            ToolbarItem(placement: .confirmationAction) {
                Button(creating ? "Creating…" : "Create") { create() }
                    .disabled(creating || (importing && !imported) || name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .onAppear {
            if let duplicate, name.isEmpty {
                name = "\(duplicate.name) copy"
            }
            if !choseAvatar {
                let choices = store.avatarChoices()
                glyph = choices.glyphs.first ?? glyph
                hue = choices.hues.first ?? hue
                choseAvatar = true
            }
        }
    }

    private func loadCard() {
        do {
            let draft = try JSONDecoder().decode(MarbleClient.MarbleDraft.self, from: Data(importText.utf8))
            name = draft.name
            role = draft.role
            persona = draft.persona
            (glyph, hue) = store.avatarChoices().available(glyph: draft.glyph, hue: draft.hue)
            card = draft.card ?? .init()
            creationError = nil
            imported = true
        } catch {
            creationError = "This isn’t an agent card. Check the JSON and try again."
        }
    }

    private func create() {
        let marbleName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        creating = true
        creationError = nil
        Task {
            let id = await store.addMarble(
                name: marbleName,
                role: role,
                glyph: glyph,
                hue: hue,
                persona: persona.trimmingCharacters(in: .whitespacesAndNewlines),
                card: card,
                duplicateId: duplicate?.id
            )
            creating = false
            if let id {
                dismiss()
                navigation.openMarble(id)
            } else {
                creationError = store.errorMessage ?? "Couldn’t create this agent. Try again."
            }
        }
    }
}

private struct MarbleAgentCardFields: View {
    @Binding var card: MarbleAgentCard
    @Binding var persona: String

    private let toolChoices = [
        ("List workspace", "glass.workspace.list"),
        ("Read files", "glass.workspace.read"),
        ("Search files", "glass.workspace.search"),
        ("Write files", "glass.workspace.write"),
        ("Ask another agent", "glass.marble.ask"),
        ("List repository crates", "glass.crates.list"),
    ]

    var body: some View {
        Section("System prompt") {
            TextField("Instructions", text: $persona, axis: .vertical)
                .lineLimit(3...8)
                .frame(minHeight: 44)
        }
        Section("Turn settings") {
            TextField("Voice", text: $card.voice).frame(minHeight: 44)
            TextField("Model (host default if empty)", text: $card.model)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .frame(minHeight: 44)
            Picker("When to speak", selection: $card.whenToSpeak) {
                ForEach(MarbleAgentCard.Speaking.allCases, id: \.self) { choice in
                    Text(choice.label).tag(choice)
                }
            }
            .frame(minHeight: 44)
            LabeledContent("Tool call limit") {
                TextField("Default", value: $card.toolCallLimit, format: .number)
                    .accessibilityLabel("Tool call limit")
                    .multilineTextAlignment(.trailing)
                    .keyboardType(.numberPad)
            }
            .frame(minHeight: 44)
            LabeledContent("Time limit (seconds)") {
                TextField("Default", value: $card.turnTimeoutSecs, format: .number)
                    .accessibilityLabel("Time limit in seconds")
                    .multilineTextAlignment(.trailing)
                    .keyboardType(.numberPad)
            }
            .frame(minHeight: 44)
        }
        Section {
            Toggle("Native tools (all as a group)", isOn: $card.tools.native)
                .frame(minHeight: 44)
            ForEach(toolChoices, id: \.1) { label, tool in
                Toggle(label, isOn: Binding(
                    get: { card.tools.allowed.contains(tool) },
                    set: { allowed in
                        if allowed {
                            card.tools.allowed.append(tool)
                        } else {
                            card.tools.allowed.removeAll { $0 == tool }
                        }
                    }
                ))
                .frame(minHeight: 44)
            }
        } header: {
            Text("Allowed tools")
        } footer: {
            Text("Marble tools are enforced individually. Native tools can only be enabled or disabled together.")
        }
    }
}

private struct MarbleLookPicker: View {
    @Binding var glyph: String
    @Binding var hue: Int
    let choices: MarbleAvatarChoices

    var body: some View {
        let namedShapes = choices.glyphs.filter { !$0.hasPrefix("roster-") }
        let offeredShapes = namedShapes.isEmpty ? Array(choices.glyphs.prefix(6)) : namedShapes
        MarbleAvatar(glyph: glyph, hue: hue, size: 58)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 8)
        Picker("Shape", selection: $glyph) {
            ForEach(offeredShapes.contains(glyph) ? offeredShapes : [glyph] + offeredShapes, id: \.self) { shape in
                let label = shape.hasPrefix("roster-") ? "Rosette \(shape.dropFirst(7))" : shape.capitalized
                Text(label).tag(shape)
            }
        }
        Picker("Hue", selection: $hue) {
            ForEach(choices.hues.contains(hue) ? choices.hues : [hue] + choices.hues, id: \.self) { choice in
                Text(MarbleTheme.hues.first { $0.degrees == choice }?.name ?? "Hue \(choice)°").tag(choice)
            }
        }
        .sensoryFeedback(.selection, trigger: hue)
        .sensoryFeedback(.selection, trigger: glyph)
    }
}

/// One marble's own chat — its durable conversation, which the first message
/// creates on the host — with its info a tap away.
struct MarbleChatScreen: View {
    var marbleId: String
    @EnvironmentObject private var store: MarbleStore
    @EnvironmentObject private var navigation: MarbleNavigationModel
    @State private var draft = ""
    @State private var creatingChat = false

    private var marble: MarblePrismMarble? {
        store.prism.marbles.first { $0.id == marbleId }
    }

    var body: some View {
        Group {
            if let marble, store.state.conversation(marble.conversationId)?.archived == true {
                MarbleEmptyState(title: Text("Chat archived"), systemImage: "archivebox") {
                    MarbleInlineButton(title: "Restore") {
                        Task { await store.archiveConversation(marble.conversationId, archived: false) }
                    }
                }
            } else if let marble {
                transcript(marble)
            } else {
                MarbleEmptyState(
                    "That agent is gone",
                    description: Text("It was removed from your agents."),
                    systemImage: "person.crop.circle.badge.questionmark"
                )
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbarVisibility(.hidden, for: .tabBar)
        .marbleChatBar(edge: .top) {
            MarbleTitleView(title: marble?.name ?? "Marble",
                            avatar: marble.map(store.rosterStyle(for:)),
                            avatarWorking: marble.map(store.isAgentWorking) ?? false)
                .marbleChatHeader()
        }
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    Button("New chat", systemImage: "square.and.pencil") {
                        creatingChat = true
                        Task {
                            defer { creatingChat = false }
                            do {
                                let chat = try await store.createConversation(marbleId: marbleId)
                                navigation.openNewAgentChat(chat, agentId: marbleId)
                            } catch {
                                store.errorMessage = error.localizedDescription
                            }
                        }
                    }
                    ForEach(agentChats) { chat in
                        NavigationLink(chat.displayTitle, value: ChatRoute.conversation(chat.id))
                    }
                } label: {
                    Label("Chats", systemImage: "bubble.left.and.bubble.right")
                }
                .disabled(creatingChat)
            }
            ToolbarItem(placement: .topBarTrailing) {
                NavigationLink(value: ChatRoute.marbleInfo(marbleId)) {
                    Image(systemName: "person.crop.circle")
                }
                .accessibilityLabel("About this agent")
            }
            if let conversationId = marble?.conversationId, !conversationId.isEmpty {
                ToolbarItem(placement: .topBarTrailing) {
                    MarbleChatTools(conversationId: conversationId)
                }
            }
        }
    }

    private var agentChats: [MarbleConversation] {
        store.state.conversations.filter { $0.marbleId == marbleId && $0.archived != true }
            .sorted { $0.updatedAt > $1.updatedAt }
    }

    @ViewBuilder
    private func transcript(_ marble: MarblePrismMarble) -> some View {
        let style = store.rosterStyle(for: marble)
        let target = MarbleSendTarget.marble(marble.id)
        let durable = store.marbleTranscript(marble)
        let presentation = store.outgoingPresentation(durable, to: target, conversationId: marble.conversationId)
        let entries = store.withLiveToolRows(presentation.entries, conversationId: marble.conversationId)
        let working = store.isAgentWorking(marble)
        let active = store.latestTurn(in: marble.conversationId)
        let thinking = !presentation.responders.isEmpty || working
            || store.isAwaitingReply(in: marble.conversationId, entries: entries)
        ScrollView {
                LazyVStack(alignment: .leading, spacing: 16) {
                    if entries.isEmpty && store.marbleHistoryLoading(marble) {
                        ProgressView().frame(maxWidth: .infinity).padding(.top, 60)
                            .accessibilityLabel("Loading messages")
                    } else if entries.isEmpty && store.historyErrors.contains(marble.conversationId) {
                        MarbleHistoryUnavailable { await store.openMarbleHistory(marble) }
                    } else if entries.isEmpty && !thinking {
                        MarbleEmptyState("Nothing yet",
                                         description: Text("Say hello — \(marble.name) answers in its own voice."))
                    }
                    let transcript = MarbleReasoningAssociation(entries)
                    ForEach(transcript.standalone, id: \.rowID) { entry in
                        MarbleEntryView(entry: entry, speaker: entry.kind == "assistant" || entry.kind == "pending"
                                        ? style : nil, replyHue: style.hue ?? marble.hue, reasoning: transcript.attached[entry.id],
                                        toolRows: transcript.tools[entry.id] ?? [], timestamp: transcript.timestamps[entry.id])
                    }
                    let reasoning = active.map { store.reasoning(for: $0.id) } ?? ""
                    if working && !reasoning.isEmpty
                        && !entries.contains(where: {
                            $0.taskId == active?.id && ["reasoning", "assistant"].contains($0.kind)
                        }) {
                        MarbleReasoningRow(text: reasoning, speaker: marble.name)
                    }
                    MarbleTurnTail(task: active, entries: entries, showsLiveTools: true)
                    if thinking && !entries.contains(where: {
                        ["pending", "assistant"].contains($0.kind) && $0.taskId == active?.id
                    }) {
                        MarbleThinkingBubble(name: marble.name, status: active.flatMap { store.workingLines[$0.id] },
                                             speaker: style)
                    }
                    MarbleTranscriptEnd(bottomPadding: 12)
                }
                .marbleHistoryScrollTargets()
                .padding(.horizontal, 16)
                .padding(.top, 12)
                .frame(maxWidth: 820, alignment: .leading)
                .frame(maxWidth: .infinity)
        }
        .background(MarbleTheme.paper)
        .scrollDismissesKeyboard(.interactively)
        .marbleFollowsLatest(hasMessages: !entries.isEmpty, outgoing: store.lastOutgoing(to: target)) { beforePrepend in
            await store.loadOlderHistory(marble.conversationId, beforePrepend: beforePrepend)
        }
        .task(id: marble.conversationId) { await store.openMarbleHistory(marble) }
        .onDisappear { store.closeMarbleHistory(marble.conversationId) }
        .marbleComposerInset {
            ComposerBar(
                draft: $draft,
                isWorking: working,
                placeholder: "Message \(marble.name)",
                workingPlaceholder: "Correct \(marble.name)’s reply",
                steerReady: active?.isRunning == true,
                onSend: { store.send($0, to: target, entries: durable) },
                onSteer: {
                    await store.steer($0, conversationId: marble.conversationId, taskId: active?.id)
                },
                onStop: {
                    Task {
                        if let active, store.isTaskWorking(active) {
                            await store.cancel(active)
                        } else {
                            await store.stop(marble)
                        }
                    }
                }
            )
        }
    }

}

/// A marble's info: who it is (editable), its look, its memory, its workspace,
/// what it can do, and a way to say goodbye.
struct MarbleInfoView: View {
    var marbleId: String
    @EnvironmentObject private var store: MarbleStore
    @Environment(\.dismiss) private var dismiss
    @State private var persona = ""
    @State private var personaLoaded = false
    @State private var name = ""
    @State private var role = ""
    @State private var card = MarbleAgentCard()
    @State private var glyph = "circle"
    @State private var hue = 0
    @State private var confirmRemove = false
    @State private var duplicate: MarblePrismMarble?

    private var marble: MarblePrismMarble? {
        store.prism.marbles.first { $0.id == marbleId }
    }

    var body: some View {
        Form {
            if let marble {
                Section {
                    VStack(spacing: 8) {
                        MarbleAvatar(glyph: glyph, hue: hue, size: 64)
                        Text(marble.name).font(.body.weight(.semibold))
                        if !marble.role.isEmpty {
                            Text(marble.displayRole).font(.subheadline).foregroundStyle(MarbleTheme.muted)
                        }
                    }
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity)
                    .listRowBackground(Color.clear)
                }

                Section {
                    NavigationLink {
                        MemoryView(marbleId: marble.id)
                    } label: {
                        Label("Memory", systemImage: "bookmark")
                    }
                }
                .listRowBackground(MarbleTheme.card)

                if !marble.isDefault {
                    Section("Identity") {
                        TextField("First name", text: $name).frame(minHeight: 44)
                        TextField("Role", text: $role).frame(minHeight: 44)
                    }
                    if MarbleAvatarChoices.builtIns[marble.id] == nil {
                        Section("Avatar") {
                            MarbleLookPicker(glyph: $glyph, hue: $hue, choices: store.avatarChoices(excluding: marble.id))
                        }
                    }
                    Section("Everyone") {
                        Toggle("Member of Everyone", isOn: $card.inEveryone).frame(minHeight: 44)
                    }
                }
                MarbleAgentCardFields(card: $card, persona: $persona)
                Section {
                    Button("Save card") { saveCard(marble) }.frame(minHeight: 44)
                    Button("Duplicate agent") { duplicate = marble }.frame(minHeight: 44)
                    if let error = store.errorMessage {
                        Text(error).foregroundStyle(MarbleTheme.problem)
                    }
                }
                Section {
                    DisclosureGroup("Workspace and abilities") {
                        MarbleValueRow("Workspace", value: marble.workspace.isEmpty ? "Host workspace" : marble.workspace)
                        let model = marble.card?.model.isEmpty == false ? marble.card?.model : store.model
                        MarbleValueRow("Model", value: model.flatMap { $0.isEmpty ? nil : $0 } ?? "Host default")
                        NavigationLink("Tools") { WhatMarbleCanUseView() }
                    }
                }
                if !marble.isDefault {
                    Section {
                        Button("Remove \(marble.name)", role: .destructive) {
                            confirmRemove = true
                        }
                    }
                }
            }
        }
        .scrollContentBackground(.hidden)
        .background(MarbleTheme.paper)
        .navigationTitle("Profile")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear {
            guard !personaLoaded, let marble else { return }
            personaLoaded = true
            persona = marble.persona
            name = marble.name
            role = marble.role
            card = marble.card ?? .init()
            let style = store.rosterStyle(for: marble)
            glyph = style.glyph
            hue = style.hue ?? marble.hue
        }
        .sheet(item: $duplicate) { agent in
            NavigationStack { NewMarbleView(duplicate: agent) }
        }
        .confirmationDialog(
            "Remove \(marble?.name ?? "this agent") and their chats?",
            isPresented: $confirmRemove,
            titleVisibility: .visible
        ) {
            Button("Remove", role: .destructive) {
                if let marble {
                    Task {
                        await store.removeMarble(marble)
                        dismiss()
                    }
                }
            }
        } message: {
            Text("\(marble?.name ?? "It") and its chat are removed, along with the lines it left in Everyone.")
        }
    }

    private func saveCard(_ marble: MarblePrismMarble) {
        Task {
            await store.updateMarble(marble.id, update: .init(
                name: marble.isDefault ? nil : name,
                role: marble.isDefault ? nil : role,
                glyph: MarbleAvatarChoices.builtIns[marble.id] == nil ? glyph : nil,
                hue: MarbleAvatarChoices.builtIns[marble.id] == nil ? hue : nil,
                persona: persona,
                card: card
            ))
        }
    }
}
