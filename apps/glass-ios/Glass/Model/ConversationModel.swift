import Foundation
import GlassAPI
import GlassModels
import GlassTransport
import Observation
import UIKit

/// One conversation: history windows plus the live `/api/events` stream.
@MainActor @Observable
final class ConversationModel {
    enum Connection: Equatable {
        case idle
        case connecting
        case live
        case reconnecting(String)
        case disconnected(String)
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
    /// Milliseconds from returning to the app until the stream was live again, for the last return.
    private(set) var lastReconnect: Int?
    private var resumeStarted: ContinuousClock.Instant?
    private var backgroundTask: UIBackgroundTaskIdentifier = .invalid
    /// The stream stayed open through the last background stretch because keep-alive kept Glass running.
    private var keptAlive = false
    /// The next frame is the host's catch-up after (re)connecting: events that already happened.
    private var catchingUp = true
    /// When Glass last came back from the background. A socket iOS froze delivers what it missed right
    /// after this, and those replies are already past.
    private var cameBack: ContinuousClock.Instant?
    private var foregroundWatch: Task<Void, Never>?
    var error: String?

    private let app: AppModel
    private var liveTask: Task<Void, Never>?
    private var starting: Task<Void, Never>?
    private var attempt = UUID()
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
        foregroundWatch = Task { [weak self] in
            for await _ in NotificationCenter.default.notifications(named: UIApplication.willEnterForegroundNotification) {
                guard let self else { return }
                // Kept alive, the stream never stopped, so what arrives next is new.
                if !KeepAlive.shared.running { self.cameBack = .now }
            }
        }
    }

    var title: String { thread.displayTitle }

    /// This app holds the session control channel and may send input.
    var canWrite: Bool { timeline.canWrite && connection == .live }

    /// Ask to take control only once the host has said another client holds it, never while that is still unknown.
    var needsControl: Bool { connection == .live && timeline.controlKnown && !timeline.canWrite }

    var connectionMessage: String? {
        switch connection {
        case .disconnected(let reason): reason
        case .reconnecting: "The connection to your host was interrupted."
        case .idle, .connecting, .live: nil
        }
    }

    var hasEarlier: Bool { timeline.hasOlder }

    /// Loads history, attaches the host connection, and follows live events until stopped.
    func start() async {
        if let starting { return await starting.value }
        let id = attempt
        // Owned by the model so a view rebuild cancelling `.task` cannot abort the first load.
        let task = Task { await attach() }
        starting = task
        await task.value
        if attempt == id { starting = nil }
    }

    private func attach() async {
        guard !Task.isCancelled, liveTask == nil, let api = app.api else { return }
        connection = .connecting
        if !loadedHistory {
            do {
                let page = try await api.history(thread.id, sourceRef: thread.sourceRef)
                let initial = await Self.initialTimeline(page)
                guard !Task.isCancelled else { return }
                timeline = initial
                loadedHistory = true
            } catch {
                return fail(error)
            }
        }
        do {
            let opened = try await api.open(thread.id, sourceRef: thread.sourceRef)
            guard !Task.isCancelled else { return }
            originLabel = opened.originLabel ?? originLabel
        } catch {
            return fail(error)
        }
        Task {
            await composer.refreshContext()
            await composer.refreshChecklist()
        }
        let live = LiveEvents(client: api, thread: thread.id, sourceRef: thread.sourceRef)
        let resume = cursor
        liveTask = Task { [weak self] in
            for await update in live.updates(resume: resume) {
                guard !Task.isCancelled, let self else { return }
                await self.receive(update)
            }
        }
        KeepAlive.shared.hold(keepAliveKey) { [weak self] in self?.stop() }
    }

    private var keepAliveKey: String { "conversation:" + thread.id }

