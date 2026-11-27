import GlassModels
import SwiftUI

struct ThreadsView: View {
    @Environment(AppModel.self) private var app
    @State private var threads: ThreadsModel?

    var body: some View {
        Group {
            if let threads {
                ConversationShell(threads: threads)
            } else {
                ProgressView()
            }
        }
        .task { if threads == nil { threads = ThreadsModel(app: app) } }
    }
}

private struct ConversationShell: View {
    @Environment(AppModel.self) private var app
    @Environment(\.horizontalSizeClass) private var sizeClass
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Bindable var threads: ThreadsModel
    @State private var sidebar = NavigationSplitViewVisibility.all
    @State private var drawer = false

    var body: some View {
        Group {
            if sizeClass == .compact {
                compact
            } else {
                NavigationSplitView(columnVisibility: $sidebar) {
                    ThreadListView(threads: threads, choose: open, new: newChat, close: nil)
                        .navigationSplitViewColumnWidth(min: 330, ideal: 355, max: 395)
                } detail: {
                    detail
                }
                .navigationSplitViewStyle(.balanced)
            }
        }
        .onChange(of: threads.selection, initial: true) { _, id in Attention.visibleThread = id }
        #if DEBUG
        .task {
            guard let preview = LensPreview.current else { return }
            if preview == .drawer, sizeClass == .compact { drawer = true }
            if preview == .settings { app.show(nil) }
            if let selection = preview.threadID {
                await threads.reload()
                if threads.thread(selection) != nil { threads.selection = selection }
            }
        }
        .onChange(of: threads.loaded) { _, loaded in
            guard loaded, let selection = LensPreview.current?.threadID, threads.selection == nil,
                  threads.thread(selection) != nil else { return }
            threads.selection = selection
        }
        #endif
        .onChange(of: app.selectedTab) { _, tab in
            Attention.visibleThread = tab == .threads ? threads.selection : nil
        }
        .sheet(item: Binding(get: {
            app.selectedTab == .threads ? nil : app.selectedTab
        }, set: { tab in app.selectedTab = tab ?? .threads })) { tab in
            NavigationStack(path: Bindable(app).settingsPath) {
                Group {
                    switch tab {
                    case .desktop: DesktopView()
                    case .settings: SettingsView().navigationDestination(for: SettingsRoute.self) { $0.destination }
                    case .threads: EmptyView()
                    }
                }
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        Button("Done") { app.selectedTab = .threads }
                    }
                }
            }
        }
    }

    private var compact: some View {
        GeometryReader { geometry in
            let width = min(360, geometry.size.width * 0.88)
            ZStack(alignment: .leading) {
                detail
                    .environment(\.sidebarOpen, drawer)
                    .accessibilityHidden(drawer)
                if drawer {
                    Color.primary.opacity(0.18)
                        .ignoresSafeArea()
                        .onTapGesture { setDrawer(false) }
                        .accessibilityHidden(true)
                    NavigationStack {
                        ThreadListView(threads: threads, choose: open, new: newChat, close: { setDrawer(false) })
                            .containerBackground(.clear, for: .navigation)
                    }
                    .frame(width: width)
                    // One floating glass panel: header, search, list and footer all sit on this surface.
                    .clipShape(.rect(cornerRadius: Design.drawerCorner, style: .continuous))
                    .background(reduceTransparency ? Design.surface : .clear,
                                in: .rect(cornerRadius: Design.drawerCorner, style: .continuous))
                    .glassEffect(reduceTransparency ? .identity : .regular,
                                 in: .rect(cornerRadius: Design.drawerCorner, style: .continuous))
                    .padding(.leading, Design.gap)
                    .padding(.vertical, Design.gap)
                    .transition(.opacity)
                    .overlay(alignment: .trailing) {
                        Color.clear.frame(width: 18)
                            .contentShape(Rectangle())
                            .gesture(DragGesture(minimumDistance: 24).onEnded { value in
                                if value.translation.width < -70 { setDrawer(false) }
                            })
                            .accessibilityHidden(true)
                    }
                } else {
                    Color.clear
                        .frame(width: 18)
                        .contentShape(Rectangle())
                        .gesture(DragGesture(minimumDistance: 22).onEnded { value in
                            if value.translation.width > 50 { setDrawer(true) }
                        })
                        .accessibilityHidden(true)
                }
            }
        }
    }

    @ViewBuilder
    private var detail: some View {
        if let id = threads.selection, let thread = threads.thread(id) {
            ConversationView(conversation: threads.conversation(for: thread), threads: threads, goBack: toggleSidebar)
                .id(id)
        } else {
            StartConversationView(threads: threads, menu: toggleSidebar, newChat: newChat)
        }
    }

    private func newChat() {
        setDrawer(false)
        Task { await threads.newThread() }
    }

    private func open(_ id: String) {
        threads.selection = id
        setDrawer(false)
    }

    private func toggleSidebar() {
        if sizeClass == .compact { setDrawer(!drawer) }
        else { sidebar = sidebar == .detailOnly ? .all : .detailOnly }
    }

    private func setDrawer(_ open: Bool) {
        withAnimation(reduceMotion ? nil : .spring(duration: 0.32, bounce: 0.06)) { drawer = open }
    }
}

