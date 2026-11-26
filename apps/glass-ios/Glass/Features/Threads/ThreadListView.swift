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
    @Bindable var threads: ThreadsModel
    @State private var sidebar = NavigationSplitViewVisibility.all
    @State private var drawer = false
    @State private var starting = false
    @State private var launchLogo = true

    var body: some View {
        Group {
            if sizeClass == .compact {
                compact
            } else {
                NavigationSplitView(columnVisibility: $sidebar) {
                    ThreadListView(threads: threads, choose: open, new: newChat, close: nil)
                } detail: {
                    detail
                        .toolbarVisibility(.hidden, for: .navigationBar)
                }
            }
        }
        .onChange(of: threads.selection, initial: true) { _, id in Attention.visibleThread = id }
        .onChange(of: app.selectedTab) { _, tab in
            Attention.visibleThread = tab == .threads ? threads.selection : nil
        }
        .task {
            try? await Task.sleep(for: .milliseconds(1_300))
            launchLogo = false
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
            let width = min(360, geometry.size.width * 0.9)
            ZStack(alignment: .leading) {
                detail
                    .accessibilityHidden(drawer)
                if drawer {
                    Color.primary.opacity(0.18)
                        .ignoresSafeArea()
                        .onTapGesture { setDrawer(false) }
                        .accessibilityHidden(true)
                    NavigationStack {
                        ThreadListView(threads: threads, choose: open, new: newChat, close: { setDrawer(false) })
                    }
                    .frame(width: width)
                    .background(Design.background)
                    .transition(.move(edge: .leading))
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
            ScrollView {
                NewChatView(launchRipple: launchLogo) { suggestion in
                    Task {
                        guard !starting else { return }
                        starting = true
                        await threads.newThread()
                        if let id = threads.selection, let thread = threads.thread(id) {
                            threads.conversation(for: thread).composer.text = suggestion
                        }
                        starting = false
                    }
                }
                .padding(Design.padding)
            }
            .floatingBar(edge: .top) {
                HStack {
                    Button("Conversations", systemImage: "sidebar.left", action: toggleSidebar)
                        .labelStyle(.iconOnly).glassButtonStyle()
                        .accessibilityIdentifier("conversation.back")
                    Spacer()
                    Button("New Chat", systemImage: "square.and.pencil", action: newChat)
                        .labelStyle(.iconOnly).glassButtonStyle()
                        .accessibilityIdentifier("conversation.new")
                }
                .padding(.horizontal, Design.padding)
            }
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
        if sizeClass == .compact {
            setDrawer(!drawer)
        } else {
            sidebar = sidebar == .all ? .detailOnly : .all
        }
    }

    private func setDrawer(_ open: Bool) {
        withAnimation(reduceMotion ? nil : .smooth(duration: 0.28)) { drawer = open }
    }
}

struct ThreadListView: View {
    @Environment(AppModel.self) private var app
    @Bindable var threads: ThreadsModel
    var choose: (String) -> Void
    var new: () -> Void
    var close: (() -> Void)?
    @State private var searching = false
    @State private var rename: ThreadSummary?
    @State private var name = ""
    @State private var delete: ThreadSummary?

    var body: some View {
        List {
            if let error = threads.error {
                Text(error)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .accessibilityIdentifier("threads.error")
            }
            ForEach(sections, id: \.label) { section in
                Section(section.label) {
                    ForEach(section.threads) { thread in row(thread) }
                }
            }
            if threads.nextOffset != nil {
                Button("Load more") { Task { await threads.loadMore() } }
                    .task(id: threads.nextOffset) { await threads.loadMore() }
                    .disabled(threads.isLoading)
                    .accessibilityIdentifier("threads.more")
            }
            Section {
                Button("Desktop", systemImage: "display") { app.selectedTab = .desktop }
                    .accessibilityIdentifier("tab.desktop")
                Button("Settings", systemImage: "gearshape") { app.show(nil) }
                    .accessibilityIdentifier("tab.settings")
            }
            .foregroundStyle(.primary)
        }
        .listStyle(.sidebar)
        .searchable(text: $threads.query, isPresented: $searching, placement: .navigationBarDrawer(displayMode: .always), prompt: "Search conversations")
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
        .navigationTitle("Conversations")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if let close {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close", systemImage: "sidebar.left", action: close)
                        .labelStyle(.iconOnly)
                }
            }
            ToolbarItemGroup(placement: .primaryAction) {
                filters
                Button("New Chat", systemImage: "square.and.pencil", action: new)
                    .labelStyle(.iconOnly)
                    .accessibilityIdentifier("threads.new")
            }
        }
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

    private var filters: some View {
        Menu("Filters", systemImage: "line.3.horizontal.decrease") {
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
        }
        .accessibilityIdentifier("threads.filters")
    }

    private func row(_ thread: ThreadSummary) -> some View {
        Button { choose(thread.id) } label: {
            ThreadRow(thread: thread)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityIdentifier("thread.\(thread.id)")
        .swipeActions(edge: .leading, allowsFullSwipe: false) { pin(thread) }
        .swipeActions(allowsFullSwipe: false) { remove(thread); archive(thread) }
        .contextMenu {
            Button("Rename", systemImage: "pencil") {
                name = thread.displayTitle
                rename = thread
            }
            pin(thread)
            archive(thread)
            remove(thread)
        }
    }

    private func pin(_ thread: ThreadSummary) -> some View {
        Button(thread.pinned == true ? "Unpin" : "Pin", systemImage: thread.pinned == true ? "pin.slash" : "pin") {
            Task { await threads.update(thread, .pin(thread.pinned != true)) }
        }
        .tint(.secondary)
    }

    private func archive(_ thread: ThreadSummary) -> some View {
        Button(thread.archived == true ? "Unarchive" : "Archive", systemImage: thread.archived == true ? "tray.and.arrow.up" : "archivebox") {
            Task { await threads.update(thread, .archive(thread.archived != true)) }
        }
        .tint(.secondary)
    }

    private func remove(_ thread: ThreadSummary) -> some View {
        Button("Delete", systemImage: "trash", role: .destructive) { delete = thread }
    }

    private var sections: [(label: String, threads: [ThreadSummary])] {
        let calendar = Calendar.current
        let today = calendar.startOfDay(for: .now)
        func label(_ thread: ThreadSummary) -> String {
            if thread.pinned == true { return "Pinned" }
            guard let date = thread.modifiedDate else { return "Earlier" }
            let days = calendar.dateComponents([.day], from: calendar.startOfDay(for: date), to: today).day ?? 0
            switch days {
            case ...0: return "Today"
            case 1: return "Yesterday"
            case 2...7: return "Previous 7 Days"
            case 8...30: return "Previous 30 Days"
            default: return date.formatted(.dateTime.month(.wide).year())
            }
        }
        let ordered = threads.threads.sorted { left, right in
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
    var thread: ThreadSummary

    var body: some View {
        VStack(alignment: .leading, spacing: Design.small) {
            HStack(alignment: .firstTextBaseline, spacing: Design.gap) {
                if thread.running == true {
                    Circle().fill(Design.accent).frame(width: 6, height: 6)
                        .accessibilityLabel("Running now")
                }
                Text(thread.displayTitle).font(.body).lineLimit(1)
                Spacer(minLength: Design.gap)
                if let date = thread.modifiedDate {
                    Text(date, format: .relative(presentation: .numeric, unitsStyle: .abbreviated))
                        .font(.caption).foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            let directory = thread.workspace.map { ($0 as NSString).lastPathComponent } ?? ""
            let subtitle = [directory, thread.snippet ?? ""].filter { !$0.isEmpty }.joined(separator: " · ")
            if !subtitle.isEmpty {
                Text(subtitle).font(.footnote).foregroundStyle(.secondary).lineLimit(1)
            }
        }
        .accessibilityElement(children: .combine)
        .padding(.vertical, Design.small)
    }
}
