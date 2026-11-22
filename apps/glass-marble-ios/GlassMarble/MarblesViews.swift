import SwiftUI

/// A marble's shape and hue as a small avatar — how people tell them apart.
struct MarbleAvatar: View {
    var glyph: String
    var hue: Int
    var size: CGFloat = 22

    var body: some View {
        Image(systemName: symbol)
            .font(.system(size: size * 0.62, weight: .semibold))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background(
                Color(
                    hue: Double((hue % 360 + 360) % 360) / 360,
                    saturation: 0.55, brightness: 0.72
                ),
                in: RoundedRectangle(cornerRadius: size * 0.3, style: .continuous)
            )
            .accessibilityHidden(true)
    }

    private var symbol: String {
        switch glyph {
        case "hex": "hexagon.fill"
        case "shield": "shield.fill"
        case "drop": "drop.fill"
        case "star": "star.fill"
        case "square": "square.fill"
        default: "circle.fill"
        }
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
/// once. The header bar shows who's in the room; tapping one opens its info.
struct EveryoneView: View {
    @EnvironmentObject private var store: MarbleStore
    @EnvironmentObject private var navigation: MarbleNavigationModel
    @State private var draft = ""
    @State private var follow = MarbleScrollFollow()
    @State private var userDriven = false
    @State private var showsActivity = false

    private static let bottomAnchor = "everyone-bottom"

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 16) {
                    if entries.isEmpty && !store.loaded {
                        ProgressView().frame(maxWidth: .infinity).padding(.top, 60)
                    } else if entries.isEmpty {
                        ContentUnavailableView {
                            Text("No one has spoken yet")
                        } description: {
                            Text("Say something, or name a Marble with @ to call on it.")
                        }
                    }
                    ForEach(entries) { entry in
                        MarbleEntryView(
                            entry: entry,
                            speaker: store.speakerStyle(for: entry)
                        )
                    }
                    ForEach(store.everyoneThinking, id: \.self) { name in
                        MarbleThinkingBubble(name: name)
                    }
                    Color.clear
                        .frame(height: 1)
                        .id(Self.bottomAnchor)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
                .frame(maxWidth: 820, alignment: .leading)
                .frame(maxWidth: .infinity)
            }
            .scrollDismissesKeyboard(.interactively)
            .defaultScrollAnchor(.bottom)
            .defaultScrollAnchor(.bottom, for: .sizeChanges)
            .onScrollPhaseChange { _, phase in
                userDriven = phase == .tracking || phase == .interacting || phase == .decelerating
            }
            .onScrollGeometryChange(for: Double.self) { geometry in
                geometry.contentSize.height - geometry.contentOffset.y - geometry.containerSize.height
            } action: { _, distance in
                if userDriven { follow.userMoved(distanceFromBottom: distance) }
            }
            .onChange(of: entries.count) { _, _ in scrollIfFollowing(proxy) }
            .onChange(of: store.everyoneThinking.count) { _, _ in scrollIfFollowing(proxy) }
            .onAppear {
                if follow.initialFollow(now: now()) { proxy.scrollTo(Self.bottomAnchor, anchor: .bottom) }
                // Lazy rows settle their heights after the first pass; land
                // on the real end once they have.
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
                    if follow.following { proxy.scrollTo(Self.bottomAnchor, anchor: .bottom) }
                }
            }
            .onChange(of: store.loaded) { _, _ in scrollIfFollowing(proxy) }
            .marbleKeepsLatestVisible(proxy: proxy, anchor: Self.bottomAnchor, following: follow.following)
        }
        .safeAreaInset(edge: .top, spacing: 0) { header }
        .safeAreaInset(edge: .bottom, spacing: 0) { composer }
        .navigationTitle("Everyone")
        .navigationBarTitleDisplayMode(.inline)
        .toolbarBackground(.visible, for: .navigationBar)
        .toolbar {
            ToolbarItem(placement: .principal) {
                MarbleTitleView(title: "Everyone")
            }
        }
        .sheet(isPresented: $showsActivity) {
            NavigationStack {
                ActivityView()
                    .toolbar {
                        ToolbarItem(placement: .confirmationAction) {
                            Button("Done") { showsActivity = false }
                        }
                    }
            }
        }
    }

    private var entries: [MarbleEntry] { store.everyoneEntries }

    private func now() -> TimeInterval { Date().timeIntervalSinceReferenceDate }

    private func scrollIfFollowing(_ proxy: ScrollViewProxy) {
        // A new row is a discrete change, not streaming: no throttle.
        guard follow.following else { return }
        // Wait one pass so the new row has its height before scrolling to it.
        DispatchQueue.main.async {
            withAnimation(.easeOut(duration: 0.2)) {
                proxy.scrollTo(Self.bottomAnchor, anchor: .bottom)
            }
        }
    }

    /// Who's in the room, on the same bar as the title so messages scroll
    /// under material rather than behind bare avatars; plus a line when
    /// something needs a decision.
    private var header: some View {
        VStack(spacing: 0) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 16) {
                    ForEach(store.prism.marbles) { marble in
                        Button {
                            navigation.openMarbleInfo(marble.id, fromEveryone: true)
                        } label: {
                            VStack(spacing: 3) {
                                MarbleAvatar(glyph: marble.glyph, hue: marble.hue, size: 28)
                                    .overlay(alignment: .bottomTrailing) {
                                        if marble.isWorking {
                                            Circle()
                                                .fill(Color.accentColor)
                                                .frame(width: 9, height: 9)
                                                .overlay(Circle().strokeBorder(Color(.systemBackground), lineWidth: 1.5))
                                                .offset(x: 3, y: 3)
                                        }
                                    }
                                Text(marble.name)
                                    .font(.caption2)
                                    .foregroundStyle(.primary)
                                    .lineLimit(1)
                            }
                            .frame(minWidth: 44)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("\(marble.name) info")
                    }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 8)
            }
            let waiting = store.state.approvals.count
            if waiting > 0 {
                Button { showsActivity = true } label: {
                    HStack(spacing: 8) {
                        Image(systemName: "hand.raised.fill")
                        Text(waiting == 1 ? "1 request needs you" : "\(waiting) requests need you")
                        Spacer()
                        Image(systemName: "chevron.right").font(.caption.weight(.semibold))
                    }
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(Color.accentColor)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 8)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
            }
        }
        .background {
            // Reaches up behind the navigation bar so title, avatars, and
            // the request line read as one solid bar.
            Rectangle()
                .fill(.bar)
                .background(Color(.systemBackground).opacity(0.7))
                .overlay(alignment: .bottom) { Divider() }
                .ignoresSafeArea(edges: .top)
        }
    }

    /// The composer with @Name completion from the roster.
    private var composer: some View {
        VStack(spacing: 0) {
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
                onSend: send,
                onStop: {}
            )
        }
        .marbleComposerBackground()
    }

    private func send() {
        let text = draft
        draft = ""
        Task { await store.sendEveryone(text) }
    }
}