    /// Leaving the app: a turn in progress keeps its stream for as long as iOS allows, so "Reply ready"
    /// can still arrive; otherwise the stream closes now instead of dying silently while suspended.
    func suspend() {
        composer.flushDraft()
        guard liveTask != nil else { return }
        if KeepAlive.shared.running {
            keptAlive = true
            return
        }
        guard timeline.isActive else { return stop() }
        backgroundTask = UIApplication.shared.beginBackgroundTask(withName: "Glass reply") { [weak self] in
            MainActor.assumeIsolated {
                self?.stop()
                self?.endBackgroundTask()
            }
        }
    }

    /// Back in the app: the cached chat is already on screen; open a fresh stream right away from the
    /// last event id, rather than waiting for a socket iOS suspended to time out.
    func resumeNow() async {
        endBackgroundTask()
        defer { keptAlive = false }
        // Glass stayed running and the stream stayed open: nothing to reconnect.
        if keptAlive, KeepAlive.shared.running, liveTask != nil {
            lastReconnect = 0
            drawPendingFrames()
            return
        }
        guard loadedHistory else { return await start() }
        resumeStarted = .now
        stop()
        await start()
    }

    func reconnect() async {
        stop()
        await start()
    }

    private func endBackgroundTask() {
        guard backgroundTask != .invalid else { return }
        UIApplication.shared.endBackgroundTask(backgroundTask)
        backgroundTask = .invalid
    }

    func stop() {
        attempt = UUID()
        starting?.cancel()
        starting = nil
        KeepAlive.shared.release(keepAliveKey)
        liveTask?.cancel()
        liveTask = nil
        drawPendingFrames()
        connection = .idle
    }

    func loadOlder() async {
        guard let api = app.api, let before = timeline.olderBefore, before > 0, !loadingOlder else { return }
        loadingOlder = true
        defer { loadingOlder = false }
        do {
            let page = try await api.history(thread.id, sourceRef: thread.sourceRef, before: before)
            timeline.prependOlder(page)
        } catch {
            fail(error)
        }
    }

    private nonisolated static func initialTimeline(_ page: HistoryPage) async -> Timeline {
        var timeline = Timeline()
        timeline.replaceHistory(page)
        return timeline
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
            catchingUp = true
            connection = .live
            error = nil
            if let started = resumeStarted {
                let elapsed = ContinuousClock.now - started
                lastReconnect = Int(elapsed.components.seconds * 1000 + elapsed.components.attoseconds / 1_000_000_000_000_000)
                resumeStarted = nil
            }
        case .frame(let frame, let id):
            let resumed = cursor != nil
            let replayed = catchingUp || cameBack.map { ContinuousClock.now - $0 < .seconds(3) } == true
            catchingUp = false
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
                let events = frame.live + frame.events
                for event in events { composer.observe(event) }
                if let id { alert(for: events, event: id.description, replayed: replayed) }
                if events.contains(.done), backgroundTask != .invalid {
                    stop()
                    endBackgroundTask()
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
                    guard !Task.isCancelled else { return }
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
            KeepAlive.shared.release(keepAliveKey)
            app.handle(.unauthorized(message))
        }
    }

    /// One alert per frame, for the most recent reply outcome in it.
    private func alert(for events: [GlassEvent], event: String, replayed: Bool) {
        let title = events.reversed().lazy.compactMap { event -> String? in
            switch event {
            case .done: "Reply ready"
            case .interaction: "Glass needs your answer"
            case .error(_, let settled, _, _) where settled: "Glass stopped with a problem"
            default: nil
            }
        }.first
        guard let title else { return }
        Attention.notify(title, conversation: thread.displayTitle, thread: thread.id, event: event, replayed: replayed)
    }

    private func fail(_ error: GlassError) {
        guard !Task.isCancelled, error != .cancelled else { return }
        if error.isUnauthorized { app.handle(error) } else {
            if connection == .connecting {
                connection = .disconnected(error.localizedDescription)
            } else {
                self.error = error.localizedDescription
            }
        }
    }
}
