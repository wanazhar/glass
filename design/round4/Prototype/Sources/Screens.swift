import SwiftUI

enum Screen: String, CaseIterable {
    case empty, active, working, failed, drawer, settings

    var title: String {
        switch self {
        case .empty: "New chat"
        case .active: "Active conversation"
        case .working: "Working"
        case .failed: "Run failed"
        case .drawer: "Thread drawer"
        case .settings: "Settings"
        }
    }
}

struct WorkspacePreview: View {
    let initialScreen: Screen
    @State private var selected: Screen = .active
    @State private var showingDrawer = false
    @State private var splitSidebar = true
    @State private var showingModel = false
    @State private var draft = ""
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(\.palette) private var p

    private var split: Bool { sizeClass == .regular }
    private var title: String { split ? "Fix the nil crash in TitleView" : "Glass" }

    var body: some View {
        GeometryReader { geometry in
            HStack(spacing: 0) {
                if split && splitSidebar && selected != .settings {
                    ThreadDrawer(embedded: true, close: { splitSidebar = false }, newChat: newChat,
                                 openThread: openThread, settings: showSettings)
                        .frame(width: 275)
                    Rectangle().fill(p.line).frame(width: 1)
                }
                Group {
                    switch selected {
                    case .empty:
                        EmptyChat(title: "Glass", split: split, menu: toggleDrawer, newChat: newChat,
                                  draft: $draft, chooseModel: { showingModel = true })
                    case .settings:
                        SettingsPreview(back: openThread)
                    case .active, .working, .failed, .drawer:
                        TranscriptSurface(title: title, mode: selected == .working ? .working : .idle,
                                          managesSidebar: split && splitSidebar, menu: toggleDrawer, newChat: newChat) {
                            ConversationSample(screen: selected)
                        }
                        .id(selected)
                    }
                }
                .frame(maxWidth: .infinity)
            }
            .overlay(alignment: .leading) {
                if showingDrawer && !split {
                    ZStack(alignment: .leading) {
                        Color.black.opacity(p.dark ? 0.38 : 0.15)
                            .ignoresSafeArea()
                            .onTapGesture(perform: openThread)
                        ThreadDrawer(close: openThread, newChat: newChat, openThread: openThread, settings: showSettings)
                            .frame(width: min(geometry.size.width - 53, 350))
                            .padding(.leading, 5)
                    }
                }
            }
        }
        .onAppear {
            selected = initialScreen == .drawer ? .active : initialScreen
            showingDrawer = initialScreen == .drawer
        }
        .sheet(isPresented: $showingModel) {
            ModelPanel()
                .padding(20)
                .frame(maxHeight: .infinity, alignment: .top)
                .presentationBackground(p.paper)
                .presentationDragIndicator(.visible)
                .presentationDetents([.medium, .large])
        }
    }

    private func toggleDrawer() {
        if split { splitSidebar.toggle() } else { showingDrawer.toggle() }
    }

    private func openThread() {
        showingDrawer = false
        selected = .active
    }

    private func newChat() {
        showingDrawer = false
        selected = .empty
        draft = ""
    }

    private func showSettings() {
        showingDrawer = false
        selected = .settings
    }
}

private struct EmptyChat: View {
    let title: String
    let split: Bool
    let menu: () -> Void
    let newChat: () -> Void
    @Binding var draft: String
    let chooseModel: () -> Void
    @Environment(\.palette) private var p

    var body: some View {
        ViewThatFits(in: .vertical) {
            VStack(spacing: 24) {
                Spacer(minLength: 10)
                welcome
                Spacer(minLength: 10)
            }
            ScrollView { welcome.padding(.vertical, 32) }
        }
        .safeAreaBar(edge: .top, spacing: 6) {
            ChatToolbar(title: title, leadingAction: !split, trailingAction: !split, menu: menu, newChat: newChat)
                .padding(.horizontal, 18)
        }
        .safeAreaBar(edge: .bottom, spacing: 6) {
            Composer(mode: draft.isEmpty ? .idle : .typing, draftText: draft, tapModel: chooseModel)
                .id(draft)
                .padding(.horizontal, 18)
        }
    }

