import Foundation

/// Splits a byte stream into lines the way a real HTTP body delivers them.
///
/// Foundation's `AsyncLineSequence` never yields empty lines, but SSE framing
/// needs them — a blank line is what completes an event frame. This splitter
/// takes raw bytes and yields every line, including empty ones, across `\n`,
/// `\r\n`, and a bare `\r`, with lines allowed to split over chunk boundaries.
/// Delimiters are ASCII, so decoding completed lines as UTF-8 is safe.
public struct MarbleSSELineSplitter: Sendable {
    private var pending: [UInt8] = []
    /// Set after a `\r` so the `\n` of `\r\n` does not emit a second empty line.
    private var skipNextLineFeed = false

    public init() {}

    /// Feed one byte; returns any lines it completed, in order.
    public mutating func feed(_ byte: UInt8) -> [String] {
        if byte == UInt8(ascii: "\n") {
            if skipNextLineFeed {
                skipNextLineFeed = false
                return []
            }
            return [takeLine()]
        }
        skipNextLineFeed = false
        if byte == UInt8(ascii: "\r") {
            skipNextLineFeed = true
            return [takeLine()]
        }
        pending.append(byte)
        return []
    }

    /// Feed a chunk; returns every line the chunk completes.
    public mutating func feed<S: Sequence>(_ bytes: S) -> [String] where S.Element == UInt8 {
        var lines: [String] = []
        for byte in bytes {
            lines.append(contentsOf: feed(byte))
        }
        return lines
    }

    /// Whatever was buffered when the stream ended; nil when empty.
    public mutating func finish() -> String? {
        skipNextLineFeed = false
        guard !pending.isEmpty else { return nil }
        return takeLine()
    }

    private mutating func takeLine() -> String {
        let line = String(decoding: pending, as: UTF8.self)
        pending.removeAll(keepingCapacity: true)
        return line
    }
}

/// The single source of truth for the connection status shown everywhere.
/// A pure reducer: every surface — refresh results, stream open, stream
/// events, stream endings, reconnect attempts, sign-out — feeds the same
/// inputs, so the Chats strip, the chat header, and Settings can never
/// disagree about whether Marble is connected.
public struct MarbleConnectionStatus: Equatable, Sendable {
    public enum State: Equatable, Sendable {
        case connecting
        case connected
        case offline(reason: String)
        case signedOut
    }

    public private(set) var state: State = .connecting
    /// True while one reconnect is in flight after a working connection
    /// dropped. The status stays Connected during that attempt and only goes
    /// Offline if it — or a refresh — fails.
    private var probing = false
    /// True while the app is in the background, where iOS tears connections
    /// down; a stream ending then says nothing about the host.
    public private(set) var suspended = false
    /// Set on returning to the app: the first failed attempt retries instead
    /// of showing Offline, because the network is often still waking up.
    private var resumeGrace = false

    public init() {}

    public var label: String {
        switch state {
        case .connected: "Connected"
        case .connecting: "Connecting"
        case .offline: "Offline"
        case .signedOut: "Signed out"
        }
    }

    public var isConnected: Bool {
        if case .connected = state { return true }
        return false
    }

    public var isOffline: Bool {
        if case .offline = state { return true }
        return false
    }

    public var offlineReason: String? {
        if case let .offline(reason) = state { return reason }
        return nil
    }

    /// A state poll succeeded: that alone proves the host answers.
    public mutating func refreshSucceeded() {
        state = .connected
        probing = false
        resumeGrace = false
    }

    /// A state poll failed: the clearest possible "the host is not there".
    /// Returns false when the failure came right after returning to the app
    /// and the label was kept, so the caller retries once before Offline.
    @discardableResult
    public mutating func refreshFailed(_ reason: String) -> Bool {
        if resumeGrace {
            resumeGrace = false
            return false
        }
        state = .offline(reason: reason)
        probing = false
        return true
    }

    /// The app went to the background.
    public mutating func suspend() {
        suspended = true
    }

    /// The app is active again and is reconnecting. The label stays as it
    /// was, or reads Connecting after Offline, and one failure is forgiven.
    public mutating func resume() {
        suspended = false
        resumeGrace = true
        probing = false
        if case .offline = state { state = .connecting }
    }

    /// `/api/events` answered 200: the stream is up before any event arrives.
    public mutating func streamOpened() {
        state = .connected
        probing = false
        resumeGrace = false
    }

    /// An event arrived — including keepalive frames the parser drops; the
    /// bytes themselves are the liveness proof.
    public mutating func streamEvent() {
        state = .connected
        probing = false
    }

    /// The stream iteration ended or threw. Right after a working connection
    /// this is not yet Offline: one immediate reconnect is attempted first.
    /// A second failure — or a drop from a stream that never opened — is.
    public mutating func streamEnded(_ reason: String?) {
        if suspended { return }
        if resumeGrace {
            resumeGrace = false
            return
        }
        switch state {
        case .connected where probing:
            state = .offline(reason: reason ?? "The connection dropped")
            probing = false
        case .connected:
            probing = true
        case .connecting:
            state = .offline(reason: reason ?? "The connection dropped")
        default:
            break
        }
    }

    /// A reconnect attempt is starting. From Connected the label does not
    /// downgrade — it arms the probe so the next failure goes Offline.
    /// From Connecting or Offline the attempt changes nothing visible.
    public mutating func reconnectStarted() {
        if case .connected = state {
            probing = true
        }
    }

    public mutating func signedOut() {
        state = .signedOut
        probing = false
        suspended = false
        resumeGrace = false
    }
}

/// Numbers each host connection so a response that started under an earlier
/// host or session is recognised and dropped instead of published.
public struct MarbleConnectionGeneration: Equatable, Sendable {
    public private(set) var current: UInt64 = 0

    public init() {}

    /// Start a new connection; every earlier token stops being current.
    public mutating func advance() {
        current &+= 1
    }

    public func isCurrent(_ token: UInt64) -> Bool {
        token == current
    }
}