struct ThreadListView: View {
    @Environment(AppModel.self) private var app
    @Bindable var threads: ThreadsModel
    var choose: (String) -> Void
    var new: () -> Void
    var close: (() -> Void)?
    @State private var rename: ThreadSummary?
    @State private var name = ""
    @State private var delete: ThreadSummary?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    var body: some View {
        let groups = sections
        let titles = threads.titles()
        ScrollViewReader { proxy in
        List {
            if threads.loaded, threads.threads.isEmpty, threads.error == nil {
                ContentUnavailableView(threads.query.isEmpty ? "No conversations yet" : "No matching conversations",
                                       systemImage: threads.query.isEmpty ? "bubble.left.and.bubble.right" : "magnifyingglass")
                    .accessibilityIdentifier("threads.empty")
            }
            if let error = threads.error {
                Text(error)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier("threads.error")
            }
            ForEach(groups, id: \.label) { section in
                Section {
                    ForEach(section.threads) { thread in
                        row(thread, title: titles[thread.id] ?? thread.lensTitle, section: section.label)
                    }
                } header: {
                    Text(section.label).font(.subheadline.weight(.semibold))
                        .textCase(nil)
                        .foregroundStyle(Design.ink)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.leading, Design.padding)
                        .listRowInsets(EdgeInsets(top: Design.gap, leading: 0, bottom: Design.small, trailing: 0))
                        .id(section.label)
                        .accessibilityIdentifier(section.label == groups.first?.label ? "threads.ready" : "threads.section")
                }
                .listSectionMargins(.horizontal, Design.gap)
                .listSectionSeparator(.hidden)
            }
            if threads.nextOffset != nil {
                Button("Load more") { Task { await threads.loadMore() } }
                    .task(id: threads.nextOffset) { await threads.loadMore() }
                    .disabled(threads.isLoading)
                    .accessibilityIdentifier("threads.more")
            }
        }
        .listStyle(.insetGrouped)
        .listSectionSpacing(Design.padding)
        .scrollContentBackground(.hidden)
        .scrollEdgeEffectStyle(.soft, for: [.top, .bottom])
        .safeAreaBar(edge: .top, spacing: Design.small) {
          VStack(spacing: Design.gap) {
            LensTitleBar(sideWidth: 2 * Design.control + 2 * Design.small) {
                if let close {
                    LensBarGroup {
                        Button(action: close) { LensBarIcon(symbol: "xmark") }
                            .buttonStyle(.plain)
                            .accessibilityLabel("Close threads")
                    }
                }
            } title: {
                Text("Threads").font(Design.headline).accessibilityAddTraits(.isHeader)
            } trailing: {
                LensBarGroup {
                    filters(groups, proxy: proxy)
                    Button(action: new) { LensBarIcon(symbol: "square.and.pencil") }
                        .buttonStyle(.plain)
                        .accessibilityLabel("New Chat")
                        .accessibilityIdentifier("threads.new")
                }
            }
            LensSearch(prompt: "Find a thread", query: $threads.query)
                .accessibilityIdentifier("threads.search")
                .frame(height: Design.control)
                .padding(.horizontal, Design.padding)
          }
        }
        .safeAreaBar(edge: .bottom, spacing: Design.small) { accountBar }
        }
        .scrollDismissesKeyboard(.interactively)
        .task(id: Filter(query: threads.query, project: threads.project, archived: threads.archived)) {
            if !threads.query.isEmpty {
                do { try await Task.sleep(for: .milliseconds(200)) } catch { return }
            }
            await threads.reload()
        }
        .refreshable { await threads.reload() }
        .overlay {
            if threads.isLoading && !threads.loaded { ProgressView().allowsHitTesting(false) }
        }
        .toolbarVisibility(.hidden, for: .navigationBar)
        .alert("Rename Conversation", isPresented: Binding(get: { rename != nil }, set: { if !$0 { rename = nil } })) {
            TextField("Title", text: $name)
            Button("Save") {
                if let rename { Task { await threads.update(rename, .rename(name)) } }
            }
            Button("Cancel", role: .cancel) {}
        }
        .confirmationDialog("Delete Conversation?", isPresented: Binding(get: { delete != nil }, set: { if !$0 { delete = nil } }), titleVisibility: .visible) {
            Button("Delete", role: .destructive) {
                if let delete { Task { await threads.update(delete, .delete) } }
            }
        } message: {
            Text(delete?.displayTitle ?? "")
        }
    }

    private var accountBar: some View {
        HStack(spacing: Design.gap) {
            LensBarGroup {
                Button { app.show(nil) } label: {
                    Label("Settings", systemImage: "gearshape")
                        .padding(.horizontal, Design.gap)
                        .frame(height: Design.control)
                        .contentShape(.capsule)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("tab.settings")
            }
            Spacer(minLength: 0)
            LensBarGroup {
                Button { app.selectedTab = .desktop } label: {
                    Label("Desktop", systemImage: "display")
                        .padding(.horizontal, Design.gap)
                        .frame(height: Design.control)
                        .contentShape(.capsule)
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("tab.desktop")
            }
        }
        .font(.subheadline.weight(.semibold))
        .lineLimit(1)
        .padding(.horizontal, Design.padding)
        .padding(.bottom, Design.small)
    }

    private func filters(_ groups: [(label: String, threads: [ThreadSummary])], proxy: ScrollViewProxy) -> some View {
        Menu {
            if groups.count > 1 {
                Section("Jump to") {
                    ForEach(groups, id: \.label) { group in
                        Button(group.label) {
                            withAnimation(reduceMotion ? nil : .snappy(duration: 0.24)) {
                                proxy.scrollTo(group.label, anchor: .top)
                            }
                        }
                    }
                }
            }
            Picker("Conversations", selection: $threads.archived) {
                Text("Active").tag(false)
                Text("Archived").tag(true)
            }
            Picker("Working directory", selection: $threads.project) {
                Text("All directories").tag("")
                ForEach(threads.projects) { project in
                    Text(project.workspace.isEmpty ? "Unassigned" : project.name)
                        .tag(project.workspace.isEmpty ? "@unassigned" : project.workspace)
                }
            }
            if let host = app.endpoint?.url.host {
                Section("Machine") { Label(host, systemImage: "checkmark") }
            }
        } label: { LensBarIcon(symbol: "line.3.horizontal.decrease") }
        .buttonStyle(.plain)
        .accessibilityLabel("Filters")
        .accessibilityIdentifier("threads.filters")
    }

    private func row(_ thread: ThreadSummary, title: String, section: String) -> some View {
        Button { choose(thread.id) } label: {
            ThreadRow(thread: thread, title: title, section: section, selected: threads.selection == thread.id)
                .contentShape(.capsule)
                .contentShape(.contextMenuPreview, .capsule)
        }
        .buttonStyle(.plain)
        .listRowInsets(EdgeInsets(top: 1, leading: 0, bottom: 1, trailing: 0))
        .listRowBackground(Color.clear)
        .listRowSeparator(.hidden)
        .accessibilityAddTraits(threads.selection == thread.id ? .isSelected : [])
        .accessibilityIdentifier("thread.\(thread.id)")
        .swipeActions(edge: .leading, allowsFullSwipe: false) { pin(thread) }
        .swipeActions(allowsFullSwipe: false) { remove(thread); archive(thread) }
        .contextMenu {
            Button("Rename", systemImage: "pencil") {
                name = title
                rename = thread
            }
            pin(thread)
            archive(thread)
            remove(thread)
        }
    }

    private func pin(_ thread: ThreadSummary) -> some View {
        Button(thread.pinned == true ? "Unpin" : "Pin", systemImage: thread.pinned == true ? "pin.slash" : "pin") {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            Task { await threads.update(thread, .pin(thread.pinned != true)) }
        }
        .tint(.secondary)
    }

    private func archive(_ thread: ThreadSummary) -> some View {
        Button(thread.archived == true ? "Unarchive" : "Archive", systemImage: thread.archived == true ? "tray.and.arrow.up" : "archivebox") {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            Task { await threads.update(thread, .archive(thread.archived != true)) }
        }
        .tint(.secondary)
    }

    private func remove(_ thread: ThreadSummary) -> some View {
        Button("Delete", systemImage: "trash", role: .destructive) { delete = thread }
    }

    private var sections: [(label: String, threads: [ThreadSummary])] {
        Self.sections(for: threads.threads)
    }

    static func sections(for threads: [ThreadSummary], now: Date = .now,
                         calendar: Calendar = .current) -> [(label: String, threads: [ThreadSummary])] {
        let today = calendar.startOfDay(for: now)
        func label(_ thread: ThreadSummary) -> String {
            if thread.pinned == true { return "Pinned" }
            guard let date = thread.modifiedDate else { return "Date Unknown" }
            let days = calendar.dateComponents([.day], from: calendar.startOfDay(for: date), to: today).day ?? 0
            switch days {
            case ...0: return "Today"
            case 1: return "Yesterday"
            case 2...7: return "Previous 7 Days"
            default: return date.formatted(.dateTime.month(.wide).year())
            }
        }
        let ordered = threads.sorted { left, right in
            if (left.pinned == true) != (right.pinned == true) { return left.pinned == true }
            return (left.modifiedDate ?? .distantPast) > (right.modifiedDate ?? .distantPast)
        }
        var sections: [(label: String, threads: [ThreadSummary])] = []
        for thread in ordered {
            let name = label(thread)
            if sections.last?.label == name {
                sections[sections.count - 1].threads.append(thread)
            } else {
                sections.append((name, [thread]))
            }
        }
        return sections
    }

    private struct Filter: Hashable {
        let query: String
        let project: String
        let archived: Bool
    }
}

private struct ThreadRow: View {
    @Environment(\.colorScheme) private var scheme
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    var thread: ThreadSummary
    var title: String
    var section: String
    var selected: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title).font(.subheadline.weight(selected ? .semibold : .medium))
                .foregroundStyle(Design.ink)
            HStack(spacing: Design.small) {
                if thread.running == true { ActivityIndicator().tint(Design.accent).accessibilityLabel("Running now") }
                Text(ThreadListView.metadata(for: thread, section: section)).font(.caption).foregroundStyle(Design.muted)
            }
        }
        .lineLimit(1)
        .accessibilityElement(children: .combine)
        .padding(.horizontal, Design.padding)
        .padding(.vertical, Design.gap)
        .frame(maxWidth: .infinity, minHeight: Design.control, alignment: .leading)
        .background {
            if selected {
                // A brighter pane of the same glass, raised slightly off the panel.
                Capsule()
                    .fill(scheme == .dark ? Color.white.opacity(0.08) : Color.white.opacity(0.5))
                    .glassEffect(reduceTransparency ? .identity : .regular, in: .capsule)
                    .shadow(color: .black.opacity(scheme == .dark ? 0.24 : 0.06), radius: 6, y: 2)
            }
        }
    }
}

extension ThreadListView {
    /// "3:13 PM · DeepSeek V4.1 Flash": a time that adds to the section header, then the model or origin.
    static func metadata(for thread: ThreadSummary, section: String, now: Date = .now) -> String {
        var time: String?
        if let date = thread.modifiedDate {
            switch section {
            case "Today", "Yesterday": time = date.formatted(date: .omitted, time: .shortened)
            case "Previous 7 Days": time = date.formatted(.dateTime.weekday(.wide))
            case "Pinned": time = date.formatted(.relative(presentation: .named, unitsStyle: .abbreviated))
            default: time = date.formatted(.dateTime.month(.abbreviated).day())
            }
        }
        let detail = thread.model.flatMap(ModelName.friendly) ?? thread.originNote
        return [time, detail].compactMap { $0?.isEmpty == false ? $0 : nil }.joined(separator: " · ")
    }
}
