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
    var selection: ThreadSummary.ID?

    private let app: AppModel
    private var conversations: [String: ConversationModel] = [:]
    private var pinned: [String: ThreadSummary] = [:]

    init(app: AppModel) { self.app = app }

    var countLabel: String {
        let count = total ?? threads.count
        return count == 1 ? "1 conversation" : "\(count) conversations"
    }

    func thread(_ id: String) -> ThreadSummary? { threads.first { $0.id == id } ?? pinned[id] }

    func reload() async {
        guard let api = app.api else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            let page = try await api.threads(offset: 0, query: query, project: project)
            threads = page.threads
            if query.isEmpty, project.isEmpty || projects.isEmpty { projects = page.projects ?? projects }
            total = page.totalThreads
            nextOffset = page.nextOffset
            error = nil
            loaded = true
        } catch {
            fail(error)
        }
    }

    func loadMore() async {
        guard let api = app.api, let offset = nextOffset, !isLoading else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            let page = try await api.threads(offset: offset, query: query, project: project)
            let known = Set(threads.map(\.id))
            threads += page.threads.filter { !known.contains($0.id) }
            nextOffset = page.nextOffset
        } catch {
            fail(error)
        }
    }

    /// `POST /api/new` in the selected project (or the host workspace), then opens it.
    func newThread() async {
        guard let api = app.api else { return }
        do {
            let created = try await api.newThread(workspace: project)
            let summary = ThreadSummary(session: created.id, title: nil, workspace: project.isEmpty ? app.status?.workspace : project,
                                        modified: Date.now.timeIntervalSince1970)
            pinned[created.id] = summary
            threads.insert(summary, at: 0)
            total = total.map { $0 + 1 }
            selection = created.id
        } catch {
            fail(error)
        }
    }

    /// `POST /api/close`: releases the host-side connection; the thread stays in history.
    func close(_ id: String) async {
        guard let api = app.api, let thread = thread(id) else { return }
        conversations[id]?.stop()
        conversations[id] = nil
        do {
            _ = try await api.close(id, sourceRef: thread.sourceRef)
            if selection == id { selection = nil }
        } catch {
            fail(error)
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
        if error.isUnauthorized { app.handle(error) } else { self.error = error.localizedDescription }
    }
}
