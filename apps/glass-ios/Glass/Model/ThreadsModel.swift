import Foundation
import GlassAPI
import GlassModels
import GlassTransport
import Observation

/// Sidebar state: paged thread list, search, project scope, and open conversations.
@MainActor @Observable
final class ThreadsModel {
    private(set) var threads: [ThreadSummary] = []
    private(set) var projects: [ProjectCount] = []
    private(set) var total: Int?
    private(set) var nextOffset: Int?
    private(set) var isLoading = false
    private(set) var loaded = false
    var error: String?
    var query = ""
    var project = ""
    var archived = false
    var selection: ThreadSummary.ID?

    private let app: AppModel
    private var conversations: [String: ConversationModel] = [:]
    private var knownThreads: [String: ThreadSummary] = [:]
    private var request = 0
    private var clearError: Task<Void, Never>?

    init(app: AppModel) { self.app = app }

    var countLabel: String {
        let count = total ?? threads.count
        return count == 1 ? "1 thread" : "\(count) threads"
    }

    func thread(_ id: String) -> ThreadSummary? { knownThreads[id] }

    func reload() async {
        guard let api = app.api else { return }
        request += 1
        let revision = request
        isLoading = true
        defer { if request == revision { isLoading = false } }
        do {
            let page = try await api.threads(offset: 0, query: query, project: project, archived: archived)
            guard request == revision, !Task.isCancelled else { return }
            threads = page.threads
            for thread in threads { knownThreads[thread.id] = thread }
            if query.isEmpty, project.isEmpty || projects.isEmpty { projects = page.projects ?? projects }
            total = page.totalThreads
            nextOffset = page.nextOffset
            error = nil
            loaded = true
        } catch {
            if request == revision { fail(error) }
        }
    }

    func loadMore() async {
        guard let api = app.api, let offset = nextOffset, !isLoading else { return }
        let revision = request
        isLoading = true
        defer { if request == revision { isLoading = false } }
        do {
            let page = try await api.threads(offset: offset, query: query, project: project, archived: archived)
            guard request == revision, !Task.isCancelled else { return }
            let known = Set(threads.map(\.id))
            threads += page.threads.filter { !known.contains($0.id) }
            for thread in page.threads { knownThreads[thread.id] = thread }
            nextOffset = page.nextOffset
        } catch {
            if request == revision { fail(error) }
        }
    }

    /// `POST /api/new` in the selected project (or the host workspace), then opens it.
    func newThread(draft: String? = nil, action: ComposerModel.InitialAction? = nil) async {
        guard let api = app.api else { return }
        do {
            let created = try await api.newThread(workspace: project)
            let summary = ThreadSummary(session: created.id, title: nil, workspace: project.isEmpty ? app.status?.workspace : project,
                                        modified: Date.now.timeIntervalSince1970)
            knownThreads[created.id] = summary
            if !archived, query.isEmpty {
                threads.insert(summary, at: 0)
                total = total.map { $0 + 1 }
            }
            if draft != nil || action != nil {
                let composer = conversation(for: summary).composer
                if let draft { composer.text = draft }
                composer.initialAction = action
            }
            selection = created.id
        } catch {
            fail(error)
        }
    }

    func update(_ thread: ThreadSummary, _ change: GlassClient.ThreadChange) async {
        guard let api = app.api else { return }
        do {
            if case .delete = change, conversations[thread.id] != nil || thread.writerActive == true {
                await close(thread.id, reportErrors: false)
            }
            try await api.updateThread(thread, change)
            if case .delete = change { knownThreads.removeValue(forKey: thread.id) }
            if case .rename(let title) = change { knownThreads[thread.id]?.title = title }
            await reload()
        } catch {
            fail(error)
        }
    }

    /// `POST /api/close`: releases the host-side connection; the thread stays in history.
    func close(_ id: String, reportErrors: Bool = true) async {
        guard let api = app.api, let thread = thread(id) else { return }
        conversations[id]?.stop()
        conversations[id] = nil
        do {
            _ = try await api.close(id, sourceRef: thread.sourceRef)
            if selection == id { selection = nil }
        } catch {
            if reportErrors { fail(error) }
        }
    }

    /// One live model per thread so returning to a conversation resumes from its cursor.
    func conversation(for thread: ThreadSummary) -> ConversationModel {
        if let existing = conversations[thread.id] { return existing }
        let model = ConversationModel(app: app, thread: thread)
        conversations[thread.id] = model
        return model
    }

    private func fail(_ error: GlassError) {
        guard error != .cancelled else { return }
        if error.isUnauthorized {
            app.handle(error)
            return
        }
        self.error = error.localizedDescription
        clearError?.cancel()
        clearError = Task {
            try? await Task.sleep(for: .seconds(5))
            if !Task.isCancelled { self.error = nil }
        }
    }
}
