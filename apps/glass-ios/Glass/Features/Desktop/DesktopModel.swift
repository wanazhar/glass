import GlassAPI
import GlassModels
import SwiftUI
import UIKit

/// Maps touches on the zoomed, letterboxed desktop picture to 0…1 desktop coordinates and back.
struct DesktopViewport: Equatable {
    var container: CGSize
    var image: CGSize
    var zoom: CGFloat = 1
    var pan: CGSize = .zero

    var pictureRect: CGRect {
        guard image.width > 0, image.height > 0, container.width > 0, container.height > 0 else { return .zero }
        let scale = min(container.width / image.width, container.height / image.height)
        let size = CGSize(width: image.width * scale, height: image.height * scale)
        return CGRect(x: (container.width - size.width) / 2, y: (container.height - size.height) / 2,
                      width: size.width, height: size.height)
    }

    private var center: CGPoint { CGPoint(x: container.width / 2, y: container.height / 2) }

    /// Desktop position under `point`, or nil when it falls outside the picture (unless `clamped`).
    func desktopPoint(at point: CGPoint, clamped: Bool = false) -> CGPoint? {
        let rect = pictureRect
        guard rect.width > 0, zoom > 0 else { return nil }
        let x = ((point.x - center.x - pan.width) / zoom + center.x - rect.minX) / rect.width
        let y = ((point.y - center.y - pan.height) / zoom + center.y - rect.minY) / rect.height
        if clamped { return CGPoint(x: min(max(x, 0), 1), y: min(max(y, 0), 1)) }
        guard (0...1).contains(x), (0...1).contains(y) else { return nil }
        return CGPoint(x: x, y: y)
    }

    /// Screen position of a desktop position.
    func screenPoint(for desktop: CGPoint) -> CGPoint {
        let rect = pictureRect
        let content = CGPoint(x: rect.minX + desktop.x * rect.width, y: rect.minY + desktop.y * rect.height)
        return CGPoint(x: (content.x - center.x) * zoom + center.x + pan.width,
                       y: (content.y - center.y) * zoom + center.y + pan.height)
    }

    /// Keeps panning within the zoomed picture.
    func clampedPan(_ proposed: CGSize) -> CGSize {
        let rect = pictureRect
        let limitX = max(0, (rect.width * zoom - container.width) / 2 + max(0, container.width - rect.width) / 2 * zoom)
        let limitY = max(0, (rect.height * zoom - container.height) / 2 + max(0, container.height - rect.height) / 2 * zoom)
        return CGSize(width: min(max(proposed.width, -limitX), limitX), height: min(max(proposed.height, -limitY), limitY))
    }
}

/// Live view and control of the host's managed desktop.
@MainActor @Observable
final class DesktopModel {
    enum Picture: Equatable { case idle, connecting, live, stills }

    private(set) var status: DesktopStatus?
    private(set) var frame: UIImage?
    private(set) var picture: Picture = .idle
    private(set) var busy: String?
    var error: String?
    var cursor = CGPoint(x: 0.5, y: 0.5)
    var control = false
    var alt = false

    private var api: GlassClient?
    /// The live picture only runs while the Desktop screen is showing.
    var isVisible = false
    private var frameTask: Task<Void, Never>?
    private var pictureSession: String?
    /// Inputs waiting to be sent, in order. A newer pointer move replaces a waiting one, so a slow
    /// network never replays a backlog of stale moves.
    private var pendingInputs: [DesktopInput] = []
    private var sending = false
    private var decoding = false
    private var latestJPEG: (Data, DesktopFrame?)?
    /// Where the time goes, for the connection details readout.
    private(set) var stats = DesktopStats()
    private var lastMove = ContinuousClock.now
    private var pendingScroll = CGSize.zero

    func attach(_ api: GlassClient?) { self.api = api }

    func watch() async {
        while !Task.isCancelled {
            await refresh()
            try? await Task.sleep(for: .seconds(3))
        }
    }