    private var welcome: some View {
        VStack(spacing: 24) {
            Horizon(fullWidth: true)
                .stroke(p.muted.opacity(p.dark ? 0.60 : 0.34), lineWidth: 0.8)
                .frame(height: 112)
                .overlay {
                    LensMark(optical: true).frame(width: 112, height: 112)
                }
            Text("Start with a change")
                .font(.title2.weight(.semibold))
                .multilineTextAlignment(.center)
                .padding(.horizontal, 20)
            GlassEffectContainer(spacing: 12) {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 12) { suggestions }
                    VStack(spacing: 10) { suggestions }
                }
            }
            .padding(.horizontal, 18)
        }
        .frame(maxWidth: 620)
        .frame(maxWidth: .infinity)
    }

    @ViewBuilder private var suggestions: some View {
        suggestion("Review code", icon: "chevron.left.forwardslash.chevron.right",
                   task: "Review the changes in my current branch.")
        suggestion("Plan a project", icon: "list.bullet.clipboard",
                   task: "Help me plan the next milestone.")
    }

    private func suggestion(_ label: String, icon: String, task: String) -> some View {
        Button { draft = task } label: {
            Label(label, systemImage: icon)
                .font(.subheadline.weight(.medium))
                .frame(minHeight: 44)
                .padding(.horizontal, 14)
                .foregroundStyle(p.ink)
        }
        .buttonStyle(.plain)
        .glassEffect(.regular.interactive(), in: .capsule)
    }
}

struct ConnectionPreview: View {
    let offline: Bool
    @State private var showBanner = true

    var body: some View {
        TranscriptSurface(title: "Glass", mode: .idle, connectionBanner: showBanner ? offline : nil,
                          connectionAction: { showBanner = false }) {
            ConversationSample(screen: .active)
        }
    }
}

private struct ConversationSample: View {
    let screen: Screen
    @State private var showingOutput = false
    @State private var showingDiff = false
    @Environment(\.palette) private var p

    var body: some View {
        if screen == .working {
            UserBubble(message: "Can you help me fix a crash in my iOS app?")
            reply("Yes. Share the relevant code or crash details and I’ll trace what’s failing.")
            UserBubble(message: "Can you fix the nil crash in the title?")
            LiveTrace()
        } else {
            UserBubble(message: "Can you fix the nil crash in the title?")
            if screen == .failed {
                reply("The fallback is in place. One test command stopped before it could verify the fix.")
                FailureCard(output: { showingOutput = true })
                    .sheet(isPresented: $showingOutput) {
                        OutputDetail()
                            .padding(20)
                            .frame(maxHeight: .infinity, alignment: .top)
                            .presentationBackground(p.paper)
                            .presentationDragIndicator(.visible)
                            .presentationDetents([.height(285), .large])
                    }
            } else {
                reply("Use nil coalescing to give the title a safe default.")
                WorkLog(running: false, titleSample: true, onDiff: { showingDiff = true })
                    .sheet(isPresented: $showingDiff) {
                        DiffDetail(titleSample: true)
                            .padding(20)
                            .frame(maxHeight: .infinity, alignment: .top)
                            .presentationBackground(p.paper)
                            .presentationDragIndicator(.visible)
                            .presentationDetents([.height(255), .large])
                    }
                CodeCard(titleSample: true)
                reply("A missing title now displays “Untitled.” An existing title stays unchanged.")
                BenchmarkTable(titleSample: true)
            }
        }
    }

