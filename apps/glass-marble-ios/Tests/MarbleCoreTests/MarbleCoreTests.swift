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

    func testEncodedSegmentIsNotEncodedTwice() throws {
        let e = try MarbleEndpoint.parse("http://h").get()
        let id = "host:session:task:call-1"
            .addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? ""
        XCTAssertEqual(
            e.url(path: "/api/approvals/\(id)/allow")?.absoluteString,
            "http://h/api/approvals/host%3Asession%3Atask%3Acall%2D1/allow"
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

final class ATSErrorMappingTests: XCTestCase {
    func testATSErrorMaps() {
        XCTAssertEqual(
            MarbleClientError.from(URLError(.appTransportSecurityRequiresSecureConnection)),
            .appTransportSecurity
        )
    }

    func testSecureConnectionMapsToTLS() {
        guard case .tls = MarbleClientError.from(URLError(.secureConnectionFailed)) else {
            return XCTFail("expected .tls")
        }
    }

    func testServerCertErrorsMapToTLS() {
        for code: URLError.Code in [
            .serverCertificateHasBadDate, .serverCertificateUntrusted,
            .serverCertificateHasUnknownRoot, .serverCertificateNotYetValid,
            .clientCertificateRejected, .clientCertificateRequired,
        ] {
            guard case .tls = MarbleClientError.from(URLError(code)) else {
                return XCTFail("expected .tls for \(code)")
            }
        }
    }
}

final class OnboardingTLSVerdictTests: XCTestCase {
    private let health = MarbleHealth(
        ok: true, version: "0.3.14", revision: 1, sequence: 0, authRequired: true,
        status: "idle",
        runtime: MarbleHealth.RuntimeHealth(health: "ok", lastError: nil)
    )

    func testHTTPAddressThatHitTLSSaysSo() {
        let verdict = MarbleOnboardingEvaluator.evaluate(
            urlTried: "http://100.98.115.95:4783",
            health: .failure(.tls("handshake failed")),
            state: .failed("state check failed"),
            hadToken: true
        )
        guard case .hostAnswered(let message) = verdict else {
            return XCTFail("expected .hostAnswered, got \(verdict)")
        }
        XCTAssertTrue(message.contains("iOS tried to use HTTPS"), message)
    }

    func testHTTPAddressBlockedByATSSaysSo() {
        let verdict = MarbleOnboardingEvaluator.evaluate(
            urlTried: "http://100.98.115.95:4783",
            health: .failure(.appTransportSecurity),
            state: .failed("state check failed"),
            hadToken: true
        )
        guard case .hostAnswered(let message) = verdict else {
            return XCTFail("expected .hostAnswered, got \(verdict)")
        }
        XCTAssertTrue(message.contains("iOS tried to use HTTPS"), message)
    }

    func testSpeaksHTTPCarriesHint() {
        let verdict = MarbleOnboarding.speaksHTTP("http://100.98.115.95:4783")
        XCTAssertEqual(verdict.httpHint, "http://100.98.115.95:4783")
        XCTAssertTrue(verdict.message.contains("HTTP, not HTTPS"))
        XCTAssertNil(MarbleOnboarding.ready("0.3.14").httpHint)
    }

    func testProbeRetriesHTTPSOverHTTP() async {
        // https health dies in TLS; the http retry answers like Marble.
        let endpoint = MarbleEndpoint(
            baseURL: URL(string: "https://100.98.115.95:4783")!, token: "t"
        )
        var calls: [String] = []
        let (verdict, _) = await MarbleOnboardingEvaluator.probe(
            endpoint: endpoint,
            health: { candidate in
                calls.append(candidate.baseURL.absoluteString)
                if candidate.baseURL.scheme == "https" {
                    throw MarbleClientError.tls("handshake failed")
                }
                return self.health
            },
            state: { _ in 0 }
        )
        XCTAssertEqual(verdict, .speaksHTTP("http://100.98.115.95:4783"))
        XCTAssertEqual(calls, [
            "https://100.98.115.95:4783",
            "http://100.98.115.95:4783",
        ])
    }

    func testProbeKeepsTLSVerdictWhenHTTPAlsoFails() async {
        let endpoint = MarbleEndpoint(
            baseURL: URL(string: "https://100.98.115.95:4783")!, token: "t"
        )
        let (verdict, _) = await MarbleOnboardingEvaluator.probe(
            endpoint: endpoint,
            health: { _ in throw MarbleClientError.tls("handshake failed") },
            state: { _ in 0 }
        )
        guard case .unreachable(let message) = verdict else {
            return XCTFail("expected .unreachable, got \(verdict)")
        }
        XCTAssertTrue(message.contains("TLS"), message)
    }
}

final class MarbleCryptoTests: XCTestCase {
    func testSHA256Vectors() {
        XCTAssertEqual(
            MarbleSHA256.hex(Data("abc".utf8)),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        )
        XCTAssertEqual(
            MarbleSHA256.hex(Data()),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        )
        let million = Data(repeating: 0x61, count: 1_000_000)
        XCTAssertEqual(
            MarbleSHA256.hex(million),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        )
    }

    func testHMACRFC4231() {
        XCTAssertEqual(
            MarbleHMAC.sha256Hex(
                key: [UInt8](repeating: 0x0b, count: 20),
                message: [UInt8]("Hi There".utf8)
            ),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        )
        XCTAssertEqual(
            MarbleHMAC.sha256Hex(
                key: [UInt8]("Jefe".utf8),
                message: [UInt8]("what do ya want for nothing?".utf8)
            ),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        )
        XCTAssertEqual(
            MarbleHMAC.sha256Hex(
                key: [UInt8](repeating: 0x0a, count: 25),
                message: [UInt8](repeating: 0xdd, count: 50)
            ),
            "b74f4dabb5e3f7f4c09efc5808cd288179ddd2975060fe3739f2b12a90e47c49"
        )
        // Long keys are hashed first — the >64-byte path.
        XCTAssertEqual(
            MarbleHMAC.sha256Hex(
                key: [UInt8](repeating: 0xaa, count: 131),
                message: [UInt8](
                    "Test Using Larger Than Block-Size Key - Hash Key First".utf8
                )
            ),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        )
    }

    func testNonce() {
        let a = MarbleTLSPin.nonce()
        XCTAssertEqual(a.count, 64)
        XCTAssertTrue(a.allSatisfy { $0.isHexDigit })
        XCTAssertNotEqual(a, MarbleTLSPin.nonce())
    }

    func testVerifyMatchesHostProof() {
        let token = "tokentest"
        let nonce = String(repeating: "a", count: 64)
        let cert = String(repeating: "0", count: 64)
        var message = [UInt8]("marble-tls-pin-v1".utf8)
        message.append(0)
        message.append(contentsOf: [UInt8](nonce.utf8))
        message.append(0)
        message.append(contentsOf: [UInt8](cert.utf8))
        let proof = MarbleHMAC.sha256Hex(key: [UInt8](token.utf8), message: message)
        XCTAssertTrue(MarbleTLSPin.verify(
            token: token, nonce: nonce, observedCertSHA256: cert, proofHex: proof
        ))
        XCTAssertFalse(MarbleTLSPin.verify(
            token: "wrong", nonce: nonce, observedCertSHA256: cert, proofHex: proof
        ))
        XCTAssertFalse(MarbleTLSPin.verify(
            token: token, nonce: nonce, observedCertSHA256: cert,
            proofHex: String(repeating: "f", count: 64)
        ))
    }
}

final class MarbleNavigationTests: XCTestCase {
    func testLaunchLandsOnChatsListWithEmptyPath() {
        var nav = MarbleNavigation()
        nav.openConversation("a")
        nav.selectTab(.settings)
        nav.launch()
        XCTAssertEqual(nav.tab, .everyone)
        XCTAssertTrue(nav.chatPath.isEmpty)
        XCTAssertNil(nav.viewingConversationId)
    }

    func testRefreshNeverChangesNavigation() {
        var nav = MarbleNavigation()
        nav.openConversation("a")
        let before = nav
        nav.refreshed()
        XCTAssertEqual(nav, before)
    }

    func testNewChatPushesDraftAndDraftResolves() {
        var nav = MarbleNavigation()
        nav.newChat()
        nav.newChat() // a second tap does not stack drafts
        XCTAssertEqual(nav.chatPath, [.draft])
        nav.draftResolved("conv-1")
        XCTAssertEqual(nav.chatPath, [.conversation("conv-1")])
        XCTAssertEqual(nav.viewingConversationId, "conv-1")
    }

    func testDeletedPopsBackToList() {
        var nav = MarbleNavigation()
        nav.openConversation("a")
        nav.deleted("a")
        XCTAssertTrue(nav.chatPath.isEmpty)
    }

    func testDeletingChatFromSearchLatestClosesBothSearchRoutes() {
        var nav = MarbleNavigation()
        nav.openMarble("marble")
        // Latest swaps the content of the search destination without replacing its route.
        nav.chatPath.append(.searchHit(conversationId: "a", threadId: nil, entryId: "message"))
        nav.everyonePath = [.searchHit(conversationId: "a", threadId: "thread", entryId: "reply")]
        nav.selectTab(.everyone)
        nav.queuePendingOpen("a")

        nav.deleted("a")

        XCTAssertEqual(nav.chatPath, [.marbleChats])
        XCTAssertTrue(nav.everyonePath.isEmpty)
        XCTAssertEqual(nav.tab, .everyone)
        XCTAssertNil(nav.drainPendingOpen())
    }

    func testDeletingChatPopsItsDescendantsAndPreservesUnrelatedRoutes() {
        var nav = MarbleNavigation()
        let unrelated = ChatRoute.searchHit(conversationId: "b", threadId: nil, entryId: "message")
        nav.chatPath = [.marbleChats, .conversation("a"), .threadInbox("a"), .thread("thread")]
        nav.everyonePath = [unrelated]

        nav.deleted("a")

        XCTAssertEqual(nav.chatPath, [.marbleChats])
        XCTAssertEqual(nav.everyonePath, [unrelated])
    }

    func testNotificationTappedReplacesPath() {
        var nav = MarbleNavigation()
        nav.openConversation("a")
        nav.selectTab(.settings)
        nav.notificationTapped("b")
        XCTAssertEqual(nav.tab, .marbles)
        XCTAssertEqual(nav.chatPath, [.marbleChats, .conversation("b")])
    }

    func testPendingOpenDrainsOnce() {
        var nav = MarbleNavigation()
        nav.queuePendingOpen("x")
        XCTAssertEqual(nav.drainPendingOpen(), "x")
        XCTAssertNil(nav.drainPendingOpen())
    }

    func testSignOutResets() {
        var nav = MarbleNavigation()
        nav.openConversation("a")
        nav.signOut()
        XCTAssertEqual(nav, MarbleNavigation())
    }
}

final class MarbleScrollFollowTests: XCTestCase {
    func testFollowsWhenAtBottom() {
        var follow = MarbleScrollFollow()
        follow.userMoved(distanceFromBottom: 0)
        XCTAssertTrue(follow.following)
        XCTAssertFalse(follow.showJumpButton)
    }

    func testStopsFollowingWhenUserScrollsUp() {
        var follow = MarbleScrollFollow()
        follow.userMoved(distanceFromBottom: 400)
        XCTAssertFalse(follow.following)
        XCTAssertTrue(follow.showJumpButton)
    }

    func testSearchJumpPausesFollowingUntilUserReturnsToBottom() {
        var follow = MarbleScrollFollow()
        follow.pauseForSearchJump()
        XCTAssertFalse(follow.following)
        follow.userMoved(distanceFromBottom: 0)
        XCTAssertTrue(follow.following)
    }

    func testJumpToLatestReArms() {
        var follow = MarbleScrollFollow()
        follow.userMoved(distanceFromBottom: 400)
        follow.jumpToLatest()
        XCTAssertTrue(follow.following)
        XCTAssertFalse(follow.showJumpButton)
    }
}

final class MarbleScheduleTests: XCTestCase {
    func testRoundTrips() {
        XCTAssertEqual(MarbleSchedule(spec: "every:30m")?.spec, "every:30m")
        XCTAssertEqual(MarbleSchedule(spec: "every:2h")?.spec, "every:2h")
        XCTAssertEqual(MarbleSchedule(spec: "daily:09:30")?.spec, "daily:09:30")
        XCTAssertEqual(MarbleSchedule(spec: "daily:9:30"), .daily(hour: 9, minute: 30))
        XCTAssertNil(MarbleSchedule(spec: "daily:25:00"))
        XCTAssertNil(MarbleSchedule(spec: "cron:0 9 * * *"))
    }

    func testPlainLanguage() {
        XCTAssertEqual(MarbleSchedule(spec: "every:2h")?.summary, "Every 2 hours")
        XCTAssertEqual(MarbleSchedule(spec: "every:1h")?.summary, "Every hour")
        XCTAssertEqual(MarbleSchedule(spec: "daily:09:30")?.summary, "Every day at 9:30")
    }
}

final class MarbleTurnBodyTests: XCTestCase {
    func testBodyCarriesExactlyTheDraftAndConversation() throws {
        let data = try JSONEncoder().encode(
            MarbleTurnBody(text: "hello draft", conversationId: "conv-7")
        )
        let doc = try JSONSerialization.jsonObject(with: data) as! [String: Any]
        XCTAssertEqual(doc.count, 2)
        XCTAssertEqual(doc["text"] as? String, "hello draft")
        XCTAssertEqual(doc["conversationId"] as? String, "conv-7")
    }

    func testNilConversationIsNewChat() throws {
        let data = try JSONEncoder().encode(MarbleTurnBody(text: "hi", conversationId: nil))
        let doc = try JSONSerialization.jsonObject(with: data) as! [String: Any]
        XCTAssertEqual(doc["text"] as? String, "hi")
        XCTAssertNil(doc["conversationId"])
    }
}

final class MarbleSSELineSplitterTests: XCTestCase {
    private func split(_ chunks: [[UInt8]]) -> [String] {
        var splitter = MarbleSSELineSplitter()
        var lines: [String] = []
        for chunk in chunks { lines.append(contentsOf: splitter.feed(chunk)) }
        if let last = splitter.finish() { lines.append(last) }
        return lines
    }

    func testBlankLinesAreDelivered() {
        let text = "data: a\n\ndata: b\n\n: keepalive\n\n"
        XCTAssertEqual(
            split([Array(text.utf8)]),
            ["data: a", "", "data: b", "", ": keepalive", ""]
        )
    }

    func testCRLFAndBareCR() {
        let text = "data: a\r\n\r\ndata: b\r\rdata: c"
        XCTAssertEqual(
            split([Array(text.utf8)]),
            ["data: a", "", "data: b", "", "data: c"]
        )
    }

    /// A realistic capture with keepalive framing, replayed at every possible
    /// chunk boundary — the splitter must produce identical lines each time.
    func testEveryChunkBoundaryProducesTheSameLines() {
        let capture = ": keepalive\n\nevent: state\nid: 41\ndata: {\"kind\":\"state\"}\n\n"
        let bytes = Array(capture.utf8)
        let expected = [
            ": keepalive", "", "event: state", "id: 41",
            "data: {\"kind\":\"state\"}", "",
        ]
        XCTAssertEqual(split([bytes]), expected)
        for boundary in 0..<bytes.count {
            XCTAssertEqual(
                split([Array(bytes[0..<boundary]), Array(bytes[boundary...])]),
                expected,
                "split at byte \(boundary) changed the lines"
            )
        }
        // One byte at a time too.
        for byte in bytes.enumerated() { _ = byte }
        XCTAssertEqual(split(bytes.map { [$0] }), expected)
    }

    func testFinishReturnsTrailingPartialLine() {
        var splitter = MarbleSSELineSplitter()
        XCTAssertEqual(splitter.feed(Array("data: x".utf8)), [])
        XCTAssertEqual(splitter.finish(), "data: x")
        XCTAssertNil(splitter.finish())
    }

    /// Parser fed by the splitter: a keepalive, then a real frame, is exactly
    /// what the host sends — and what `bytes.lines` used to swallow.
    func testSplitterFeedsParserEndToEnd() {
        var splitter = MarbleSSELineSplitter()
        var parser = MarbleSSEParser()
        var frames: [MarbleSSEFrame] = []
        for byte in Array("data: {\"a\":1}\r\n\r\n: keepalive\n\ndata: {\"b\":2}\n\n".utf8) {
            for line in splitter.feed(byte) {
                if let frame = parser.feed(line) { frames.append(frame) }
            }
        }
        XCTAssertEqual(frames.map(\.data), ["{\"a\":1}", "{\"b\":2}"])
    }
}

final class MarbleConnectionStatusTests: XCTestCase {
    func testRefreshSuccessConnects() {
        var status = MarbleConnectionStatus()
        XCTAssertEqual(status.label, "Connecting")
        status.refreshSucceeded()
        XCTAssertEqual(status.label, "Connected")
        XCTAssertTrue(status.isConnected)
    }

    func testRefreshFailureGoesOffline() {
        var status = MarbleConnectionStatus()
        status.refreshFailed("timed out")
        XCTAssertEqual(status.label, "Offline")
        XCTAssertEqual(status.offlineReason, "timed out")
    }

    /// The 200 on /api/events alone means connected — even with nothing but
    /// keepalives on the wire.
    func testStreamOpenAloneIsConnected() {
        var status = MarbleConnectionStatus()
        status.streamOpened()
        XCTAssertEqual(status.label, "Connected")
        // And it stays connected across quiet periods: nothing moves it.
        XCTAssertEqual(status.label, "Connected")
    }

    /// A drop right after a working connection stays Connected while one
    /// immediate reconnect runs; only a second failure shows Offline.
    func testDropAfterSuccessKeepsConnectedForOneReconnect() {
        var status = MarbleConnectionStatus()
        status.streamOpened()
        status.streamEnded("stream closed")
        XCTAssertEqual(status.label, "Connected")
        status.reconnectStarted()
        XCTAssertEqual(status.label, "Connected")
        status.streamEnded("still broken")
        XCTAssertEqual(status.label, "Offline")
        XCTAssertEqual(status.offlineReason, "still broken")
    }

    /// A successful reconnect after a drop returns to Connected cleanly.
    func testReconnectSucceedsBackToConnected() {
        var status = MarbleConnectionStatus()
        status.streamOpened()
        status.streamEnded(nil)
        status.reconnectStarted()
        status.streamOpened()
        XCTAssertEqual(status.label, "Connected")
        // A later quiet drop starts the one-reconnect grace again.
        status.streamEnded(nil)
        XCTAssertEqual(status.label, "Connected")
    }

    /// A stream that never opened going straight down is Offline, not grace.
    func testDropWithoutOpenIsOffline() {
        var status = MarbleConnectionStatus()
        status.streamEnded("connection refused")
        XCTAssertEqual(status.label, "Offline")
    }

    func testForegroundStreamRetryUsesResumeGraceOnce() {
        var status = MarbleConnectionStatus()
        status.streamOpened()
        status.suspend()
        status.resume()
        status.reconnectStarted()
        status.streamEnded("first attempt")
        XCTAssertTrue(status.isConnected)
        status.reconnectStarted()
        status.streamEnded("still offline")
        XCTAssertEqual(status.offlineReason, "still offline")
    }

    func testEventsKeepItConnected() {
        var status = MarbleConnectionStatus()
        status.streamOpened()
        status.streamEvent()
        XCTAssertEqual(status.label, "Connected")
    }

    func testSignOutThenSignIn() {
        var status = MarbleConnectionStatus()
        status.streamOpened()
        status.signedOut()
        XCTAssertEqual(status.label, "Signed out")
        status.refreshSucceeded()
        XCTAssertEqual(status.label, "Connected")
    }
}

final class MarbleChatRowNavigationTests: XCTestCase {
    /// The value a conversation row pushes must be that conversation's route —
    /// anything else lands the wrong chat on screen.
    func testRowValueIsTheConversationRoute() {
        XCTAssertEqual(
            ChatRoute.conversation("conv-42"),
            .conversation("conv-42")
        )
        XCTAssertNotEqual(ChatRoute.conversation("conv-42"), .draft)
    }

    func testOpenConversationPushesTheRightChat() {
        var navigation = MarbleNavigation()
        navigation.launch()
        navigation.openConversation("conv-3")
        XCTAssertEqual(navigation.tab, .marbles)
        XCTAssertEqual(navigation.chatPath, [.conversation("conv-3")])
        XCTAssertEqual(navigation.viewingConversationId, "conv-3")
        // Back returns to the list, and a refresh cannot undo that.
        navigation.back()
        XCTAssertTrue(navigation.chatPath.isEmpty)
        navigation.refreshed()
        XCTAssertTrue(navigation.chatPath.isEmpty)
    }

    /// A search hit opens the hit's chat even from another tab or over a draft.
    func testSearchHitOpensItsConversation() {
        var navigation = MarbleNavigation()
        navigation.launch()
        navigation.selectTab(.settings)
        navigation.openConversation("hit-9")
        XCTAssertEqual(navigation.chatPath, [.conversation("hit-9")])
        // Over a draft, the hit replaces it — the person lands in that chat.
        navigation.newChat()
        navigation.openConversation("hit-7")
        XCTAssertEqual(navigation.chatPath, [.conversation("hit-9"), .conversation("hit-7")])
    }
}

// MARK: - Everyone plane

final class MarbleMentionTests: XCTestCase {
    private let names = ["Ada", "Ben", "Marble"]

    func testTokenOnlyAtWordStart() {
        XCTAssertNil(MarbleMentions.token(in: "email a@b"))
        XCTAssertEqual(MarbleMentions.token(in: "hi @ad"), "ad")
        XCTAssertEqual(MarbleMentions.token(in: "@"), "")
        XCTAssertNil(MarbleMentions.token(in: "@ada done"))
    }

    func testCompletionsArePrefixAndCaseInsensitive() {
        XCTAssertEqual(
            MarbleMentions.completions(in: "ask @a", names: names),
            ["Ada"]
        )
        XCTAssertEqual(
            MarbleMentions.completions(in: "@EV", names: names),
            ["everyone"]
        )
        XCTAssertEqual(
            MarbleMentions.completions(in: "@", names: names),
            ["Ada", "Ben", "Marble", "everyone"]
        )
        XCTAssertTrue(MarbleMentions.completions(in: "no at", names: names).isEmpty)
    }

    func testApplyingACompletionReplacesTheToken() {
        XCTAssertEqual(
            MarbleMentions.applying("Ben", to: "hey @b"),
            "hey @Ben "
        )
        XCTAssertEqual(MarbleMentions.applying("Ada", to: "plain"), "plain")
    }

    func testMentionsAreWholeWord() {
        XCTAssertEqual(MarbleMentions.mentions(in: "@ada hi", names: names), ["Ada"])
        XCTAssertEqual(MarbleMentions.mentions(in: "radar @adaptive", names: names), [])
        XCTAssertEqual(
            MarbleMentions.mentions(in: "@everyone look", names: names),
            names
        )
        XCTAssertEqual(
            Set(MarbleMentions.mentions(in: "@Ada and @ben", names: names)),
            Set(["Ada", "Ben"])
        )
    }

    func testNextHueFillsTheLargestGap() {
        XCTAssertEqual(MarbleMentions.nextHue(used: []), 40)
        XCTAssertEqual(MarbleMentions.nextHue(used: [0, 180]), 90)
        let hue = MarbleMentions.nextHue(used: [10, 20, 30])
        XCTAssertFalse([10, 20, 30].contains(hue))
    }
}

final class MarbleHandoffLineTests: XCTestCase {
    private func handoff(_ status: String, summary: String = "") -> MarbleHandoff {
        MarbleHandoff(
            from: "m-ada", fromName: "Ada", to: "m-ben", toName: "Ben",
            instruction: "check the failing tests", status: status, summary: summary
        )
    }

    func testAskedReadsPlainly() {
        XCTAssertEqual(
            MarbleHandoffLine.render(handoff("asked")),
            "Ada asked Ben to check the failing tests."
        )
    }

    func testFinishedCarriesTheReply() {
        XCTAssertEqual(
            MarbleHandoffLine.render(handoff("finished", summary: "three tests were flaky")),
            "Ben finished: three tests were flaky"
        )
        XCTAssertTrue(MarbleHandoffLine.isExpandable(handoff("finished", summary: "x")))
    }

    func testQuietRowTitleAndDetail() {
        XCTAssertEqual(MarbleHandoffLine.title(handoff("asked")), "Ada asked Ben")
        XCTAssertEqual(MarbleHandoffLine.detail(handoff("asked")), "check the failing tests")
        let finished = handoff("finished", summary: "three tests were flaky")
        XCTAssertEqual(MarbleHandoffLine.title(finished), "Ben replied to Ada")
        XCTAssertEqual(MarbleHandoffLine.detail(finished), "three tests were flaky")
        XCTAssertEqual(MarbleHandoffLine.title(handoff("refused")), "Ben couldn't take it")
        XCTAssertEqual(MarbleHandoffLine.title(handoff("failed")), "Ben couldn't finish")
    }

    func testRefusedAndFailed() {
        XCTAssertEqual(
            MarbleHandoffLine.render(handoff("refused", summary: "busy with another task")),
            "Ben couldn't take it: busy with another task"
        )
        XCTAssertEqual(
            MarbleHandoffLine.render(handoff("failed", summary: "engine stopped")),
            "Ben couldn't finish: engine stopped"
        )
    }
}

final class MarbleSpeakerStyleTests: XCTestCase {
    private func marble(_ id: String, _ name: String, _ hue: Int) -> MarblePrismMarble {
        MarblePrismMarble(
            id: id, name: name, role: "", glyph: "circle", hue: hue,
            workspace: "", state: "idle", conversationId: "", taskId: "",
            lastAt: 0, runs: 0, lastError: ""
        )
    }

    func testAssistantRowsTakeTheMarblesColour() {
        let marbles = [marble("m-ada", "Ada", 45)]
        let entry = MarbleEntry(
            conversationId: "everyone", kind: "assistant", text: "hi",
            taskId: "t", at: 1, speaker: "m-ada", speakerName: "Ada"
        )
        let style = MarbleSpeakerStyle.for(entry: entry, marbles: marbles)
        XCTAssertEqual(style?.name, "Ada")
        XCTAssertEqual(style?.hue, 45)
    }

    func testTheDefaultAssistantGetsItsOwnDot() {
        let entry = MarbleEntry(
            conversationId: "everyone", kind: "assistant", text: "hi",
            taskId: "t", at: 1, speaker: "marble", speakerName: "Marble"
        )
        let style = MarbleSpeakerStyle.for(entry: entry, marbles: [])
        XCTAssertEqual(style?.hue, MarbleSpeakerStyle.defaultHue)
    }

    func testUserAndSystemRowsHaveNoLabel() {
        let user = MarbleEntry(
            conversationId: "everyone", kind: "user", text: "hi",
            taskId: "", at: 1, speaker: "user", speakerName: "You"
        )
        XCTAssertNil(MarbleSpeakerStyle.for(entry: user, marbles: []))
    }
}

final class EveryoneDecodingTests: XCTestCase {
    func testEveryoneEntriesDecodeSpeakersAndHandoffs() throws {
        let json = """
        {"entries":[
          {"conversationId":"everyone","kind":"user","text":"@Ada hi","taskId":"",
           "at":100,"speaker":"user","speakerName":"You"},
          {"conversationId":"everyone","kind":"assistant","text":"Hello!","taskId":"t-1",
           "at":101,"speaker":"m-ada","speakerName":"Ada"},
          {"conversationId":"everyone","kind":"handoff","text":"Ada asked Ben to check.",
           "taskId":"t-1","at":102,"speaker":"m-ada","speakerName":"Ada",
           "handoff":{"from":"m-ada","fromName":"Ada","to":"m-ben","toName":"Ben",
             "instruction":"check","status":"asked","summary":""}}
        ]}
        """
        struct Reply: Decodable { var entries: [MarbleEntry] }
        let reply = try JSONDecoder().decode(Reply.self, from: Data(json.utf8))
        XCTAssertEqual(reply.entries.count, 3)
        XCTAssertEqual(reply.entries[1].speaker, "m-ada")
        XCTAssertEqual(reply.entries[1].speakerName, "Ada")
        XCTAssertEqual(reply.entries[2].handoff?.status, "asked")
        XCTAssertEqual(reply.entries[2].handoff?.toName, "Ben")
    }

    func testRosterDecodesPersonaAndTheDefaultEntry() throws {
        let json = """
        {"marbles":[
          {"id":"marble","name":"Marble","role":"the default assistant",
           "glyph":"circle","hue":210,"workspace":"","state":"idle",
           "conversationId":"","taskId":"","lastAt":0,"runs":0,"lastError":"",
           "persona":"kind","isDefault":true},
          {"id":"m-ada","name":"Ada","role":"","glyph":"star","hue":45,
           "workspace":"","state":"working","conversationId":"c-1","taskId":"t",
           "lastAt":1,"runs":2,"lastError":"","persona":"warm","isDefault":false}
        ]}
        """
        let prism = try JSONDecoder().decode(MarblePrism.self, from: Data(json.utf8))
        XCTAssertEqual(prism.marbles.count, 2)
        XCTAssertTrue(prism.marbles[0].isDefault)
        XCTAssertEqual(prism.marbles[0].persona, "kind")
        XCTAssertEqual(prism.marbles[1].tagline, "warm")
        XCTAssertEqual(prism.marbles[1].displayState(presence: nil), "Working…")
    }

    func testMemoryFactDecodesAnOwner() throws {
        let json = """
        {"id":"f-1","text":"Ada likes tea","kind":"fact","createdAt":1,"uses":0,
         "owner":"m-ada"}
        """
        let fact = try JSONDecoder().decode(MarbleMemoryFact.self, from: Data(json.utf8))
        XCTAssertEqual(fact.owner, "m-ada")
        // An older host leaves owner off; the fact is shared.
        let older = """
        {"id":"f-2","text":"shared","kind":"fact","createdAt":1,"uses":0}
        """
        XCTAssertEqual(
            try JSONDecoder().decode(MarbleMemoryFact.self, from: Data(older.utf8)).owner,
            ""
        )
    }
}

final class MarbleVisionNavigationTests: XCTestCase {
    func testLaunchLandsOnEveryone() {
        var nav = MarbleNavigation()
        nav.launch()
        XCTAssertEqual(nav.tab, .everyone)
    }

    func testRosterRowOpensTheRightDestination() {
        var nav = MarbleNavigation()
        nav.openMarble("m-ada")
        XCTAssertEqual(nav.tab, .marbles)
        XCTAssertEqual(nav.chatPath, [.marbleChat("m-ada")])
        nav.openMarble("marble")
        XCTAssertEqual(nav.chatPath.last, .marbleChats)
    }

    func testMarbleInfoFromEveryoneUsesTheEveryoneStack() {
        var nav = MarbleNavigation()
        nav.openMarbleInfo("m-ben", fromEveryone: true)
        XCTAssertEqual(nav.everyonePath, [.marbleInfo("m-ben")])
        XCTAssertTrue(nav.chatPath.isEmpty)
        XCTAssertEqual(nav.tab, .everyone)
    }
}

final class MarbleConnectionGenerationTests: XCTestCase {
    func testReplyFromAnEarlierHostIsNotCurrent() {
        var connection = MarbleConnectionGeneration()
        let before = connection.current
        connection.advance()
        XCTAssertFalse(connection.isCurrent(before))
        XCTAssertTrue(connection.isCurrent(connection.current))
    }

    func testEverySwitchRetiresTheLastToken() {
        var connection = MarbleConnectionGeneration()
        connection.advance()
        let signedIn = connection.current
        connection.advance()
        connection.advance()
        XCTAssertFalse(connection.isCurrent(signedIn))
    }
}

final class MarbleEveryoneTimelineTests: XCTestCase {
    private let timeline = [
        MarbleEntry(
            conversationId: "everyone", kind: "user", text: "hi",
            taskId: "", at: 1, speaker: "", speakerName: ""
        )
    ]

    private func event(_ json: String) throws -> MarbleEvent {
        try JSONDecoder().decode(MarbleEvent.self, from: Data(json.utf8))
    }

    func testHostClearEmptiesTheTimeline() throws {
        let cleared = try event(
            #"{"sequence":9,"conversationId":"everyone","taskId":"","kind":"everyone","data":{"cleared":2}}"#
        )
        XCTAssertTrue(MarbleEveryoneTimeline.isChange(cleared))
        XCTAssertEqual(cleared.data.cleared, 2)
        XCTAssertTrue(MarbleEveryoneTimeline.applying(cleared, to: timeline).isEmpty)
    }

    func testNewPostKeepsTheTimelineUntilRefetched() throws {
        let posted = try event(
            #"{"sequence":10,"conversationId":"everyone","taskId":"","kind":"everyone","data":{"kind":"user"}}"#
        )
        XCTAssertTrue(MarbleEveryoneTimeline.isChange(posted))
        XCTAssertEqual(MarbleEveryoneTimeline.applying(posted, to: timeline), timeline)
    }

    func testOtherEventsAreNotEveryoneChanges() throws {
        let text = try event(
            #"{"sequence":11,"conversationId":"everyone","taskId":"t","kind":"text","data":{"text":"x"}}"#
        )
        XCTAssertFalse(MarbleEveryoneTimeline.isChange(text))
        XCTAssertEqual(MarbleEveryoneTimeline.applying(text, to: timeline), timeline)
    }
}

final class MarbleMarkdownTests: XCTestCase {
    func testQuoteListsTableCodeAndLink() {
        let blocks = MarbleMarkdown.parse("""
        Summary:

        > Ship it first.

        - Fix the composer
          and its background
        - [x] Render **Markdown**

        1. One
        2. Two

        | Area | State |
        |:---|---:|
        | Chat | ready |

        ```swift
        let tint = Color.accentColor
        ```

        See [the docs](https://example.com).
        """)
        XCTAssertEqual(blocks, [
            .paragraph("Summary:"),
            .quote([.paragraph("Ship it first.")]),
            .list(ordered: false, items: [
                MarbleMarkdownListItem(marker: "•", level: 0, text: "Fix the composer\nand its background", checked: nil),
                MarbleMarkdownListItem(marker: "", level: 0, text: "Render **Markdown**", checked: true),
            ]),
            .list(ordered: true, items: [
                MarbleMarkdownListItem(marker: "1.", level: 0, text: "One", checked: nil),
                MarbleMarkdownListItem(marker: "2.", level: 0, text: "Two", checked: nil),
            ]),
            .table(header: ["Area", "State"], alignments: [.leading, .trailing], rows: [["Chat", "ready"]]),
            .code(language: "swift", code: "let tint = Color.accentColor"),
            .paragraph("See [the docs](https://example.com)."),
        ])
    }

    func testHeadingsRulesAndNestedQuotes() {
        XCTAssertEqual(MarbleMarkdown.parse("## Plan ##\n---\n> > deep\n> - item"), [
            .heading(level: 2, text: "Plan"),
            .rule,
            .quote([
                .quote([.paragraph("deep")]),
                .list(ordered: false, items: [MarbleMarkdownListItem(marker: "•", level: 0, text: "item", checked: nil)]),
            ]),
        ])
    }

    func testPlainTextWithPipesIsNotATable() {
        XCTAssertEqual(MarbleMarkdown.parse("a | b\nnext line"), [.paragraph("a | b\nnext line")])
    }

    func testUnclosedFenceKeepsTheRestAsCode() {
        XCTAssertEqual(MarbleMarkdown.parse("```\nx\ny"), [.code(language: "", code: "x\ny")])
    }

    func testLongerFenceKeepsShorterFencesAsCode() {
        XCTAssertEqual(
            MarbleMarkdown.parse("````markdown\n```swift\nlet x = 1\n```\n````\nafter"),
            [
                .code(language: "markdown", code: "```swift\nlet x = 1\n```"),
                .paragraph("after"),
            ]
        )
    }

    func testFenceClosesOnlyOnTheSameCharacter() {
        XCTAssertEqual(
            MarbleMarkdown.parse("~~~\n```\nx\n~~~~"),
            [.code(language: "", code: "```\nx")]
        )
        XCTAssertEqual(
            MarbleMarkdown.parse("```\n``` not a close\n```"),
            [.code(language: "", code: "``` not a close")]
        )
    }

    func testShortTableRowsArePadded() {
        XCTAssertEqual(MarbleMarkdown.parse("|a|b|\n|-|-|\n|1|"), [
            .table(header: ["a", "b"], alignments: [.leading, .leading], rows: [["1", ""]])
        ])
    }

    func testNestedListLevelComesFromIndent() {
        guard case let .list(_, items) = MarbleMarkdown.parse("- a\n  - b\n    - c").first else {
            return XCTFail("expected a list")
        }
        XCTAssertEqual(items.map(\.level), [0, 1, 2])
    }
}

final class MarbleDecisionWordsTests: XCTestCase {
    func testKnownToolsReadAsActions() {
        XCTAssertEqual(MarbleDecisionWords.action(tool: "glass.workspace.write"), "Write to workspace")
        XCTAssertEqual(MarbleDecisionWords.action(tool: "glass.workspace.list"), "List workspace files")
        XCTAssertEqual(MarbleDecisionWords.action(tool: "glass.marble.ask"), "Ask another Marble")
    }

    func testUnknownToolsAreSpelledOut() {
        XCTAssertEqual(MarbleDecisionWords.action(tool: "glass.not_real.thing"), "Not real thing")
        XCTAssertEqual(MarbleDecisionWords.action(tool: "shell"), "Shell")
        XCTAssertEqual(MarbleDecisionWords.action(tool: ""), "Use a tool")
    }

    func testHostReasonsBecomePlainOutcomes() {
        XCTAssertEqual(MarbleDecisionWords.outcome(decision: "denied", reason: "declined on this surface"), "Denied by you")
        XCTAssertEqual(MarbleDecisionWords.outcome(decision: "approved", reason: "approved on this surface"), "Allowed by you")
        XCTAssertEqual(MarbleDecisionWords.outcome(decision: "denied", reason: "cancelled before this tool call ran"), "Cancelled before it ran")
        XCTAssertEqual(MarbleDecisionWords.outcome(decision: "denied", reason: ".env matches the denied path rule .env"), "Blocked: off-limits folder")
        XCTAssertEqual(MarbleDecisionWords.outcome(decision: "allowed", reason: "grant g1 already allows glass.workspace.write in src"), "Allowed by a saved permission")
        XCTAssertEqual(MarbleDecisionWords.outcome(decision: "denied", reason: "the observe profile allows reads only"), "Blocked: read-only mode")
    }

    func testPolicyReasonsAreWordedByTheDecision() {
        let reason = "the observe profile allows reads only"
        XCTAssertEqual(MarbleDecisionWords.outcome(decision: "allowed", reason: reason), "Allowed in read-only mode")
        XCTAssertEqual(MarbleDecisionWords.outcome(decision: "denied", reason: reason), "Blocked: read-only mode")
        XCTAssertEqual(
            MarbleDecisionWords.outcome(decision: "denied", reason: "the observe profile is read-only, so glass.workspace.write cannot change the workspace"),
            "Blocked: read-only mode"
        )
    }

    func testUnknownReasonsFallBackToTheDecision() {
        XCTAssertEqual(MarbleDecisionWords.outcome(decision: "approved", reason: "human"), "Allowed by you")
        XCTAssertEqual(MarbleDecisionWords.outcome(decision: "allowed", reason: ""), "Allowed automatically")
        XCTAssertEqual(MarbleDecisionWords.outcome(decision: "denied", reason: "something new"), "Denied")
        XCTAssertTrue(MarbleDecisionWords.wentAhead(decision: "allowed"))
        XCTAssertFalse(MarbleDecisionWords.wentAhead(decision: "denied"))
    }
}

final class MarbleResumeTests: XCTestCase {
    func testResumesFromTheSavedCursor() {
        XCTAssertEqual(MarbleResumeCursor.resume(saved: 40, hostSequence: 52, previousPID: 1, hostPID: 1),
                       .init(since: 40, restarted: false))
    }

    func testSequenceRewindReplaysFromZeroWhenHostDoesNotReportPID() {
        XCTAssertEqual(MarbleResumeCursor.resume(saved: 900, hostSequence: 12, previousPID: nil, hostPID: nil),
                       .init(since: 0, restarted: true))
    }

    func testNewPIDReplaysFromZeroEvenAfterPassingTheOldCursor() {
        XCTAssertEqual(MarbleResumeCursor.resume(saved: 40, hostSequence: 52, previousPID: 1, hostPID: 2),
                       .init(since: 0, restarted: true))
    }

    func testAnOlderSavedCursorWithNoPIDReplaysOnce() {
        XCTAssertEqual(MarbleResumeCursor.resume(saved: 40, hostSequence: 52, previousPID: nil, hostPID: 2),
                       .init(since: 0, restarted: true))
        XCTAssertEqual(MarbleResumeCursor.resume(saved: 0, hostSequence: 52, previousPID: nil, hostPID: 2),
                       .init(since: 0, restarted: false))
    }

    func testReturningToTheAppDoesNotFlashOffline() {
        var status = MarbleConnectionStatus()
        status.refreshSucceeded()
        status.suspend()
        status.streamEnded("The network connection was lost.")
        XCTAssertTrue(status.isConnected)
        status.resume()
        XCTAssertFalse(status.refreshFailed("timed out"), "first failure after resume is retried")
        XCTAssertTrue(status.isConnected)
        XCTAssertTrue(status.refreshFailed("timed out"))
        XCTAssertTrue(status.isOffline)
    }

    func testResumeFromOfflineReadsConnecting() {
        var status = MarbleConnectionStatus()
        status.refreshFailed("down")
        status.resume()
        XCTAssertEqual(status.label, "Connecting")
        status.streamOpened()
        XCTAssertTrue(status.isConnected)
    }

    func testChatCacheRoundTripsPerHost() throws {
        let entry = MarbleEntry(
            conversationId: "everyone", kind: "assistant", text: "> hi", taskId: "t",
            at: 3, speaker: "marble", speakerName: "Marble"
        )
        let cache = MarbleChatCache(everyone: [entry], transcripts: ["c": [entry]])
        XCTAssertEqual(MarbleChatCache.decoded(try cache.encoded()), cache)
        XCTAssertNil(MarbleChatCache.decoded(Data("nope".utf8)))
        XCTAssertNotEqual(
            MarbleChatCache.fileName(for: "http://a:4783"),
            MarbleChatCache.fileName(for: "http://b:4783")
        )
        XCTAssertTrue(MarbleChatCache.fileName(for: "http://a:1").hasPrefix("marble-chats-"))
    }

    func testDeletedConversationIsAbsentFromRewrittenDiskCache() throws {
        let deleted = MarbleEntry(conversationId: "deleted", kind: "user", text: "private draft", taskId: "", at: 1)
        let kept = MarbleEntry(conversationId: "kept", kind: "assistant", text: "keep this", taskId: "", at: 2)
        let everyone = MarbleEntry(conversationId: "everyone", kind: "assistant", text: "shared", taskId: "", at: 3)
        var cache = MarbleChatCache(everyone: [everyone], transcripts: ["deleted": [deleted], "kept": [kept]])
        let url = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: url) }
        try cache.encoded().write(to: url, options: .atomic)

        cache.removeConversation("deleted")
        try cache.encoded().write(to: url, options: .atomic)

        let restored = try XCTUnwrap(MarbleChatCache.decoded(Data(contentsOf: url)))
        XCTAssertEqual(restored.transcripts, ["kept": [kept]])
        XCTAssertEqual(restored.everyone, [everyone])
    }

    func testCacheFileNamesNeverCollide() {
        XCTAssertNotEqual(
            MarbleChatCache.fileName(for: "http://a-b.example:4783"),
            MarbleChatCache.fileName(for: "http://a.b.example:4783")
        )
        XCTAssertEqual(
            MarbleChatCache.fileName(for: "HTTP://Host.Example:4783/"),
            MarbleChatCache.fileName(for: "http://host.example:4783")
        )
        let name = MarbleChatCache.fileName(for: "http://a:1")
        XCTAssertEqual(name.count, "marble-chats-".count + 64 + ".json".count)
    }
}