    func refresh() async {
        guard let api else { return }
        let previous = status?.lease
        do {
            status = try await api.desktopStatus()
        } catch {
            self.error = error.localizedDescription
            return
        }
        if let status, let previous, previous != status.lease {
            AccessibilityNotification.Announcement(status.controlSummary).post()
        }
        if status?.isRunning == true {
            guard isVisible else { return }
            if frameTask != nil, pictureSession != status?.sessionId { reconnect() } else { startPicture() }
        } else {
            stopPicture()
            frame = nil
        }
    }

    func start(_ option: DesktopOption?) async {
        await run("Starting desktop…") { try await $0.startDesktop(option) }
    }

    func stop() async {
        stopPicture()
        await run("Stopping desktop…") { try await $0.stopDesktop() }
        frame = nil
    }

    func deleteSavedDesktop() async {
        await run("Deleting saved desktop…") { try await $0.deleteDesktopProfile() }
    }

    func takeControl() async { await setLease(.takeControl, "Taking control…") }
    func giveBackControl() async { await setLease(.returnToAgent, "Giving back control…") }
    func viewOnly() async { await setLease(.readOnly, "Switching to view only…") }

    private func setLease(_ action: DesktopLeaseAction, _ label: String) async {
        let session = status?.sessionId
        await run(label) { _ = try await $0.setDesktopLease(action, session: session) }
        control = false
        alt = false
    }

    private func run(_ label: String, _ work: (GlassClient) async throws -> Void) async {
        guard let api, busy == nil else { return }
        busy = label
        do {
            try await work(api)
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
        await refresh()
        busy = nil
    }

    // MARK: Picture

    func reconnect() {
        stopPicture()
        guard isVisible else { return }
        startPicture()
    }

    func stopPicture() {
        frameTask?.cancel()
        frameTask = nil
        picture = .idle
    }

    private func startPicture() {
        guard frameTask == nil, let api else { return }
        pictureSession = status?.sessionId
        frameTask = Task { [weak self] in
            var failures = 0
            while !Task.isCancelled {
                self?.picture = .connecting
                // The pushed stream is the lowest-latency path: each frame costs one trip, not a request
                // and a response. Changed-tile updates are the fallback when the stream can't connect.
                if failures >= 2, let session = self?.status?.sessionId, self?.status?.supportsRegionUpdates == true, failures < 4 {
                    do {
                        try await self?.followRegions(api, session: session)
                    } catch {
                        failures += 1
                    }
                    continue
                }
                do {
                    let frames = try await api.desktopStream(width: 1280)
                    self?.picture = .live
                    failures = 0
                    for try await frame in frames { self?.show(frame) }
                } catch {
                    failures += 1
                }
                guard !Task.isCancelled else { return }
                self?.picture = .stills
                for _ in 0..<(failures > 3 ? 20 : 4) {
                    guard !Task.isCancelled else { return }
                    if let still = try? await api.desktopFrame() { self?.show(still) }
                    try? await Task.sleep(for: .milliseconds(500))
                }
            }
        }
    }

    private func followRegions(_ api: GlassClient, session: String) async throws {
        let canvas = DesktopCanvas()
        var after = 0
        while !Task.isCancelled {
            if let update = try await api.desktopUpdates(session: session, after: after) {
                try update.validate(session: session, after: after, hasBase: canvas.hasBase)
                if let image = try canvas.apply(update) { frame = image }
                after = update.frameSeq
            }
            picture = .live
        }
    }

    /// Decodes off the main thread with the system's hardware-backed JPEG path, always the newest
    /// frame: one that arrives mid-decode replaces any frame still waiting.
    private func show(_ frame: DesktopFrame) {
        stats.received(frame)
        latestJPEG = (frame.jpeg, frame)
        guard !decoding else { return }
        decodeNext()
    }

    private func decodeNext() {
        guard let (jpeg, _) = latestJPEG else { return }
        latestJPEG = nil
        decoding = true
        Task { [weak self] in
            let started = ContinuousClock.now
            let image = await Task.detached(priority: .userInitiated) {
                UIImage(data: jpeg)?.preparingForDisplay()
            }.value
            guard let self else { return }
            self.decoding = false
            if let image {
                self.frame = image
                self.stats.decoded(in: ContinuousClock.now - started)
            }
            self.decodeNext()
        }
    }

    // MARK: Input

    func send(_ input: DesktopInput) {
        if case .move = input, case .move? = pendingInputs.last {
            pendingInputs[pendingInputs.count - 1] = input
        } else {
            pendingInputs.append(input)
        }
        guard !sending else { return }
        sending = true
        Task { [weak self] in await self?.drainInputs() }
    }

    private func drainInputs() async {
        defer { sending = false }
        while !pendingInputs.isEmpty {
            let input = pendingInputs.removeFirst()
            guard let api else { continue }
            let started = ContinuousClock.now
            do {
                try await api.sendDesktopInput(input)
                stats.inputSent(in: ContinuousClock.now - started)
            } catch {
                self.error = error.localizedDescription
            }
        }
    }

    /// Pointer moves are coalesced to about 30 a second; `force` always sends.
    func move(to point: CGPoint, force: Bool = false) {
        cursor = point
        let now = ContinuousClock.now
        guard force || now - lastMove >= .milliseconds(33) else { return }
        lastMove = now
        send(.move(x: point.x, y: point.y))
    }

    func scroll(at point: CGPoint, by delta: CGSize) {
        pendingScroll.width += delta.width
        pendingScroll.height += delta.height
        guard abs(pendingScroll.width) >= 20 || abs(pendingScroll.height) >= 20 else { return }
        send(.scroll(x: point.x, y: point.y, deltaX: pendingScroll.width, deltaY: pendingScroll.height))
        pendingScroll = .zero
    }

    func tapKey(_ key: String) {
        let modifiers = (control ? ["Control"] : []) + (alt ? ["Alt"] : [])
        if modifiers.isEmpty {
            send(.keyTap(key))
        } else {
            send(.chord(modifiers + [key]))
            control = false
            alt = false
        }
    }

    func type(_ text: String) {
        guard !text.isEmpty else { return }
        if (control || alt), text.count == 1 {
            tapKey(text.uppercased())
        } else {
            send(.text(text))
        }
    }

}

/// Rolling desktop timings: what the host spends (frame age at send, JPEG encode), what the network
/// carries (frame rate and size, input round trip) and what this iPhone spends decoding.
struct DesktopStats: Equatable {
    private(set) var framesPerSecond: Double = 0
    private(set) var frameKilobytes: Double = 0
    private(set) var hostAgeMs: Double?
    private(set) var encodeMs: Double?
    private(set) var decodeMs: Double?
    private(set) var inputMs: Double?
    private var lastArrival: ContinuousClock.Instant?

