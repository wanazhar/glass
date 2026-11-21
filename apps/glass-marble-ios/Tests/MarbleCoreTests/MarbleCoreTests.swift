import Foundation
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
import XCTest
@testable import MarbleCore

private func fixture(_ name: String) throws -> Data {
    let url = Bundle.module.url(forResource: "Fixtures/\(name)", withExtension: nil)!
    return try Data(contentsOf: url)
}

private func decode<T: Decodable>(_ type: T.Type, _ name: String) throws -> T {
    try JSONDecoder().decode(type, from: fixture(name))
}

final class EndpointTests: XCTestCase {
    func testBareHostGetsHTTP() throws {
        let e = try MarbleEndpoint.parse("  100.98.115.95:4783  ").get()
        XCTAssertEqual(e.baseURL.absoluteString, "http://100.98.115.95:4783")
    }

    func testTrailingSlashStripped() throws {
        let e = try MarbleEndpoint.parse("http://host:4783/").get()
        XCTAssertEqual(e.baseURL.absoluteString, "http://host:4783")
    }

    func testHTTPSKept() throws {
        let e = try MarbleEndpoint.parse("https://marble.example.com").get()
        XCTAssertEqual(e.baseURL.scheme, "https")
    }

    func testRejectsNonHTTP() {
        XCTAssertEqual(MarbleEndpoint.parse("ftp://host"), .failure(.badScheme))
    }

    func testRejectsEmpty() {
        XCTAssertEqual(MarbleEndpoint.parse("   "), .failure(.empty))
    }

    func testRejectsMissingHost() {
        XCTAssertEqual(MarbleEndpoint.parse("http://"), .failure(.missingHost))
    }

    func testPathJoinsOntoBase() throws {
        let e = try MarbleEndpoint.parse("http://host:4783").get()
        XCTAssertEqual(
            e.url(path: "/api/state")?.absoluteString,
            "http://host:4783/api/state"
        )
    }

    func testQueryEncoding() throws {
        let e = try MarbleEndpoint.parse("http://h").get()
        let url = e.url(path: "/api/transcript/search", query: [
            URLQueryItem(name: "q", value: "a b&c"),
            URLQueryItem(name: "limit", value: "5"),
        ])
        XCTAssertEqual(
            url?.absoluteString,
            "http://h/api/transcript/search?q=a%20b%26c&limit=5"
        )
    }
}

final class RequestTests: XCTestCase {
    func testBearerHeaderSet() throws {
        var e = try MarbleEndpoint.parse("http://h").get()
        e.token = "secret-token"
        let request = try e.request(path: "/api/state")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Authorization"), "Bearer secret-token")
        XCTAssertEqual(request.value(forHTTPHeaderField: "Accept"), "application/json")
        XCTAssertEqual(request.httpMethod, "GET")
    }

    func testNoTokenNoAuthHeader() throws {
        let e = try MarbleEndpoint.parse("http://h").get()
        let request = try e.request(path: "/api/state")
        XCTAssertNil(request.value(forHTTPHeaderField: "Authorization"))
    }

    func testEventStreamAccept() throws {
        let e = try MarbleEndpoint.parse("http://h").get()
        let request = try e.request(path: "/api/events", acceptsEventStream: true)
        XCTAssertEqual(request.value(forHTTPHeaderField: "Accept"), "text/event-stream")
    }

    func testPostBodyContentType() throws {
        let e = try MarbleEndpoint.parse("http://h").get()
        let request = try e.request(path: "/api/turns", method: "POST", body: Data("{}".utf8))
        XCTAssertEqual(request.value(forHTTPHeaderField: "Content-Type"), "application/json")
        XCTAssertEqual(request.httpBody, Data("{}".utf8))
    }
}

final class FixtureDecodeTests: XCTestCase {
    func testHealthzDecodes() throws {
        let health = try decode(MarbleHealth.self, "healthz.json")
        XCTAssertEqual(health.ok, true)
        XCTAssertEqual(health.version, "0.3.14")
        XCTAssertEqual(health.authRequired, true)
        XCTAssertEqual(health.runtime?.health, "ok")
    }

    func testLeanStateDecodes() throws {
        let state = try decode(MarbleState.self, "state-lean.json")
        XCTAssertFalse(state.conversations.isEmpty)
        XCTAssertNotNil(state.conversations[0].id)
    }

    func testTranscriptDecodes() throws {
        let page = try decode(MarbleTranscriptPage.self, "transcript.json")
        XCTAssertFalse(page.entries.isEmpty)
    }

