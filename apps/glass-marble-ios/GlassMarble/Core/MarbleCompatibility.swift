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
    /// The host answered over HTTP, so "cannot reach" would be a lie.
    case hostAnswered(String)

    var message: String {
        switch self {
        case .ready(let version):
            return "Connected to Marble \(version)."
        case .unreachable(let detail):
            return detail
        case .badToken:
            return "The host rejected that token (HTTP 401). Copy it again from the token file on the host."
        case .tokenRequired:
            return "This host requires an access token — paste it above."
        case .versionMismatch(let found, let required):
            return "Host is Marble \(found); this app needs \(required) or newer. Update the host."
        case .unhealthy(let detail):
            return "Marble answered but is not healthy: \(detail)"
        case .notMarble:
            return "Something answered there, but it is not a Marble host."
        case .hostAnswered(let detail):
            return detail
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
        /// Any HTTP status the host returned, with its `error` code if one
        /// was sent (empty string otherwise).
        case http(Int, String)
        /// No HTTP answer at all after a good health check.
        case transport(String)
        /// The host answered, but not with JSON the app understands.
        case failed(String)
    }

    /// Map a thrown client error to a state outcome: statuses keep their
    /// code, transport failures keep their description.
    static func stateOutcome(from error: Error) -> StateOutcome {
        guard let error = error as? MarbleClientError else {
            return .transport(error.localizedDescription)
        }
        switch error {
        case .unauthorized:
            return .http(401, "unauthorized")
        case .forbidden:
            return .http(403, "forbidden")
        case .notFound:
            return .http(404, "not_found")
        case .conflict(let code):
            return .http(409, code)
        case .server(let status, let body), .http(let status, let body):
            return .http(status, MarbleClientError.errorCode(from: body))
        case .unreachable, .timeout, .offline:
            return .transport(error.localizedDescription)
        case .decoding(let detail):
            return .failed(detail)
        case .invalidURL:
            return .failed(error.localizedDescription)
        }
    }

    /// The status an HTTP-layer error carries, for the health messages.
    static func statusCode(of error: MarbleClientError) -> Int {
        switch error {
        case .unauthorized: return 401
        case .forbidden: return 403
        case .notFound: return 404
        case .conflict: return 409
        case .server(let status, _), .http(let status, _): return status
        default: return 0
        }
    }

    /// The host `error` code an HTTP-layer error carries, if any.
    static func errorCode(of error: MarbleClientError) -> String {
        switch error {
        case .unauthorized: return "unauthorized"
        case .forbidden: return "forbidden"
        case .notFound: return "not_found"
        case .conflict(let code): return code
        case .server(_, let body), .http(_, let body):
            return MarbleClientError.errorCode(from: body)
        default: return ""
        }
    }

    private static func httpDetail(_ status: Int, _ code: String) -> String {
        code.isEmpty ? "HTTP \(status)" : "HTTP \(status) (\(code))"
    }

    /// The probe's decision plumbing, shared by onboarding and the live
    /// tests: fetch health, fetch state, fold both into one verdict. The
    /// caller supplies the network calls so the iOS client and the Linux
    /// test client can each use their own transport.
    static func probe(
        endpoint: MarbleEndpoint,
        health: () async throws -> MarbleHealth,
        state: () async throws -> Any
    ) async -> (MarbleOnboarding, MarbleEndpoint) {
        let healthResult: Result<MarbleHealth, MarbleClientError>
        var stateOutcome = StateOutcome.failed("state check failed")
        do {
            healthResult = .success(try await health())
            do {
                _ = try await state()
                stateOutcome = .ok
            } catch {
                stateOutcome = Self.stateOutcome(from: error)
            }
        } catch let error as MarbleClientError {
            healthResult = .failure(error)
        } catch {
            healthResult = .failure(.from(error))
        }
        let verdict = evaluate(
            urlTried: endpoint.baseURL.absoluteString,
            health: healthResult,
            state: stateOutcome,
            hadToken: endpoint.token != nil
        )
        return (verdict, endpoint)
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
            case .unreachable, .timeout, .offline, .invalidURL:
                // The host never answered: only transport failures may say so.
                return .unreachable(
                    "Cannot reach \(urlTried). Is Tailscale on, the phone on the same tailnet, and the port right?"
                )
            case .unauthorized:
                return .hostAnswered(
                    "The host answered 401 for /healthz — the health check is public, so something else is listening there."
                )
            case .forbidden:
                return .hostAnswered(
                    "The host answered 403 for /healthz — the health check is public, so something else is listening there."
                )
            case .decoding:
                return .notMarble
            default:
                return .hostAnswered(
                    "The host answered \(httpDetail(statusCode(of: error), errorCode(of: error))) for /healthz."
                )
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
            case .http(let status, _) where status == 401:
                if !health.authRequired { return .unhealthy("state returned 401") }
                return hadToken ? .badToken : .tokenRequired
            case .http(let status, _) where status == 404:
                return .hostAnswered(
                    "The host answered 404 for /api/state — this app and host don't match (host is Marble \(version))."
                )
            case .http(let status, let code):
                return .hostAnswered(
                    "The host answered \(httpDetail(status, code)) for /api/state."
                )
            case .transport(let detail):
                return .unreachable(
                    "The health check answered but /api/state did not: \(detail)"
                )
            case .failed(let detail):
                // A state endpoint that answers garbage is a host problem,
                // never a token problem.
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
