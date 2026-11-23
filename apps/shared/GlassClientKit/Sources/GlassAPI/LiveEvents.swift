import Foundation
import GlassModels
import GlassTransport

/// What the live-event loop reports to the UI.
public enum LiveUpdate: Sendable, Hashable {
    case connected
    case frame(EventFrame, cursor: EventCursor?)
    case reconnecting(attempt: Int, reason: String)
    /// The host revoked this iPhone; the loop stops.
    case unauthorized(String)
}

/// Keeps `/api/events` attached: resumes with `Last-Event-ID` and backs off with jitter between attempts.
public struct LiveEvents: Sendable {
    public typealias Delay = @Sendable (Int) -> Duration
    public static let defaultDelay: Delay = { attempt in
        let base = min(30.0, pow(2.0, Double(max(0, attempt - 1))))
        return .milliseconds(Int(base * Double.random(in: 800...1200)))
    }

    let client: GlassClient
    let thread: String
    let sourceRef: String?
    let delay: Delay

    public init(client: GlassClient, thread: String, sourceRef: String? = nil, delay: @escaping Delay = LiveEvents.defaultDelay) {
        self.client = client
        self.thread = thread
        self.sourceRef = sourceRef
        self.delay = delay
    }

    /// Runs until cancelled or unauthorized. `resume` seeds the first `Last-Event-ID`.
    public func updates(resume: EventCursor? = nil) -> AsyncStream<LiveUpdate> {
        AsyncStream { continuation in
            let task = Task {
                var cursor = resume
                var attempt = 0
                while !Task.isCancelled {
                    do {
                        let frames = try await client.eventConnection(thread, sourceRef: sourceRef, resume: cursor)
                        continuation.yield(.connected)
                        attempt = 0
                        for try await (id, frame) in frames {
                            if let id { cursor = id }
                            continuation.yield(.frame(frame, cursor: id))
                        }
                        attempt += 1
                        continuation.yield(.reconnecting(attempt: attempt, reason: "The host closed the event stream"))
                    } catch let error as GlassError where error.isUnauthorized {
                        continuation.yield(.unauthorized(error.localizedDescription))
                        break
                    } catch is CancellationError {
                        break
                    } catch {
                        if Task.isCancelled { break }
                        attempt += 1
                        let message = (error as? GlassError)?.localizedDescription ?? error.localizedDescription
                        continuation.yield(.reconnecting(attempt: attempt, reason: message))
                    }
                    try? await Task.sleep(for: delay(attempt))
                }
                continuation.finish()
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }
}