    private func reply(_ text: String) -> some View {
        Text(text).font(.body).lineSpacing(4)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct LiveTrace: View {
    @Environment(\.palette) private var p

    var body: some View {
        VStack(alignment: .leading, spacing: 13) {
            Text("Checking the fix").font(.headline)
            VStack(alignment: .leading, spacing: 12) {
                step("Read TitleView.swift", state: .done)
                step("Checking optional values", state: .live)
                step("Run title tests", state: .queued)
            }
            .background(alignment: .leading) {
                Rectangle().fill(p.muted.opacity(0.38)).frame(width: 1)
                    .padding(.vertical, 12)
                    .offset(x: 9.5)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.top, 4)
    }

    private enum State { case done, live, queued }

    private func step(_ title: String, state: State) -> some View {
        HStack(spacing: 11) {
            Group {
                switch state {
                case .done:
                    Image(systemName: "checkmark.circle.fill").foregroundStyle(p.accent)
                case .live:
                    ProgressView().controlSize(.small).tint(p.accent)
                case .queued:
                    Image(systemName: "circle").foregroundStyle(p.muted)
                }
            }
            .frame(width: 20, height: 21)
            .background(p.paper)
            .accessibilityHidden(true)
            Text(title)
                .font(.subheadline.weight(state == .live ? .semibold : .regular))
                .foregroundStyle(state == .queued ? p.muted : p.ink)
        }
        .accessibilityLabel(state == .live ? "\(title), in progress" : title)
    }
}

private struct ThreadGroup: Identifiable {
    let name: String
    let titles: [String]
    var id: String { name }

    static let samples: [Self] = [
        .init(name: "Pinned", titles: ["Swift loading fix", "Release notes"]),
        .init(name: "Today", titles: ["Review async calls", "Test retry behavior", "Update empty state"]),
        .init(name: "Yesterday", titles: ["Fix the nil crash in TitleView", "Audit nil title fixtures", "Update release checklist"]),
        .init(name: "Previous 7 Days", titles: archive(["Build diagnostics", "Watch startup", "Refine the sidebar"], count: 34)),
        .init(name: "Previous 30 Days", titles: archive(["Review pull requests", "Profile image decode", "Check offline launch"], count: 65)),
        .init(name: "October", titles: archive(["Snapshot baseline", "Resolve package warnings", "Polish settings"], count: 115)),
        .init(name: "September", titles: archive(["Verify archive flow", "Review staging build", "Simplify share controls"], count: 114)),
    ]

    private static func archive(_ names: [String], count: Int) -> [String] {
        (0..<count).map { offset in
            let title = names[offset % names.count]
            return offset < names.count ? title : "\(title) · pass \(offset / names.count + 1)"
        }
    }
}

private struct ThreadDrawer: View {
    var embedded = false
    let close: () -> Void
    let newChat: () -> Void
    let openThread: () -> Void
    let settings: () -> Void
    @State private var search = ""
    @Environment(\.palette) private var p

    private var groups: [ThreadGroup] {
        if search.isEmpty { return ThreadGroup.samples }
        return ThreadGroup.samples.compactMap { group in
            let titles = group.titles.filter { $0.localizedCaseInsensitiveContains(search) }
            return titles.isEmpty ? nil : ThreadGroup(name: group.name, titles: titles)
        }
    }

    var body: some View {
        content
            .background {
                if embedded {
                    p.panel.ignoresSafeArea()
                } else {
                    GlassEffectContainer {
                        RoundedRectangle(cornerRadius: 30)
                            .fill(p.paper.opacity(p.dark ? 0.50 : 0.60))
                            .glassEffect(.regular.tint(p.accent.opacity(p.dark ? 0.16 : 0.06)),
                                         in: .rect(cornerRadius: 30))
                    }
                    .padding(.vertical, -6)
                }
            }
    }

    private var content: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 17) {
                    ForEach(groups) { section in
                        sectionCard(section)
                            .id(section.id)
                    }
                }
                .padding(.vertical, 12)
                .padding(.horizontal, 14)
            }
            .scrollEdgeEffectStyle(.hard, for: [.top, .bottom])
            .safeAreaBar(edge: .top, spacing: 4) {
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        Button(action: close) {
                            Image(systemName: embedded ? "sidebar.leading" : "xmark")
                                .font(.system(size: 15, weight: .semibold))
                                .frame(width: 44, height: 44)
                        }
                        .accessibilityLabel(embedded ? "Hide sidebar" : "Close threads")
                        Spacer()
                        Button(action: newChat) {
                            Image(systemName: "square.and.pencil")
                                .font(.system(size: 16, weight: .semibold))
                                .frame(width: 44, height: 44)
                        }
                        .accessibilityLabel("New chat")
                    }
                    .buttonStyle(.plain)
                    Text("Threads").font(.largeTitle.weight(.semibold))
                    HStack(spacing: 8) {
                        Image(systemName: "magnifyingglass").foregroundStyle(p.muted)
                        TextField("Find a thread", text: $search, prompt: Text("Find a thread").foregroundStyle(p.muted))
                            .accessibilityLabel("Search threads")
                    }
                    .font(.subheadline)
                    .padding(.horizontal, 12)
                    .frame(minHeight: 39)
                    .background(p.inset, in: .rect(cornerRadius: 12))
                    HStack {
                        let count = groups.reduce(0) { $0 + $1.titles.count }
                        Text("\(count) threads")
                            .font(.caption.weight(.medium))
                            .foregroundStyle(p.muted)
                        Spacer()
                        Menu {
                            ForEach(groups) { group in
                                Button(group.name) {
                                    withAnimation(.snappy(duration: 0.24)) { proxy.scrollTo(group.id, anchor: .top) }
                                }
                            }
                        } label: {
                            Label("Jump to group", systemImage: "chevron.down")
                                .font(.caption.weight(.semibold))
                                .frame(minHeight: 28)
                        }
                        .tint(p.accent)
                    }
                }
                .padding(.horizontal, 14)
            }
            .safeAreaBar(edge: .bottom, spacing: 5) {
                Button(action: settings) {
                    HStack {
                        Label("Settings", systemImage: "gearshape")
                            .font(.subheadline.weight(.semibold))
                        Spacer()
                        Image(systemName: "chevron.right").font(.caption.weight(.semibold))
                    }
                    .padding(.horizontal, 15)
                    .frame(minHeight: 46)
                    .foregroundStyle(p.ink)
                }
                .buttonStyle(.plain)
                .glassEffect(.regular.interactive(), in: .capsule)
                .padding(.horizontal, 14)
                .padding(.bottom, 5)
            }
        }
    }

    private func sectionCard(_ section: ThreadGroup) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(section.name).font(.subheadline.weight(.semibold))
                .foregroundStyle(p.ink)
            LazyVStack(spacing: 0) {
                ForEach(section.titles.indices, id: \.self) { index in
                    let isRunning = section.name == "Today" && index == 0
                    if index > 0 { Divider().padding(.leading, 36) }
                    Button(action: openThread) {
                        HStack(spacing: 9) {
                            if isRunning {
                                ProgressView().controlSize(.mini).tint(p.accent).frame(width: 16)
                            } else {
                                Image(systemName: "bubble.left").foregroundStyle(p.muted).frame(width: 16)
                            }
                            Text(section.titles[index])
                                .font(.subheadline)
                                .fixedSize(horizontal: false, vertical: true)
                                .accessibilityValue(isRunning ? "Running" : "")
                            Spacer(minLength: 6)
                            Image(systemName: "chevron.right")
                                .font(.caption2.weight(.semibold))
                                .foregroundStyle(p.muted)
                        }
                        .padding(.horizontal, 11)
                        .frame(minHeight: 42)
                        .contentShape(.rect)
                    }
                    .buttonStyle(.plain)
                }
            }
            .background(p.panel.opacity(embedded || p.dark ? 0.91 : 0.84), in: .rect(cornerRadius: 13))
            .overlay(RoundedRectangle(cornerRadius: 13).strokeBorder(p.line.opacity(0.65)))
        }
    }
}