final class MarbleEveryoneThinkingTests: XCTestCase {
    func testIdleDefaultMarbleDoesNotInheritNamedAgentWork() throws {
        let json = #"{"id":"marble","name":"Marble","state":"working","isWorking":false,"activeTaskIds":[]}"#
        let marble = try JSONDecoder().decode(MarblePrismMarble.self, from: Data(json.utf8))
        XCTAssertFalse(marble.isWorking())
        XCTAssertEqual(marble.displayState(presence: nil), "Idle")
        XCTAssertEqual(marble.activeTaskIds(presence: nil), [])
    }

    func testAgentPresenceSuppliesTaskIDsOnOlderRoster() throws {
        let json = #"{"id":"ada","name":"Ada","state":"idle"}"#
        let ada = try JSONDecoder().decode(MarblePrismMarble.self, from: Data(json.utf8))
        let presence = MarbleAgentPresence(status: "working", activeTaskIds: ["t1"])
        XCTAssertTrue(ada.isWorking(presence: presence))
        XCTAssertEqual(ada.activeTaskIds(presence: presence), ["t1"])
    }

    func testThinkingStaysUntilTheSameTaskHasAnOutcome() {
        XCTAssertTrue(MarbleEveryoneTimeline.showsThinking(
            working: true, activeTaskIds: ["t1"], previousTaskIds: [], visibleOutcomes: ["t2"]
        ))
        XCTAssertTrue(MarbleEveryoneTimeline.showsThinking(
            working: false, activeTaskIds: [], previousTaskIds: ["t1"], visibleOutcomes: ["t2"]
        ))
        XCTAssertFalse(MarbleEveryoneTimeline.showsThinking(
            working: false, activeTaskIds: [], previousTaskIds: ["t1"], visibleOutcomes: ["t1"]
        ))
        XCTAssertFalse(MarbleEveryoneTimeline.showsThinking(
            working: false, activeTaskIds: [], previousTaskIds: [], visibleOutcomes: []
        ))
    }

