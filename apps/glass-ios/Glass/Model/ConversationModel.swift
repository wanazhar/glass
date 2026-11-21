import Foundation
import GlassAPI
import GlassModels
import GlassTransport
import Observation

/// One conversation: history windows plus the live `/api/events` stream.
@MainActor @Observable
final class ConversationModel {
    enum Connection: Equatable {
        case idle
        case connecting
        case live
        case reconnecting(String)
        case readOnly
    }

    let thread: ThreadSummary
    let composer: ComposerModel
    private(set) var timeline = Timeline()
    private(set) var connection: Connection = .idle
    private(set) var originLabel: String?
    private(set) var loadedHistory = false
    private(set) var loadingOlder = false
    private(set) var cursor: EventCursor?
    private(set) var resets = 0
    var error: String?

    private let app: AppModel
    private var liveTask: Task<Void, Never>?
    private var starting: Task<Void, Never>?
    /// Incremental frames waiting to be drawn. A fast stream sends far more frames than the timeline can
    /// re-render, so they are applied together at most every `frameInterval`.
    private var pendingFrames: [EventFrame] = []
    private var drawTask: Task<Void, Never>?
    private static let frameInterval = Duration.milliseconds(100)

    init(app: AppModel, thread: ThreadSummary) {
        self.app = app
        self.thread = thread
        composer = ComposerModel(app: app, thread: thread)
        originLabel = thread.originLabel
    }

    var title: String { thread.displayTitle }

    /// This app holds the session control channel and may send input.
    var canWrite: Bool { timeline.canWrite && connection == .live }

    /// Loads history, attaches the host connection, and follows live events until stopped.
    func start() async {
        if let starting { return await starting.value }
        // Owned by the model so a view rebuild cancelling `.task` cannot abort the first load.
        let task = Task { await attach() }
        starting = task
        await task.value
        starting = nil
    }

    private func attach() async {
        guard liveTask == nil, let api = app.api else { return }
        connection = .connecting
        if !loadedHistory {
            do {
                timeline.replaceHistory(try await api.history(thread.id, sourceRef: thread.sourceRef))
                loadedHistory = true
            } catch {
                return fail(error)
            }
        }
        do {
            let opened = try await api.open(thread.id, sourceRef: thread.sourceRef)
            originLabel = opened.originLabel ?? originLabel
        } catch {
            if error.isUnauthorized { return fail(error) }
            // Archived or foreign threads still show their history.
            connection = .readOnly
            self.error = error.localizedDescription
            return
        }
        Task {
            await composer.refreshContext()
            await composer.refreshChecklist()
        }
        let live = LiveEvents(client: api, thread: thread.id, sourceRef: thread.sourceRef)
        let resume = cursor
        liveTask = Task { [weak self] in
            for await update in live.updates(resume: resume) {
                guard let self else { return }
                await self.receive(update)
            }
        }
    }

    func stop() {
        liveTask?.cancel()
        liveTask = nil
        drawPendingFrames()
        if connection != .readOnly { connection = .idle }
    }

    func loadOlder() async {
        guard let api = app.api, let before = timeline.olderBefore, before > 0, !loadingOlder else { return }
        loadingOlder = true
        defer { loadingOlder = false }
        do {
            timeline.prependOlder(try await api.history(thread.id, sourceRef: thread.sourceRef, before: before))
        } catch {
            fail(error)
        }
    }

    func dismissNotice() { timeline.dismissNotice() }

    private func drawPendingFrames() {
        drawTask?.cancel()
        drawTask = nil
        guard !pendingFrames.isEmpty else { return }
        var next = timeline
        for frame in pendingFrames { next.apply(frame) }
        pendingFrames.removeAll()
        timeline = next
    }

    private func receive(_ update: LiveUpdate) async {
        switch update {
        case .connected:
            connection = .live
            error = nil
        case .frame(let frame, let id):
            let resumed = cursor != nil
            if let id { cursor = id }
            if !frame.reset {
                pendingFrames.append(frame)
                if drawTask == nil {
                    drawTask = Task { [weak self] in
                        try? await Task.sleep(for: Self.frameInterval)
                        guard !Task.isCancelled else { return }
                        self?.drawPendingFrames()
                    }
                }
                for event in frame.live + frame.events {
                    composer.observe(event)
                    alert(for: event)
                }
                return
            }
            drawPendingFrames()
            if !resumed {
                // A fresh stream always opens with a snapshot of the history just loaded.
                timeline.applySnapshot(frame)
            } else {
                guard let api = app.api else { return }
                do {
                    let history = try await api.history(thread.id, sourceRef: thread.sourceRef)
                    timeline.reset(history: history, frame: frame)
                    resets += 1
                } catch {
                    fail(error)
                }
            }
            for event in frame.live + frame.events { composer.observe(event) }
        case .reconnecting(_, let reason):
            connection = .reconnecting(reason)
        case .unauthorized(let message):
            liveTask = nil
            app.handle(.unauthorized(message))
        }
    }

    private func alert(for event: GlassEvent) {
        let title: String? = switch event {
        case .done: "Reply ready"
        case .interaction: "Glass needs your answer"
        case .error(_, let settled, _, _) where settled: "Glass stopped with a problem"
        default: nil
        }
        if let title { Attention.notify(title, conversation: thread.displayTitle, id: thread.id) }
    }

    private func fail(_ error: GlassError) {
        if error.isUnauthorized { app.handle(error) } else {
            self.error = error.localizedDescription
            if connection == .connecting { connection = .idle }
        }
    }
}
