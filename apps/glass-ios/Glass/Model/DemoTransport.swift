#if DEBUG
import Foundation
import GlassModels
import GlassTransport

actor DemoTransport: GlassHTTPTransport {
    private let workspace = "/Users/robin/Projects/Harbor"
    private var threads: [ThreadSummary]
    private var subscriptions: [String: Subscription] = [:]
    private var turn: Task<Void, Never>?
    private let preview: LensPreview?

    private struct Subscription {
        var stream: AsyncThrowingStream<Data, any Error>.Continuation
        var generation: UUID
        var sequence: UInt64 = 1
    }

    init(threadCount: Int = 336, preview: LensPreview? = nil) {
        self.preview = preview
        let subjects = ["Improve incremental sync", "Audit cache eviction", "Review deployment notes",
                        "Make search keyboard-friendly", "Clean up the release checklist", "Specify cursor behavior",
                        "Track intermittent test failures", "Measure export latency", "Update onboarding copy"]
        let now = Date.now
        threads = (0..<threadCount).map { index in
            let title = switch index {
            case 0: preview == nil ? "Repair incremental sync" : "Fix the nil crash in TitleView"
            case 1: "Investigate the staging timeout"
            case 3: "Review sync measurements"
            case 999: "Optimize large workspace imports"
            default: subjects[index % subjects.count]
            }
            let day: Double = index < 5 ? 0 : index < 9 ? 1 : index < 38 ? 4 : index < 103 ? 18 : Double(index) / 2
            let age = preview == nil ? (index < 16 ? Double(index * 420) : Double(index * 24_600)) : day * 86_400
            let showcaseDay = [0, 0, 1, 4, 18, 30][min(index, 5)] + max(0, index - 5) / 2
            let date = preview == .drawer
                ? (Calendar.current.date(byAdding: .day, value: -showcaseDay, to: now) ?? now)
                : now.addingTimeInterval(-age)
            var row = ThreadSummary(session: "harbor-\(index)", title: title, workspace: "/Users/robin/Projects/Harbor",
                                    modified: date.timeIntervalSince1970 - (preview == .drawer ? Double(index) : 0))
            row.pinned = preview == .drawer ? index == 0 : (preview == nil ? index < 3 : index == 2 || index == 3)
            row.model = preview == nil ? "Claude Sonnet 4.6" : "Opus 4.7"
            if preview != nil { row.running = index == 4 }
            return row
        }
        if let preview, let id = preview.threadID, id.hasPrefix("lens-") {
            var row = ThreadSummary(session: id, title: "Fix the nil crash in TitleView", workspace: workspace,
                                    modified: now.timeIntervalSince1970)
            row.model = "Opus 4.7"
            threads.insert(row, at: 0)
        }
    }

    func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        guard let url = request.url else { throw URLError(.badURL) }
        let query = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems ?? []
        func param(_ key: String) -> String? { query.first { $0.name == key }?.value }
        let body = request.httpBody.flatMap { try? JSONDecoder().decode(JSONValue.self, from: $0) } ?? .null
        let id = body["id"]?.stringValue ?? param("id") ?? ""
        switch url.path {
        case "/api/status":
            return try response(url, ["name": "Studio Mac", "workspace": workspace, "remote": true,
                                      "protocolVersion": 1, "capabilities": ["events-resume"]])
        case "/api/threads":
            let search = param("q") ?? ""
            let archived = param("archived") == "true"
            let matches = threads.filter {
                ($0.archived == true) == archived && (search.isEmpty || $0.displayTitle.localizedCaseInsensitiveContains(search))
            }
            let rows = try JSONSerialization.jsonObject(with: JSONEncoder().encode(matches))
            return try response(url, ["threads": rows, "offset": 0, "total_threads": matches.count,
                                      "projects": [["workspace": workspace, "count": threads.count]]])
        case "/api/new":
            let id = "draft-\(UUID().uuidString)"
            threads.insert(ThreadSummary(session: id, workspace: workspace, modified: Date.now.timeIntervalSince1970), at: 0)
            return try response(url, ["id": id])
        case "/api/open", "/api/close":
            if id == "harbor-1" { return try response(url, ["error": "The studio Mac is offline."], status: 503) }
            return try response(url, ["session": id, "originLabel": "Created in Glass"])
        case "/api/history":
            let records: [[String: any Sendable]] = switch id {
            case "harbor-0": preview == nil ? Self.history : Self.titleHistory(ipad: preview == .ipad, log: preview == .workLog)
            case "harbor-1": preview == nil
                ? [["k": "user", "t": "Check why staging stops responding after deployment."]]
                : Self.titleHistory()
            case "harbor-2": [["k": "assistant", "t": "[Deployment notes](docs/deployment.md)"]]
            case "harbor-3": Self.measurements
            case "lens-working": Self.workingHistory
            case "lens-failure": Self.failedHistory
            case "lens-readonly": Self.titleHistory()
            case "lens-rich": Self.richHistory
            default: []
            }
            return try response(url, ["before": 0, "records": records.enumerated().map { ["index": $0.offset + 1, "record": $0.element] }])
        case "/api/thread-preferences":
            if let index = threads.firstIndex(where: { $0.id == id }) {
                switch body["action"]?.stringValue {
                case "pin": threads[index].pinned = body["value"]?.boolValue
                case "archive": threads[index].archived = body["value"]?.boolValue
                case "rename": threads[index].title = body["value"]?.stringValue
                case "delete": threads.remove(at: index)
                default: break
                }
            }
        case "/api/context":
            return try response(url, ["attachments": []])
        case "/api/file":
            var text = "struct Checkpoint {\n    let cursor: String\n}"
            if param("path") == "docs/deployment.md" {
                guard id == "harbor-2", param("workspace") == nil else {
                    return try response(url, ["error": "Open this file from its conversation."], status: 403)
                }
                text = """
                # Deployment notes

                ![Deployment diagram](https://example.invalid/deployment.png)

                ```glass-ui
                {"type":"image","data":{"url":"https://example.invalid/runbook.png","alt":"Runbook diagram"}}
                ```
                """
            }
            return (Data(text.utf8),
                    HTTPURLResponse(url: url, statusCode: 200, httpVersion: nil, headerFields: ["Content-Type": "text/plain"])!)
        case "/api/commands":
            return try response(url, ["commands": []])
        case "/api/send":
            if let message = body["message"] { try handle(message, id: id) }
        default: break
        }
        return try response(url, [:])
    }

    func stream(_ request: URLRequest) async throws -> (AsyncThrowingStream<Data, any Error>, HTTPURLResponse) {
        guard let url = request.url else { throw URLError(.badURL) }
        let id = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems?.first { $0.name == "id" }?.value ?? ""
        let (stream, continuation) = AsyncThrowingStream<Data, any Error>.makeStream()
        let generation = UUID()
        subscriptions[id]?.stream.finish()
        subscriptions[id] = Subscription(stream: continuation, generation: generation)
        continuation.onTermination = { [weak self] _ in
            Task { await self?.unsubscribe(id, generation: generation) }
        }
        var initial: [String: Any] = ["reset": true, "active": id == "lens-working", "control": ["can_write": id != "lens-readonly"]]
        if id == "lens-working" {
            initial["live"] = [
                ["t": "tool", "call_id": "verify-title", "name": "bash", "command": "swift test --filter TitleViewTests"],
            ]
        }
        continuation.yield(try frame(initial, sequence: 1))
        return (stream, HTTPURLResponse(url: url, statusCode: 200, httpVersion: nil,
                                        headerFields: ["Content-Type": "text/event-stream"])!)
    }

    private func unsubscribe(_ id: String, generation: UUID) {
        if subscriptions[id]?.generation == generation { subscriptions[id] = nil }
    }

    private func handle(_ message: JSONValue, id: String) throws {
        switch message["t"]?.stringValue {
        case "command":
            guard let request = message["request_id"]?.stringValue else { return }
            let result: [String: Any] = switch message["name"]?.stringValue {
            case "model": ["models": preview == nil
                ? ["Claude Sonnet 4.6", "Claude Opus 4.7", "GPT-5.4", "Gemini 3.1 Pro"]
                : ["Opus 4.7", "Sonnet 4.6", "GPT-5.4", "Gemini 3.1 Pro"]]
            case "usage": ["reported_prompt_total": 18240, "reported_output": 2681, "known_cost_subtotal_usd": 0.08]
            case "context": ["workspace": workspace, "used": preview == nil ? "18,240 tokens" : "38K / 200K"]
            default: [:]
            }
            try emit([["t": "command_result", "request_id": request, "result": ["body": result]]], to: id)
        case "prompt":
            let prompt = message["text"]?.stringValue ?? "Keep going"
            try emit([["t": "turn_start", "text": prompt], ["t": "waiting"]], to: id)
            turn?.cancel()
            turn = Task { [weak self] in
                try? await Task.sleep(for: .seconds(1))
                guard !Task.isCancelled else { return }
                try? await self?.emit([["t": "reasoning", "delta": "I’ll check the cursor boundary before changing the retry path."]], to: id)
                let chunks = ["The sync loop now", " keeps its last committed cursor.", " On retry, it replays only the unfinished page", " and leaves completed records alone."]
                for (index, chunk) in chunks.enumerated() {
                    try? await Task.sleep(for: index == 0 ? .seconds(1.25) : .seconds(3))
                    guard !Task.isCancelled else { return }
                    try? await self?.emit([["t": "text", "delta": chunk]], to: id)
                }
                try? await self?.emit([["t": "done"]], to: id)
            }
        case "cancel":
            turn?.cancel()
            try emit([["t": "done"]], to: id)
        case "control":
            try emit([["t": "control", "can_write": message["action"]?.stringValue == "claim"]], to: id)
        default: break
        }
    }

    private func emit(_ events: [[String: Any]], to id: String) throws {
        guard var subscription = subscriptions[id] else { return }
        subscription.sequence += 1
        subscriptions[id] = subscription
        subscription.stream.yield(try frame(["events": events], sequence: subscription.sequence))
    }

    private func frame(_ data: [String: Any], sequence: UInt64) throws -> Data {
        let body = String(decoding: try JSONSerialization.data(withJSONObject: data), as: UTF8.self)
        return Data("id: 1:\(sequence)\ndata: \(body)\n\n".utf8)
    }

    private func response(_ url: URL, _ body: [String: Any], status: Int = 200) throws -> (Data, HTTPURLResponse) {
        (try JSONSerialization.data(withJSONObject: body),
         HTTPURLResponse(url: url, statusCode: status, httpVersion: nil, headerFields: ["Content-Type": "application/json"])!)
    }

    private static let measurements: [[String: any Sendable]] = [
        ["k": "assistant", "t": #"""
        ## Sync measurements

        The expected cost is $\frac{c}{1-p}$ when each retry fails independently with probability $p$.

        $$
        \mathbb{E}[T] = \sum_{k=1}^{\infty} kc(1-p)p^{k-1} = \frac{c}{1-p}
        $$
        """#],
        ["k": "assistant", "t": """
        ```glass-ui
        {"type":"chart","data":{"title":"Retries per run","kind":"pie","series":[{"points":[{"x":"Network","y":12.75},{"x":"Local cache","y":7.25}]}]}}
        ```
        """]
    ]

    private static let history: [[String: any Sendable]] = [
        ["k": "user", "t": "Sync stalls when I reopen a project after the network drops. Can you find where we lose the cursor?"],
        ["k": "reasoning", "t": "Start with the persistence boundary.\nA reconnect must reuse the last cursor committed to disk, not the last cursor seen in memory. Inspect the sync loop and retry state before editing."],
        ["k": "call", "id": "read-sync", "name": "read", "args": #"{"path":"Sources/Sync/Coordinator.swift"}"#],
        ["k": "tool", "id": "read-sync", "name": "read", "t": "Coordinator.swift · lines 74–146"],
        ["k": "bash", "id": "retry-failed", "cmd": "swift test --filter SyncCoordinatorTests", "code": 1,
         "duration_ms": 1360, "out": "Executed 8 tests, with 1 failure.\nstderr:\nSyncCoordinatorTests.swift:91: error: cursor should survive reconnect\nExpected: page-004\nReceived: nil"],
        ["k": "meta", "t": "stopped: The sync regression test did not pass."],
        ["k": "custom", "c": "display/turn-seconds", "d": "66"],
        ["k": "user", "t": "Fix the persistence boundary and add a regression test."],
        ["k": "reasoning", "t": "Keep the stored cursor separate from the in-flight page.\nThe existing retry loop is fine. Only move the commit after a successful batch; add a process restart case to the test."],
        ["k": "call", "id": "edit-sync", "name": "apply_patch", "args": #"{"path":"Sources/Sync/Coordinator.swift"}"#],
        ["k": "tool", "id": "edit-sync", "name": "apply_patch", "t": """
        diff --git a/Sources/Sync/Coordinator.swift b/Sources/Sync/Coordinator.swift
        --- a/Sources/Sync/Coordinator.swift
        +++ b/Sources/Sync/Coordinator.swift
        @@ -88,3 +88,4 @@
        -    cursor = page.cursor
             try await cache.store(page.records)
        +    try await cache.commit(cursor: page.cursor)
        +    cursor = page.cursor
        """],
        ["k": "bash", "id": "sync-pass", "cmd": "swift test --filter SyncCoordinatorTests", "code": 0, "duration_ms": 480, "out": "8 tests passed in 0.48 seconds."],
        ["k": "bash", "id": "format-pass", "cmd": "swift test --filter CacheTests", "code": 0, "duration_ms": 910, "out": "16 tests passed in 0.91 seconds."],
        ["k": "custom", "c": "display/turn-seconds", "d": "134"],
        ["k": "assistant", "t": """
        ## Sync now resumes from the last committed page

        The cursor wasn’t lost on reconnect. We advanced it **before** storing the page, so the retry was starting from a checkpoint that didn’t exist on disk.

        ```swift
        let page = try await api.next(after: cursor)
        try await cache.store(page.records)
        try await cache.commit(cursor: page.cursor)
        cursor = page.cursor
        ```

        ### What changed

        | Scenario | Before | After |
        | --- | --- | --- |
        | Connection drops mid-page | Cursor advances | Last page resumes |
        | App restarts | Cache and cursor disagree | Same checkpoint |
        | Page replays | Duplicate writes | Existing records reused |

        > A cursor is a checkpoint, not a progress indicator.

        ### Verification

        1. Seeded a project with four committed pages.
        2. Cut the connection while saving the fifth page.
        3. Restarted the sync coordinator and confirmed the fifth page was replayed once.

        The targeted suite and cache tests both pass. I left the transport retry policy unchanged so this stays a storage fix.

        [Coordinator.swift](Sources/Sync/Coordinator.swift)
        """],
    ]

    private static let titlePrompt: [String: any Sendable] = [
        "k": "user", "t": "Can you fix the nil crash in the title?",
    ]
    private static let titleCode = """
    ```swift
    // Existing titles stay unchanged.
    let title = item.title ?? "Untitled"
    Text(title)
    ```
    """

    private static func titleHistory(ipad: Bool = false, log: Bool = false) -> [[String: any Sendable]] {
        let setup: [[String: any Sendable]] = ipad ? [
            ["k": "user", "t": "The app crashes when I open a thread from an older export."],
            ["k": "assistant", "t": "Can you share the export that reproduces it?"],
            ["k": "user", "t": "The thread named Release notes is the one that crashes."],
            ["k": "assistant", "t": "I reproduced it. That thread has no title in the export."],
        ] : []
        let change: [[String: any Sendable]] = [
            titlePrompt,
            ["k": "call", "id": "read-title", "name": "read", "args": #"{"path":"TitleView.swift"}"#],
            ["k": "tool", "id": "read-title", "name": "read", "t": "TitleView.swift · lines 24–25"],
            ["k": "reasoning", "t": "Use the fallback only for missing titles so existing text stays intact."],
            ["k": "tool", "id": "edit-title", "name": "apply_patch", "t": """
            diff --git a/TitleView.swift b/TitleView.swift
            --- a/TitleView.swift
            +++ b/TitleView.swift
            @@ -24,2 +24,2 @@
            -let title = item.title!
            +let title = item.title ?? "Untitled"
             Text(title)
            """],
            ["k": "custom", "c": "display/turn-seconds", "d": "134"],
        ]
        let reply: [[String: any Sendable]] = [["k": "assistant", "t": """
        Use nil coalescing to give the title a safe default.

        \(titleCode)

        A missing title now displays “Untitled.” An existing title stays unchanged.
        """]]
        let verification: [[String: any Sendable]] = ipad ? [
            ["k": "user", "t": "Did you check that saved titles still show up?"],
            ["k": "assistant", "t": "Yes. Both title tests passed. The fallback only applies when the stored title is nil."],
        ] : []
        return setup + change + (log ? [] : reply) + verification
    }

    private static let workingHistory: [[String: any Sendable]] = [
        ["k": "user", "t": "Can you help me fix a crash in my iOS app?"],
        ["k": "assistant", "t": "Yes. Share the relevant code or crash details and I’ll trace what’s failing."],
        titlePrompt,
        ["k": "call", "id": "read-title", "name": "read", "args": #"{"path":"TitleView.swift"}"#],
        ["k": "tool", "id": "read-title", "name": "read", "t": "TitleView.swift · lines 24–25"],
        ["k": "reasoning", "t": "Check the optional title before running the title tests."],
    ]

    private static let failedHistory: [[String: any Sendable]] = [
        titlePrompt,
        ["k": "assistant", "t": "The fallback is in place. One test command stopped before it could verify the fix."],
        ["k": "bash", "id": "failed-title-test", "cmd": "swift test --filter TitleViewTests", "code": 1,
         "out": "stderr:\nerror: blank-title.json: No such file or directory"],
        ["k": "meta", "t": "stopped: The title tests couldn’t find blank-title.json. Your changes are saved."],
    ]

    private static let richHistory: [[String: any Sendable]] = [
        ["k": "user", "t": "Compare the lookup times with and without the cache."],
        ["k": "assistant", "t": """
        ## Reading the results

        The speedup is $s = \\frac{t_{cold}}{t_{warm}}$.

        ### Gateway benchmark

        | Scenario | Time | Result |
        | :--- | ---: | :--- |
        | Cold start | 128 ms | No cache |
        | Warm lookup | 32 ms | Cached |
        """],
    ]
}
#endif