/// The roster: every marble with its look, one line of personality, and what
/// it's doing — plus the built-in Marble whose chat is the chats list.
struct MarblesView: View {
    @EnvironmentObject private var store: MarbleStore
    @EnvironmentObject private var navigation: MarbleNavigationModel
    @State private var showsNew = false

    var body: some View {
        List {
            ForEach(store.prism.marbles) { marble in
                NavigationLink(value: marble.isDefault
                    ? ChatRoute.marbleChats
                    : ChatRoute.marbleChat(marble.id)) {
                    HStack(spacing: 12) {
                        MarbleAvatar(glyph: marble.glyph, hue: marble.hue, size: 34)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(marble.name).font(.callout)
                            if !marble.tagline.isEmpty {
                                Text(marble.tagline)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
                            }
                            Text(marble.displayState)
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                    }
                    .padding(.vertical, 2)
                }
            }
        }
        .navigationTitle("Marbles")
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button { showsNew = true } label: {
                    Image(systemName: "plus")
                }
                .accessibilityLabel("New Marble")
            }
        }
        .sheet(isPresented: $showsNew) {
            NavigationStack { NewMarbleView() }
        }
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
    @State private var hue: Double = 40

    private let starters = [
        "Warm and encouraging; celebrates progress and explains things simply.",
        "Careful and methodical; checks work twice and says what it verified.",
        "Direct and quick; short answers, gets to the point."
    ]

    var body: some View {
        Form {
            Section(header: Text("Name")) {
                TextField("Ada", text: $name)
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
            Section(header: Text("Look")) {
                HStack(spacing: 14) {
                    ForEach(MarbleMentions.glyphs, id: \.self) { shape in
                        Button {
                            glyph = shape
                        } label: {
                            MarbleAvatar(glyph: shape, hue: Int(hue), size: 34)
                                .overlay(
                                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                                        .strokeBorder(
                                            glyph == shape ? MarbleTheme.tint : Color.clear,
                                            lineWidth: 2
                                        )
                                        .frame(width: 40, height: 40)
                                )
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("\(shape) shape")
                    }
                }
                HStack {
                    MarbleAvatar(glyph: glyph, hue: Int(hue), size: 28)
                    Slider(value: $hue, in: 0...359, step: 1)
                }
            }
        }
        .navigationTitle("New Marble")
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel") { dismiss() }
            }
            ToolbarItem(placement: .confirmationAction) {
                Button("Create") { create() }
                    .disabled(name.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .onAppear {
            hue = Double(MarbleMentions.nextHue(used: store.prism.marbles.map(\.hue)))
        }
    }

    private func create() {
        let marbleName = name.trimmingCharacters(in: .whitespaces)
        Task {
            await store.addMarble(
                name: marbleName,
                glyph: glyph,
                hue: Int(hue),
                persona: persona.trimmingCharacters(in: .whitespacesAndNewlines)
            )
            dismiss()
            if let created = store.prism.marbles.first(where: {
                $0.name.caseInsensitiveCompare(marbleName) == .orderedSame && !$0.isDefault
            }) {
                navigation.openMarble(created.id)
            }
        }
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
        .toolbar {
            ToolbarItem(placement: .principal) {
                MarbleTitleView(title: marble?.name ?? "Marble")
            }
            ToolbarItem(placement: .topBarTrailing) {
                NavigationLink(value: ChatRoute.marbleInfo(marbleId)) {
                    Image(systemName: "info.circle")
                }
                .accessibilityLabel("About this Marble")
            }
        }
    }

    private static let bottomAnchor = "marble-chat-bottom"

    @ViewBuilder
    private func transcript(_ marble: MarblePrismMarble) -> some View {
        let entries = store.marbleTranscript(marble)
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 16) {
                    if entries.isEmpty && !marble.isWorking {
                        ContentUnavailableView {
                            Text("Nothing yet")
                        } description: {
                            Text("Say hello — \(marble.name) answers in its own voice.")
                        }
                    }
                    ForEach(entries) { entry in
                        MarbleEntryView(entry: entry)
                    }
                    if marble.isWorking {
                        MarbleThinkingBubble(name: marble.name)
                    }
                    Color.clear
                        .frame(height: 1)
                        .id(Self.bottomAnchor)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
                .frame(maxWidth: 820, alignment: .leading)
                .frame(maxWidth: .infinity)
            }
            .scrollDismissesKeyboard(.interactively)
            .defaultScrollAnchor(.bottom)
            .defaultScrollAnchor(.bottom, for: .sizeChanges)
            .onChange(of: entries.count) { _, _ in scrollToLatest(proxy) }
            .onChange(of: marble.isWorking) { _, _ in scrollToLatest(proxy) }
            .onAppear {
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
                    proxy.scrollTo(Self.bottomAnchor, anchor: .bottom)
                }
            }
            .marbleKeepsLatestVisible(proxy: proxy, anchor: Self.bottomAnchor, following: true)
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            ComposerBar(
                draft: $draft,
                isWorking: marble.isWorking,
                placeholder: "Message \(marble.name)",
                canSteer: false,
                onSend: { send(marble) },
                onStop: { Task { await store.stop(marble) } }
            )
            .marbleComposerBackground()
        }
    }

    private func scrollToLatest(_ proxy: ScrollViewProxy) {
        DispatchQueue.main.async {
            withAnimation(.easeOut(duration: 0.2)) { proxy.scrollTo(Self.bottomAnchor, anchor: .bottom) }
        }
    }

    private func send(_ marble: MarblePrismMarble) {
        let text = draft
        draft = ""
        Task {
            // A marble with no chat yet goes through Everyone's dispatch so
            // its first turn creates its durable conversation.
            if marble.conversationId.isEmpty {
                await store.sendToMarble(marble, text: text)
            } else {
                store.selectedConversationId = marble.conversationId
                await store.send(text)
            }
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
    @State private var hue: Double = 0
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
                            MarbleAvatar(glyph: glyph, hue: Int(hue), size: 54)
                            Text(marble.name).font(.headline)
                        }
                        Spacer()
                    }
                    .listRowBackground(Color.clear)
                }

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

                Section(header: Text("Look")) {
                    HStack(spacing: 14) {
                        ForEach(MarbleMentions.glyphs, id: \.self) { shape in
                            Button { glyph = shape } label: {
                                MarbleAvatar(glyph: shape, hue: Int(hue), size: 30)
                                    .opacity(glyph == shape ? 1 : 0.4)
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    Slider(value: $hue, in: 0...359, step: 1)
                        .onChange(of: glyph) { _, _ in saveLook(marble) }
                        .onChange(of: hue) { _, _ in saveLook(marble) }
                }

                Section {
                    let facts = store.memories(for: marble.id).filter { $0.owner == marble.id }
                    if facts.isEmpty && !marble.isDefault {
                        Text("Nothing of its own yet. Shared memories apply to every Marble.")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                    ForEach(facts) { fact in
                        Text(fact.text).font(.callout)
                    }
                    if marble.isDefault {
                        Text("Marble's own memories appear here; shared memories apply to every Marble.")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                } header: {
                    Text("Memory")
                } footer: {
                    if !marble.isDefault {
                        Text("Shared memories apply to every Marble.")
                    }
                }

                Section(header: Text("Workspace")) {
                    Text(marble.workspace.isEmpty ? "the host workspace" : marble.workspace)
                        .foregroundStyle(.secondary)
                }

                Section(header: Text("What \(marble.name) can do")) {
                    LabeledContent("Model", value: store.model.isEmpty ? "host default" : store.model)
                        .foregroundStyle(.secondary)
                    NavigationLink("Tools") { WhatMarbleCanUseView() }
                    Text("Its own model, skills, and schedules owned by a Marble are coming.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
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
        .navigationTitle(marble?.name ?? "Marble")
        .navigationBarTitleDisplayMode(.inline)
        .onAppear {
            guard !personaLoaded, let marble else { return }
            personaLoaded = true
            persona = marble.persona
            glyph = marble.glyph
            hue = Double(marble.hue)
        }
        .confirmationDialog(
            "Remove \(marble?.name ?? "this Marble") and her chat?",
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
        let chosenHue = Int(hue)
        guard chosen != marble.glyph || chosenHue != marble.hue else { return }
        Task {
            await store.updateMarble(marble.id, update: .init(glyph: chosen, hue: chosenHue))
        }
    }
}