private struct SettingsPreview: View {
    let back: () -> Void
    @Environment(\.palette) private var p

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 23) {
                Text("Settings").font(.largeTitle.weight(.semibold))
                host
                group("PREFERENCES", rows: [
                    ("textformat.size", "Appearance", "System"),
                    ("bell", "Notifications", ""),
                    ("antenna.radiowaves.left.and.right", "Background connection", ""),
                    ("mic", "Voice", ""),
                ])
                group("WORKSPACE", rows: [
                    ("slider.horizontal.3", "New chat defaults", ""),
                    ("stethoscope", "Diagnostics", ""),
                    ("info.circle", "About Glass", ""),
                ])
            }
            .frame(maxWidth: 520)
            .padding(.horizontal, 22)
            .padding(.vertical, 15)
            .frame(maxWidth: .infinity)
        }
        .safeAreaBar(edge: .top, alignment: .leading, spacing: 6) {
            GlassIcon(icon: "chevron.left", label: "Back to chat", action: back)
                .padding(.leading, 18)
        }
        .scrollEdgeEffectStyle(.hard, for: .top)
    }

    private var host: some View {
        HStack(spacing: 12) {
            Image(systemName: "desktopcomputer")
                .font(.title2)
                .foregroundStyle(p.ink)
                .frame(width: 47, height: 47)
                .background(p.inset, in: .rect(cornerRadius: 11))
            VStack(alignment: .leading, spacing: 4) {
                Text("Studio Mac").font(.headline)
                Text("Connected").font(.subheadline).foregroundStyle(p.success)
            }
            Spacer()
            Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(p.muted)
        }
        .padding(15)
        .panel()
        .accessibilityElement(children: .combine)
    }

    private func group(_ label: String, rows: [(String, String, String)]) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Eyebrow(label)
            VStack(spacing: 0) {
                ForEach(Array(rows.enumerated()), id: \.offset) { index, row in
                    if index > 0 { Divider().padding(.leading, 44) }
                    HStack(spacing: 11) {
                        Image(systemName: row.0).frame(width: 22).foregroundStyle(p.muted)
                        Text(row.1).font(.subheadline).fixedSize(horizontal: false, vertical: true)
                        Spacer(minLength: 5)
                        if !row.2.isEmpty { Text(row.2).font(.caption).foregroundStyle(p.muted) }
                        Image(systemName: "chevron.right").font(.caption2.weight(.semibold)).foregroundStyle(p.muted)
                    }
                    .padding(.horizontal, 13)
                    .frame(minHeight: 45)
                }
            }
            .panel(13)
        }
    }
}
