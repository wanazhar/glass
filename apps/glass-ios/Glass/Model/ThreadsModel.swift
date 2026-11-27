import Foundation
import GlassAPI
import GlassModels
import GlassTransport
import Observation

extension ThreadSummary {
    var lensTitle: String { lensTitle(firstRequest: nil) }

    /// The host's title when it says something; otherwise the first real request, a meaningful snippet line,
    /// or a plain label. Greetings and commands never become the title.
    func lensTitle(firstRequest: String?) -> String {
        let original = title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        if hasMeaningfulTitle { return original }
        if let request = firstRequest.flatMap(Self.excerpt) { return request }
        let meaningful = snippet?.split(whereSeparator: \.isNewline).lazy
            .compactMap { Self.excerpt(String($0)) }.first
        if let meaningful { return meaningful }
        if original.hasPrefix("/"), let command = original.split(whereSeparator: \.isWhitespace).first, command.count > 1 {
            return "Conversation about \(command.dropFirst())"
        }
        return "New conversation"
    }

    var hasMeaningfulTitle: Bool { !Self.isGeneric(title?.trimmingCharacters(in: .whitespacesAndNewlines) ?? "") }

    /// Up to eight words of a request, or nil when it is only a greeting or a command.
    static func excerpt(_ text: String) -> String? {
        guard let line = text.split(whereSeparator: \.isNewline).lazy.map({ $0.trimmingCharacters(in: .whitespaces) })
            .first(where: { !isGeneric($0) }) else { return nil }
        return line.split(whereSeparator: \.isWhitespace).prefix(8).joined(separator: " ")
    }

    private static let greetings: Set<String> = ["hi", "hey", "hello", "hiya", "yo", "howdy", "greetings", "sup",
                                                 "morning", "good morning", "good afternoon", "good evening", "hai", "helo"]
    private static let placeholders: Set<String> = ["untitled conversation", "new conversation", "opening greeting",
                                                    "greeting", "new chat", "untitled"]

    static func isGeneric(_ text: String) -> Bool {
        let plain = text.lowercased().trimmingCharacters(in: .punctuationCharacters.union(.whitespaces).union(.symbols))
        if plain.isEmpty || text.hasPrefix("/") || placeholders.contains(plain) || greetings.contains(plain) { return true }
        let words = plain.split(whereSeparator: { !$0.isLetter })
        guard let first = words.first, words.count <= 3 else { return false }
        let rest = words.dropFirst().map { $0.lowercased() }
        return greetings.contains(String(first)) && rest.allSatisfy { ["there", "glass", "all", "again", "you"].contains($0) }
    }
}

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
    private var retitled: Set<String> = []
    private var clearError: Task<Void, Never>?

    init(app: AppModel) { self.app = app }

    func thread(_ id: String) -> ThreadSummary? { knownThreads[id] }

    /// Sidebar titles by thread. Generated titles that would repeat get a number, so each row reads differently.
    func titles() -> [String: String] {
        var result: [String: String] = [:]
        var seen: [String: Int] = [:]
        for thread in threads.reversed() {
            var name = thread.lensTitle(firstRequest: conversations[thread.id]?.firstRequest)
            if !thread.hasMeaningfulTitle {
                let count = (seen[name] ?? 0) + 1
                seen[name] = count
                if count > 1 { name += " \(count)" }
            }
            result[thread.id] = name
        }
        return result
    }

    func title(for id: String) -> String? {
        if let title = titles()[id] { return title }
        return knownThreads[id].map { $0.lensTitle(firstRequest: conversations[id]?.firstRequest) }
    }

    /// Names a thread after its first real request once it has one, while its host title is still generic.
    func retitle(_ id: String, from request: String) async {
        guard let thread = knownThreads[id], !thread.hasMeaningfulTitle, let name = ThreadSummary.excerpt(request),
              let api = app.api, !retitled.contains(id) else { return }
        retitled.insert(id)
        do {
            try await api.updateThread(thread, .rename(name))
            knownThreads[id]?.title = name
            if let index = threads.firstIndex(where: { $0.id == id }) { threads[index].title = name }
        } catch {
            retitled.remove(id)
        }
    }

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