    func testSearchDecodes() throws {
        struct Reply: Decodable { var hits: [MarbleSearchHit] }
        let reply = try decode(Reply.self, "search.json")
        XCTAssertFalse(reply.hits.isEmpty)
        XCTAssertTrue(reply.hits[0].text.contains("pong"))
    }

    func testExportDecodes() throws {
        struct Export: Decodable {
            var format: String?
            var transcript: [MarbleEntry]?
            var conversations: [MarbleConversation]?
        }
        let export = try decode(Export.self, "export.json")
        XCTAssertEqual(export.format, "glass-marble-conversation/v1")
    }

    func testEventDecodes() throws {
        struct Event: Decodable {
            var kind: String
            var sequence: UInt64
            var conversationId: String
        }
        let event = try decode(Event.self, "event.json")
        XCTAssertEqual(event.kind, "text")
    }

    func testNoTokenInFixtures() throws {
        for name in ["healthz.json", "state-lean.json", "transcript.json",
                     "search.json", "export.json", "events.sse"] {
            let text = String(data: try fixture(name), encoding: .utf8) ?? ""
            XCTAssertFalse(text.contains("Bearer"), "\(name) leaks an auth header")
        }
    }
}

final class SSEParserTests: XCTestCase {
    func testLiveCaptureParses() throws {
        var parser = MarbleSSEParser()
        var frames: [MarbleSSEFrame] = []
        let text = String(decoding: try fixture("events.sse"), as: UTF8.self)
        for line in text.split(separator: "\n", omittingEmptySubsequences: false) {
            if let frame = parser.feed(String(line)) { frames.append(frame) }
        }
        XCTAssertEqual(frames.count, 17)
        XCTAssertEqual(frames[0].id, "1")
        XCTAssertEqual(frames[0].event, "queued")
        // tool and approval frames captured live from a real denied call.
        XCTAssertTrue(frames.contains { $0.event == "approval_pending" })
        XCTAssertTrue(frames.contains { $0.event == "approval_resolved" })
        XCTAssertTrue(frames.contains { $0.event == "tool_end" })
        XCTAssertTrue(frames.contains { $0.event == "settled" })
        // The text event carries its word.
        let textFrame = frames.first { $0.event == "text" }
        XCTAssertTrue(textFrame?.data.contains("fixturemark") == true)
    }

    func testKeepaliveIgnored() {
        var parser = MarbleSSEParser()
        XCTAssertNil(parser.feed(": keepalive"))
        XCTAssertNil(parser.feed(""))
    }

    func testMultiLineData() {
        var parser = MarbleSSEParser()
        XCTAssertNil(parser.feed("data: one"))
        XCTAssertNil(parser.feed("data: two"))
        let frame = parser.feed("")
        XCTAssertEqual(frame?.data, "one\ntwo")
    }

    func testEventAndIDFields() {
        var parser = MarbleSSEParser()
        _ = parser.feed("id: 42")
        _ = parser.feed("event: text")
        _ = parser.feed("data: {}")
        let frame = parser.feed("")
        XCTAssertEqual(frame?.id, "42")
        XCTAssertEqual(frame?.event, "text")
    }

    func testFieldlessLineIgnored() {
        var parser = MarbleSSEParser()
        XCTAssertNil(parser.feed("somegarbage"))
        XCTAssertNil(parser.feed(""))
    }

    func testFinishFlushesOpenFrame() {
        var parser = MarbleSSEParser()
        _ = parser.feed("data: tail")
        XCTAssertEqual(parser.finish()?.data, "tail")
    }

    func testBlankFrameReturnsNil() {
        var parser = MarbleSSEParser()
        XCTAssertNil(parser.feed(""))
    }
}

final class ErrorMappingTests: XCTestCase {
    private func validate(status: Int, body: String = "") throws {
        let response = HTTPURLResponse(
            url: URL(string: "http://h/api")!,
            statusCode: status,
            httpVersion: nil,
            headerFields: nil
        )!
        try marbleValidate(response, data: Data(body.utf8))
    }

    func testStatusMapping() {
        XCTAssertThrowsError(try validate(status: 401)) {
            XCTAssertEqual($0 as? MarbleClientError, .unauthorized)
        }
    }

    func testForbidden() {
        XCTAssertThrowsError(try validate(status: 403)) {
            XCTAssertEqual($0 as? MarbleClientError, .forbidden)
        }
    }

    func testNotFound() {
        XCTAssertThrowsError(try validate(status: 404)) {
            XCTAssertEqual($0 as? MarbleClientError, .notFound)
        }
    }

