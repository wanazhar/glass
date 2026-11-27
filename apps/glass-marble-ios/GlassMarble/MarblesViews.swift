import SwiftUI

/// A marble's shape and hue as a small avatar — how people tell them apart.
struct MarbleAvatar: View {
    var glyph: String
    var hue: Int
    var size: CGFloat = 22
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        let shape = MarbleSilhouette(glyph: glyph)
        shape
            .fill(MarbleTheme.hues[MarbleTheme.paletteIndex(for: hue)].fill)
            .overlay {
                if colorScheme == .light {
                    shape.stroke(MarbleTheme.adaptive(0x8F736F, 0x8F736F), lineWidth: 1)
                }
            }
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }
}

/// One marble-to-marble ask as a quiet system row: who asked whom, then the
/// instruction or reply on one line. Tapping a row with more to say opens
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
                        .frame(width: 14)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(MarbleHandoffLine.title(handoff))
                            .font(.footnote.weight(.semibold))
                            .foregroundStyle(.secondary)
                        if !expanded && !detail.isEmpty {
                            Text(detail)
                                .font(.footnote)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
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
            Color(.secondarySystemBackground).opacity(0.7),
            in: RoundedRectangle(cornerRadius: 12, style: .continuous)
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
        let responders = presentation.responders + store.prism.marbles.map(\.id).filter {
            serverWorking.contains($0) && !reservedAgents.contains($0) && !presentation.responders.contains($0)
        }
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
                    ContentUnavailableView {
                        Text("No one has spoken yet")
                    } description: {
                        Text("Say something, or name a Marble with @ to call on it.")
                    }
                }
                let transcript = MarbleReasoningAssociation(entries)
                ForEach(transcript.standalone, id: \.rowID) { entry in
                    MarbleEntryView(
                        entry: entry,
                        speaker: store.speakerStyle(for: entry),
                        reasoning: transcript.attached[entry.id],
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
                    MarbleThinkingBubble(name: store.prism.marbles.first(where: { $0.id == id })?.name ?? "Marble")
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
        .toolbar {
            ToolbarItem(placement: .principal) {
                MarbleTitleView(title: "Everyone")
                    .marbleToolbarTitle()
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
        VStack(spacing: 0) {
            if !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                let ids = MarbleMentions.responders(in: draft, roster: store.prism.marbles)
                let names = ids.compactMap { id in store.prism.marbles.first { $0.id == id }?.name }
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
                names: store.prism.marbles.map(\.name)
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
                                        Color(.secondarySystemBackground),
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
    @Environment(\.horizontalSizeClass) private var sizeClass
    @State private var showsNew = false
    var selection: ChatRoute?
    var onSelect: ((ChatRoute) -> Void)?

    var body: some View {
        List {
            if store.connectionPaused || store.connection == .offline || store.connection == .connecting || store.needsToken {
                MarbleConnectionNotice()
                    .listRowBackground(MarbleTheme.card)
            }
            Section {
                Text("Pick a Marble to talk one to one, or meet in Everyone.")
                    .font(.subheadline)
                    .foregroundStyle(MarbleTheme.muted)
                    .listRowBackground(Color.clear)
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
            }
            Section {
                Button { showsNew = true } label: {
                    Label("New Marble", systemImage: "plus.circle")
                        .font(.headline)
                        .frame(minHeight: 44, alignment: .leading)
                }
                .listRowBackground(MarbleTheme.card)
            }
        }
        .navigationTitle("Who’s on your mind?")
        .contentMargins(.bottom, sizeClass == .compact ? 32 : 0, for: .scrollContent)
        .scrollContentBackground(.hidden)
        .background(MarbleTheme.paper)
        .sheet(isPresented: $showsNew) {
            NavigationStack { NewMarbleView() }
        }
    }

    private func person(_ marble: MarblePrismMarble) -> some View {
        HStack(spacing: 14) {
            MarbleAvatar(glyph: marble.glyph, hue: marble.hue, size: 50)
            VStack(alignment: .leading, spacing: 4) {
                Text(marble.name).font(.headline).foregroundStyle(MarbleTheme.ink)
                if !marble.tagline.isEmpty {
                    Text(marble.tagline).font(.subheadline).foregroundStyle(MarbleTheme.muted)
                }
                if store.isAgentWorking(marble) {
                    Label("Working", systemImage: "circle.fill")
                        .font(.caption)
                        .foregroundStyle(MarbleTheme.waiting)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 6)
        .contentShape(Rectangle())
    }
}

/// The new-marble sheet: name, personality with starters to tap, and a look.
struct NewMarbleView: View {
    @EnvironmentObject private var store: MarbleStore
    @EnvironmentObject private var navigation: MarbleNavigationModel
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var persona = ""
    @State private var glyph = MarbleMentions.glyphs[0]
    @State private var hue = 42
    @State private var creating = false
    @State private var creationError: String?

    private let starters = [
        "Warm and encouraging; celebrates progress and explains things simply.",
        "Careful and methodical; checks work twice and says what it verified.",
        "Direct and quick; short answers, gets to the point."
    ]

    var body: some View {
        Form {
            Section(header: Text("Name")) {
                TextField("Name your Marble", text: $name)
                    .textInputAutocapitalization(.words)
            }
            Section {
                TextField("Who is this Marble?", text: $persona, axis: .vertical)
                    .lineLimit(2...5)
                ForEach(starters, id: \.self) { starter in
                    Button(starter) { persona = starter }
                        .font(.footnote)
                        .foregroundStyle(MarbleTheme.tint)
                }
            } header: {
                Text("Personality")
            } footer: {
                Text("How it talks and what it's good at. It can be changed later.")
            }
            Section("Look") {
                MarbleLookPicker(glyph: $glyph, hue: $hue)
            }
            if let creationError {
                Text(creationError).foregroundStyle(MarbleTheme.problem)
            }
        }
        .scrollContentBackground(.hidden)
        .background(MarbleTheme.paper)
        .navigationTitle("New Marble")
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel") { dismiss() }
            }
            ToolbarItem(placement: .confirmationAction) {
                Button(creating ? "Creating…" : "Create") { create() }
                    .disabled(creating || name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .onAppear {
            let used = Set(store.prism.marbles.map { MarbleTheme.paletteIndex(for: $0.hue) })
            if let index = MarbleTheme.hues.indices.first(where: { !used.contains($0) }) {
                hue = MarbleTheme.hues[index].degrees
            }
        }
    }

    private func create() {
        let marbleName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        creating = true
        creationError = nil
        Task {
            let id = await store.addMarble(
                name: marbleName,
                glyph: glyph,
                hue: hue,
                persona: persona.trimmingCharacters(in: .whitespacesAndNewlines)
            )
            creating = false
            if let id {
                dismiss()
                navigation.openMarble(id)
            } else {
                creationError = store.errorMessage ?? "Couldn’t create this Marble. Try again."
            }
        }
    }
}

private struct MarbleLookPicker: View {
    @Binding var glyph: String
    @Binding var hue: Int

    var body: some View {
        MarbleAvatar(glyph: glyph, hue: hue, size: 58)
            .frame(maxWidth: .infinity)
            .padding(.vertical, 8)
        Picker("Shape", selection: $glyph) {
            ForEach(MarbleMentions.glyphs, id: \.self) { shape in
                Text(shape == "hex" ? "Hexagon" : shape.capitalized).tag(shape)
            }
        }
        Picker("Hue", selection: Binding(
            get: { MarbleTheme.hues[MarbleTheme.paletteIndex(for: hue)].degrees },
            set: { hue = $0 }
        )) {
            ForEach(MarbleTheme.hues.indices, id: \.self) { index in
                let choice = MarbleTheme.hues[index]
                Text(choice.name).tag(choice.degrees)
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

    private var marble: MarblePrismMarble? {
        store.prism.marbles.first { $0.id == marbleId }
    }

    var body: some View {
        Group {
            if let marble {
                transcript(marble)
            } else {
                ContentUnavailableView(
                    "That Marble is gone",
                    systemImage: "person.crop.circle.badge.questionmark",
                    description: Text("It was removed from your Marbles.")
                )
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .toolbarVisibility(.hidden, for: .tabBar)
        .toolbar {
            ToolbarItem(placement: .principal) {
                MarbleTitleView(title: marble?.name ?? "Marble")
                    .marbleToolbarTitle()
            }
            ToolbarItem(placement: .topBarTrailing) {
                NavigationLink(value: ChatRoute.marbleInfo(marbleId)) {
                    if let marble {
                        MarbleAvatar(glyph: marble.glyph, hue: marble.hue, size: 24)
                            .accessibilityHidden(false)
                    } else {
                        Image(systemName: "person.crop.circle")
                    }
                }
                .accessibilityLabel("About this Marble")
            }
            if let conversationId = marble?.conversationId, !conversationId.isEmpty {
                ToolbarItem(placement: .topBarTrailing) {
                    MarbleChatTools(conversationId: conversationId)
                }
            }
        }
    }

    @ViewBuilder
    private func transcript(_ marble: MarblePrismMarble) -> some View {
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
                        ContentUnavailableView {
                            Text("Nothing yet")
                        } description: {
                            Text("Say hello — \(marble.name) answers in its own voice.")
                        }
                    }
                    let transcript = MarbleReasoningAssociation(entries)
                    ForEach(transcript.standalone, id: \.rowID) { entry in
                        MarbleEntryView(entry: entry, speaker: entry.kind == "assistant" || entry.kind == "pending"
                                        ? MarbleSpeakerStyle(name: marble.name, hue: marble.hue, glyph: marble.glyph)
                                        : nil, replyHue: marble.hue, reasoning: transcript.attached[entry.id])
                    }
                    let reasoning = active.map { store.reasoning(for: $0.id) } ?? ""
                    if working && !reasoning.isEmpty
                        && !entries.contains(where: {
                            $0.taskId == active?.id && ["reasoning", "assistant"].contains($0.kind)
                        }) {
                        MarbleReasoningRow(text: reasoning)
                    }
                    MarbleTurnTail(task: active, entries: entries, showsLiveTools: true)
                    if thinking && !entries.contains(where: { $0.kind == "assistant" && $0.taskId == active?.id }) {
                        MarbleThinkingBubble(name: marble.name)
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
    @State private var glyph = "circle"
    @State private var hue = 0
    @State private var confirmRemove = false

    private var marble: MarblePrismMarble? {
        store.prism.marbles.first { $0.id == marbleId }
    }

    var body: some View {
        Form {
            if let marble {
                Section {
                    HStack {
                        Spacer()
                        VStack(spacing: 6) {
                            MarbleAvatar(glyph: glyph, hue: hue, size: 88)
                            Text(marble.name).font(.largeTitle.weight(.semibold))
                            if !marble.tagline.isEmpty {
                                Text(marble.tagline).foregroundStyle(MarbleTheme.muted)
                            }
                        }
                        Spacer()
                    }
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

                Section {
                    TextField("Who is this Marble?", text: $persona, axis: .vertical)
                        .lineLimit(2...6)
                    Button("Save personality") {
                        Task { await store.savePersona(marble, persona: persona) }
                    }
                    .disabled(persona == marble.persona)
                } header: {
                    Text("Personality")
                } footer: {
                    Text("A new personality starts a fresh conversation for \(marble.name); the old chat stays readable.")
                }

                Section("Look") {
                    MarbleLookPicker(glyph: $glyph, hue: $hue)
                        .onChange(of: glyph) { _, _ in saveLook(marble) }
                        .onChange(of: hue) { _, _ in saveLook(marble) }
                }
                Section {
                    DisclosureGroup("Workspace and abilities") {
                        LabeledContent("Workspace", value: marble.workspace.isEmpty ? "Host workspace" : marble.workspace)
                        LabeledContent("Model", value: store.model.isEmpty ? "Host default" : store.model)
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
            glyph = marble.glyph
            hue = marble.hue
        }
        .confirmationDialog(
            "Remove \(marble?.name ?? "this Marble") and their chat?",
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

    private func saveLook(_ marble: MarblePrismMarble) {
        guard personaLoaded else { return }
        let chosen = glyph
        let chosenHue = hue
        guard chosen != marble.glyph || chosenHue != marble.hue else { return }
        Task {
            await store.updateMarble(marble.id, update: .init(glyph: chosen, hue: chosenHue))
        }
    }
}
