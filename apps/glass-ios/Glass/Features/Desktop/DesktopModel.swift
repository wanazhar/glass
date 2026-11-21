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
    private var frameTask: Task<Void, Never>?
    private var pictureSession: String?
    private var inputs: AsyncStream<DesktopInput>.Continuation?
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
                if let session = self?.status?.sessionId, self?.status?.supportsRegionUpdates == true, failures < 2 {
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
                    for try await frame in frames { self?.show(frame.jpeg) }
                } catch {
                    failures += 1
                }
                guard !Task.isCancelled else { return }
                self?.picture = .stills
                for _ in 0..<(failures > 3 ? 20 : 4) {
                    guard !Task.isCancelled else { return }
                    if let still = try? await api.desktopFrame() { self?.show(still.jpeg) }
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

    private func show(_ jpeg: Data) {
        if let image = UIImage(data: jpeg) { frame = image }
    }

    // MARK: Input

    func send(_ input: DesktopInput) {
        if inputs == nil { startInputs() }
        inputs?.yield(input)
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

    private func startInputs() {
        let (stream, continuation) = AsyncStream<DesktopInput>.makeStream(bufferingPolicy: .bufferingNewest(128))
        inputs = continuation
        Task { [weak self] in
            for await input in stream {
                guard let api = self?.api else { continue }
                do {
                    try await api.sendDesktopInput(input)
                } catch {
                    self?.error = error.localizedDescription
                }
            }
        }
    }
}