    func testEveryoneReplyDecodesBothShapes() throws {
        let named = try JSONDecoder().decode(
            MarbleEveryoneReply.self,
            from: Data(#"{"ok":true,"answering":["Ada"],"busy":[]}"#.utf8)
        )
        XCTAssertEqual(named, MarbleEveryoneReply(ok: true, answering: ["Ada"], busy: []))
        let turn = try JSONDecoder().decode(
            MarbleEveryoneReply.self,
            from: Data(#"{"accepted":true,"taskId":"t1","conversationId":"c","queued":false}"#.utf8)
        )
        XCTAssertEqual(turn.taskId, "t1")
        XCTAssertTrue(turn.ok)
        XCTAssertEqual(turn.answering, [])
    }
}

final class MarbleSteerTests: XCTestCase {
    func testNamedMarblesOfferStopOnlyMidTurn() {
        XCTAssertEqual(MarbleSteer.action(isWorking: true, canSteer: false), .stopOnly)
        XCTAssertEqual(MarbleSteer.action(isWorking: true, canSteer: true), .steer)
        XCTAssertEqual(MarbleSteer.action(isWorking: false, canSteer: false), .send)
    }

}

final class MarbleDraftTests: XCTestCase {
    func testAnUnacceptedSendKeepsTheDraftVerbatim() {
        let text = "  Try again later\n"
        XCTAssertEqual(MarbleDraft.draft(after: text, current: text, accepted: false), text)
    }

    func testAnAcceptedSendClearsOnlyTheSubmittedText() {
        XCTAssertEqual(MarbleDraft.draft(after: "fix it", current: "fix it", accepted: true), "")
        XCTAssertEqual(MarbleDraft.draft(after: "fix it", current: "fix it now", accepted: true), "fix it now")
    }
}
