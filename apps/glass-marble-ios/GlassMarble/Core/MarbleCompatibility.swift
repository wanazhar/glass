import Foundation

/// Version contract between this app and the host it connects to.
enum MarbleCompatibility {
    /// Oldest host build this app knows how to talk to.
    static let minimumHostVersion = "0.3.14"

    /// Compare two `major.minor.patch` strings (suffixes like `-dirty` ignored).
    /// Returns a negative, zero, or positive number like `compare`.
    static func compare(_ a: String, _ b: String) -> Int {
        let parts = { (value: String) -> [Int] in
            value.split(separator: "-").first.map { String($0) }?
                .split(separator: ".")
                .map { Int($0) ?? 0 } ?? [0]
        }
        let left = parts(a), right = parts(b)
        for index in 0..<max(left.count, right.count) {
            let l = index < left.count ? left[index] : 0
            let r = index < right.count ? right[index] : 0
            if l != r { return l < r ? -1 : 1 }
        }
        return 0
    }

    static func isSupported(_ hostVersion: String) -> Bool {
        compare(hostVersion, minimumHostVersion) >= 0
    }
}

/// What onboarding concluded about a host, shown as one clear sentence each.
enum MarbleOnboarding: Equatable {
    case ready(String)
    case unreachable(String)
    case badToken
    case tokenRequired
    case versionMismatch(found: String, required: String)
    case unhealthy(String)
    case notMarble

    var message: String {
        switch self {
        case .ready(let version):
            return "Connected to Marble \(version)."
        case .unreachable(let detail):
            return detail
        case .badToken:
            return "The host rejected that token. Check the token file on the host machine."
        case .tokenRequired:
            return "This host requires an access token — paste it above."
        case .versionMismatch(let found, let required):
            return "Host is Marble \(found); this app needs \(required) or newer. Update the host."
        case .unhealthy(let detail):
            return "Marble answered but is not healthy: \(detail)"
        case .notMarble:
            return "Something answered there, but it is not a Marble host."
        }
    }

    var isReady: Bool {
        if case .ready = self { return true }
        return false
    }
}

/// Combine the outcomes of `GET /healthz` and `GET /api/state?lean=1` into the
/// one onboarding verdict a person should act on.
enum MarbleOnboardingEvaluator {
    enum StateOutcome: Equatable {
        case ok
        case unauthorized
        case failed(String)
    }

    static func evaluate(
        urlTried: String,
        health: Result<MarbleHealth, MarbleClientError>,
        state: StateOutcome,
        hadToken: Bool
    ) -> MarbleOnboarding {
        switch health {
        case .failure(let error):
            switch error {
            case .unreachable, .timeout, .offline:
                return .unreachable(
                    "Cannot reach \(urlTried). Is Tailscale on, the phone on the same tailnet, and the port right?"
                )
            case .decoding:
                return .notMarble
            default:
                return .unreachable("\(error.localizedDescription) (\(urlTried))")
            }
        case .success(let health):
            guard let version = health.version, !version.isEmpty else {
                return .notMarble
            }
            if let runtime = health.runtime, runtime.health != "ok" {
                return .unhealthy(
                    runtime.lastError?.isEmpty == false ? runtime.lastError! : "engine not ready"
                )
            }
            if !health.ok {
                return .unhealthy("health check reported not ok")
            }
            guard MarbleCompatibility.isSupported(version) else {
                return .versionMismatch(found: version, required: MarbleCompatibility.minimumHostVersion)
            }
            switch state {
            case .ok:
                return .ready(version)
            case .unauthorized:
                if !health.authRequired { return .unhealthy("state returned 401") }
                return hadToken ? .badToken : .tokenRequired
            case .failed(let detail):
                // A state endpoint that answers 5xx (or garbage) is a host
                // problem, never a token problem.
                return .unhealthy(detail)
            }
        }
    }
}


/// Decides whether an SSE event should surface a local alert, and dedupes the
/// double `settled` the host publishes per turn (engine `{reason:…}` then
/// turn.rs `{cancelled, usage}`).
enum MarbleAttention {
    struct Alert: Equatable {
        var key: String
        var title: String
        var body: String
    }

    /// Returns an alert for an event that should notify, or nil. `seen` holds
    /// keys already notified this session and is mutated on a hit.
    static func alert(
        for event: MarbleEvent,
        seen: inout Set<String>
    ) -> Alert? {
        let taskId = event.data.taskId ?? event.taskId
        switch event.kind {
        case "approval_pending":
            let scope = event.data.requestId ?? taskId
            let key = "approval|\(event.conversationId)|\(scope)"
            guard seen.insert(key).inserted else { return nil }
            return Alert(
                key: key,
                title: "Marble needs approval",
                body: event.data.tool ?? "a governed tool"
            )
        case "settled":
            let key = "settled|\(event.conversationId)|\(taskId)"
            guard seen.insert(key).inserted else { return nil }
            let reason = event.data.reason ?? event.data.status ?? ""
            let cancelled = event.data.cancelled == true || reason == "cancelled"
            let title: String
            if cancelled {
                title = "Marble stopped"
            } else if ["failed", "error", "interrupted"].contains(reason) {
                title = "Marble hit a problem"
            } else {
                title = "Marble finished"
            }
            return Alert(key: key, title: title, body: reason.isEmpty ? "turn settled" : reason)
        default:
            return nil
        }
    }
}