    private static func blend(_ old: Double?, _ new: Double) -> Double { old.map { $0 * 0.8 + new * 0.2 } ?? new }
    private static func ms(_ duration: Duration) -> Double {
        Double(duration.components.seconds) * 1000 + Double(duration.components.attoseconds) / 1e15
    }

    mutating func received(_ frame: DesktopFrame) {
        let now = ContinuousClock.now
        if let lastArrival {
            let gap = Self.ms(now - lastArrival)
            if gap > 0 { framesPerSecond = Self.blend(framesPerSecond == 0 ? nil : framesPerSecond, 1000 / gap) }
        }
        lastArrival = now
        frameKilobytes = Self.blend(frameKilobytes == 0 ? nil : frameKilobytes, Double(frame.jpeg.count) / 1024)
        if let age = frame.ageMs { hostAgeMs = Self.blend(hostAgeMs, Double(age)) }
        if let encode = frame.encodeMs { encodeMs = Self.blend(encodeMs, Double(encode)) }
    }

    mutating func decoded(in duration: Duration) { decodeMs = Self.blend(decodeMs, Self.ms(duration)) }
    mutating func inputSent(in duration: Duration) { inputMs = Self.blend(inputMs, Self.ms(duration)) }

    var summary: String {
        func value(_ number: Double?) -> String { number.map { String(Int($0.rounded())) } ?? "–" }
        return "\(value(framesPerSecond)) fps · \(value(frameKilobytes)) KB a frame\n"
            + "Host \(value(hostAgeMs)) ms old when sent (encode \(value(encodeMs)) ms)\n"
            + "Decode \(value(decodeMs)) ms · Input round trip \(value(inputMs)) ms"
    }
}