    func testConflictCarriesCode() {
        XCTAssertThrowsError(try validate(status: 409, body: #"{"error":"task_running"}"#)) {
            XCTAssertEqual($0 as? MarbleClientError, .conflict("task_running"))
        }
    }

    func testServerError() {
        XCTAssertThrowsError(try validate(status: 502, body: "bad gateway")) {
            guard case .server(let code, _) = $0 as? MarbleClientError else {
                return XCTFail("expected .server, got \($0)")
            }
            XCTAssertEqual(code, 502)
        }
    }

    func testURLErrorMapsToUnreachable() {
        let error = URLError(.cannotConnectToHost)
        guard case .unreachable = MarbleClientError.from(error) else {
            return XCTFail("expected .unreachable")
        }
    }

    func testTimeoutMaps() {
        XCTAssertEqual(MarbleClientError.from(URLError(.timedOut)), .timeout)
    }

    func testErrorCopyPresent() {
        for error: MarbleClientError in [
            .invalidURL, .unreachable(""), .timeout, .offline(""), .unauthorized,
            .forbidden, .notFound, .conflict("x"), .server(500, ""), .decoding("x"),
            .http(418, ""),
        ] {
            XCTAssertFalse(error.errorDescription?.isEmpty ?? true, "\(error)")
        }
    }
}

final class SemverTests: XCTestCase {
    func testEqual() {
        XCTAssertEqual(MarbleCompatibility.compare("0.3.14", "0.3.14"), 0)
    }

    func testOlderHost() {
        XCTAssertLessThan(MarbleCompatibility.compare("0.3.13", "0.3.14"), 0)
    }

    func testNewerHost() {
        XCTAssertGreaterThan(MarbleCompatibility.compare("1.0.0", "0.3.14"), 0)
    }

    func testDirtySuffixIgnored() {
        XCTAssertTrue(MarbleCompatibility.isSupported("0.3.14-dirty"))
    }

    func testMinimum() {
        XCTAssertTrue(MarbleCompatibility.isSupported("0.3.14"))
        XCTAssertFalse(MarbleCompatibility.isSupported("0.3.13"))
    }
}

final class OnboardingEvaluatorTests: XCTestCase {
    private let health = MarbleHealth(
        ok: true, version: "0.3.14", revision: 1, sequence: 0, authRequired: true,
        status: "idle",
        runtime: MarbleHealth.RuntimeHealth(health: "ok", lastError: nil)
    )
    private let url = "http://100.64.0.5:4783"

    private func verdict(
        _ h: Result<MarbleHealth, MarbleClientError>,
        _ s: MarbleOnboardingEvaluator.StateOutcome,
        token: Bool
    ) -> MarbleOnboarding {
        MarbleOnboardingEvaluator.evaluate(urlTried: url, health: h, state: s, hadToken: token)
    }

    func testReady() {
        XCTAssertEqual(verdict(.success(health), .ok, token: true), .ready("0.3.14"))
    }

    func testUnreachable() {
        guard case .unreachable(let message) = verdict(
            .failure(.unreachable("nope")), .ok, token: false
        ) else { return XCTFail("expected .unreachable") }
        XCTAssertTrue(message.contains(url))
    }

    func testTokenRequired() {
        XCTAssertEqual(verdict(.success(health), .http(401, "unauthorized"), token: false), .tokenRequired)
    }

    func testBadToken() {
        XCTAssertEqual(verdict(.success(health), .http(401, "unauthorized"), token: true), .badToken)
    }

    func testVersionMismatch() {
        var old = health
        old.version = "0.3.10"
        XCTAssertEqual(
            verdict(.success(old), .ok, token: true),
            .versionMismatch(found: "0.3.10", required: "0.3.14")
        )
    }

    func testUnhealthy() {
        var bad = health
        bad.runtime = MarbleHealth.RuntimeHealth(health: "error", lastError: "engine died")
        guard case .unhealthy(let detail) = verdict(.success(bad), .ok, token: true) else {
            return XCTFail("expected .unhealthy")
        }
        XCTAssertEqual(detail, "engine died")
    }

    func testNotMarble() {
        var odd = health
        odd.version = nil
        XCTAssertEqual(verdict(.success(odd), .ok, token: true), .notMarble)
        XCTAssertEqual(verdict(.failure(.decoding("x")), .ok, token: true), .notMarble)
    }

    func testStateFailureIsUnhealthyNotBadToken() {
        // A 5xx (or other non-401) state answer is a host problem, not a token
        // problem — even with a token present.
        XCTAssertEqual(
            verdict(.success(health), .failed("host exploded"), token: true),
            .unhealthy("host exploded")
        )
    }

    func testUnhealthyFlag() {
        var bad = health
        bad.ok = false
        guard case .unhealthy = verdict(.success(bad), .ok, token: true) else {
            return XCTFail("expected .unhealthy")
        }
    }
}

final class AttentionTests: XCTestCase {
    private func event(
        kind: String, taskId: String = "task-1",
        reason: String? = nil, cancelled: Bool? = nil, requestId: String? = nil
    ) -> MarbleEvent {
        MarbleEvent(
            sequence: 1,
            conversationId: "conv-1",
            taskId: taskId,
            kind: kind,
            data: MarbleEventData(
                text: nil, tool: nil, isError: nil, message: nil, status: nil,
                revision: nil, taskId: taskId, reason: reason,
                cancelled: cancelled, requestId: requestId, approved: nil
            )
        )
    }

    func testDoubleSettledNotifiesOnce() {
        var seen: Set<String> = []
        let first = MarbleAttention.alert(
            for: event(kind: "settled", reason: "completed"), seen: &seen
        )
        XCTAssertEqual(first?.title, "Marble finished")
        let second = MarbleAttention.alert(
            for: event(kind: "settled", cancelled: false), seen: &seen
        )
        XCTAssertNil(second)
    }

    func testCancelledTitle() {
        var seen: Set<String> = []
        let alert = MarbleAttention.alert(
            for: event(kind: "settled", reason: "cancelled"), seen: &seen
        )
        XCTAssertEqual(alert?.title, "Marble stopped")
    }

    func testCancelledFlagTitle() {
        var seen: Set<String> = []
        let alert = MarbleAttention.alert(
            for: event(kind: "settled", cancelled: true), seen: &seen
        )
        XCTAssertEqual(alert?.title, "Marble stopped")
    }

    func testFailedTitle() {
        var seen: Set<String> = []
        for reason in ["failed", "error", "interrupted"] {
            seen = []
            let alert = MarbleAttention.alert(
                for: event(kind: "settled", reason: reason), seen: &seen
            )
            XCTAssertEqual(alert?.title, "Marble hit a problem", reason)
        }
    }

    func testApprovalAlert() {
        var seen: Set<String> = []
        let alert = MarbleAttention.alert(
            for: event(kind: "approval_pending", requestId: "req-1"), seen: &seen
        )
        XCTAssertEqual(alert?.title, "Marble needs approval")
        XCTAssertNil(
            MarbleAttention.alert(
                for: event(kind: "approval_pending", requestId: "req-1"), seen: &seen
            )
        )
    }

    func testOtherKindsSilent() {
        var seen: Set<String> = []
        XCTAssertNil(MarbleAttention.alert(for: event(kind: "text"), seen: &seen))
        XCTAssertNil(MarbleAttention.alert(for: event(kind: "state"), seen: &seen))
    }
}

final class EmbeddedQueryTests: XCTestCase {
    private let endpoint = try! MarbleEndpoint.parse("http://100.64.0.5:4783").get()

    func testQuestionMarkInPathBecomesQuery() {
        let url = endpoint.url(path: "/api/state?lean=1")
        XCTAssertNotNil(url)
        XCTAssertFalse(url!.absoluteString.contains("%3F"))
        XCTAssertEqual(url?.path, "/api/state")
        XCTAssertEqual(url?.query, "lean=1")
    }

    func testEmbeddedAndExplicitQueryMerge() {
        let url = endpoint.url(
            path: "/api/transcript?conversationId=abc",
            query: [URLQueryItem(name: "after", value: "7")]
        )
        XCTAssertEqual(url?.path, "/api/transcript")
        XCTAssertTrue(url!.query!.contains("conversationId=abc"))
        XCTAssertTrue(url!.query!.contains("after=7"))
    }

    func testRouteQueryStillBuilds() {
        let route = MarbleRoute.state
        let url = endpoint.url(path: route.path, query: route.query)
        XCTAssertEqual(url?.absoluteString, "http://100.64.0.5:4783/api/state?lean=1")
    }
}

final class TokenNormalizeTests: XCTestCase {
    func testTrimsWhitespaceAndNewlines() {
        XCTAssertEqual(MarbleToken.normalize("  abc123\n"), "abc123")
        XCTAssertEqual(MarbleToken.normalize("abc 123 def"), "abc123def")
        XCTAssertEqual(MarbleToken.normalize("ab\ncd\nef"), "abcdef")
        XCTAssertEqual(MarbleToken.normalize("  ab c \t d  "), "abcd")
    }

    func testStripsQuotes() {
        XCTAssertEqual(MarbleToken.normalize("\"abc123\""), "abc123")
        XCTAssertEqual(MarbleToken.normalize("'abc123'"), "abc123")
        XCTAssertEqual(MarbleToken.normalize(" \" abc \" "), "abc")
    }

    func testStripsBearerPrefix() {
        XCTAssertEqual(MarbleToken.normalize("Bearer abc123"), "abc123")
        XCTAssertEqual(MarbleToken.normalize("bearer  abc123"), "abc123")
        XCTAssertEqual(MarbleToken.normalize("BEARER\tabc"), "abc")
        // A token that merely starts with the letters is kept whole.
        XCTAssertEqual(MarbleToken.normalize("bearerabc"), "bearerabc")
    }

    func testEmpty() {
        XCTAssertEqual(MarbleToken.normalize("   "), "")
        XCTAssertEqual(MarbleToken.normalize("Bearer "), "")
    }
}

final class EvaluatorMessageTests: XCTestCase {
    private let health = MarbleHealth(
        ok: true, version: "0.3.14", revision: 1, sequence: 0, authRequired: true,
        status: "idle",
        runtime: MarbleHealth.RuntimeHealth(health: "ok", lastError: nil)
    )
    private let url = "http://100.64.0.5:4783"

    private func verdict(
        _ h: Result<MarbleHealth, MarbleClientError>,
        _ s: MarbleOnboardingEvaluator.StateOutcome,
        token: Bool = true
    ) -> MarbleOnboarding {
        MarbleOnboardingEvaluator.evaluate(urlTried: url, health: h, state: s, hadToken: token)
    }

    func testHealthAnswersNeverSayCannotReach() {
        // Any HTTP status means the host answered; only transport errors may
        // claim "Cannot reach".
        for error: MarbleClientError in [
            .unauthorized, .forbidden, .notFound,
            .server(500, "{\"error\":\"boom\"}"), .http(418, ""),
        ] {
            let verdict = verdict(.failure(error), .ok)
            guard case .hostAnswered(let message) = verdict else {
                return XCTFail("expected .hostAnswered for \(error), got \(verdict)")
            }
            XCTAssertTrue(message.contains("healthz"), message)
        }
    }

    func testHealthServerErrorCarriesStatusAndCode() {
        let verdict = verdict(
            .failure(.server(503, "{\"error\":\"engine_down\"}")), .ok
        )
        XCTAssertEqual(
            verdict,
            .hostAnswered("The host answered HTTP 503 (engine_down) for /healthz.")
        )
    }

    func testStateUnauthorizedIsBadToken() {
        XCTAssertEqual(
            verdict(.success(health), .http(401, "unauthorized")),
            .badToken
        )
        XCTAssertTrue(MarbleOnboarding.badToken.message.contains("401"))
    }

    func testState404IsMismatchNotBadToken() {
        let verdict = verdict(.success(health), .http(404, "not_found"))
        guard case .hostAnswered(let message) = verdict else {
            return XCTFail("expected .hostAnswered, got \(verdict)")
        }
        XCTAssertTrue(message.contains("404"), message)
        XCTAssertTrue(message.contains("0.3.14"), message)
    }

    func testStateOtherStatusCarriesCode() {
        XCTAssertEqual(
            verdict(.success(health), .http(503, "engine_down")),
            .hostAnswered("The host answered HTTP 503 (engine_down) for /api/state.")
        )
    }

    func testStateTransportHasOwnMessage() {
        guard case .unreachable(let message) = verdict(
            .success(health), .transport("connection lost"), token: true
        ) else { return XCTFail("expected .unreachable") }
        XCTAssertTrue(message.contains("/api/state"), message)
        XCTAssertTrue(message.contains("connection lost"), message)
    }

    func testOutcomeMapping() {
        XCTAssertEqual(
            MarbleOnboardingEvaluator.stateOutcome(from: MarbleClientError.unauthorized),
            .http(401, "unauthorized")
        )
        XCTAssertEqual(
            MarbleOnboardingEvaluator.stateOutcome(
                from: MarbleClientError.server(500, "{\"error\":\"boom\"}")
            ),
            .http(500, "boom")
        )
        guard case .transport = MarbleOnboardingEvaluator.stateOutcome(
            from: MarbleClientError.timeout
        ) else { return XCTFail("expected .transport") }
        guard case .failed = MarbleOnboardingEvaluator.stateOutcome(
            from: MarbleClientError.decoding("bad json")
        ) else { return XCTFail("expected .failed") }
    }
}
