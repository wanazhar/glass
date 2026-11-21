import Foundation
import XCTest
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
@testable import MarbleCore

/// Live checks against a real host, using the same Core request builder,
/// routes, and probe the app itself uses. They run only when MARBLE_LIVE_HOST
/// and MARBLE_TOKEN_FILE are set; the token is read from the file and never
/// printed.
final class MarbleLiveTests: XCTestCase {
    private var host: String!
    private var token: String!
    private var session: URLSession!

    override func setUp() async throws {
        let environment = ProcessInfo.processInfo.environment
        guard
            let host = environment["MARBLE_LIVE_HOST"], !host.isEmpty,
            let tokenFile = environment["MARBLE_TOKEN_FILE"],
            let raw = try? String(contentsOfFile: tokenFile, encoding: .utf8)
        else {
            throw XCTSkip("live host not configured (MARBLE_LIVE_HOST, MARBLE_TOKEN_FILE)")
        }
        self.host = host
        self.token = MarbleToken.normalize(raw)
        XCTAssertFalse(token.isEmpty)
        let configuration = URLSessionConfiguration.default
        configuration.timeoutIntervalForRequest = 30
        session = URLSession(configuration: configuration)
    }

    // MARK: - Plumbing shared with the app's client

    private func endpoint(token: String?) throws -> MarbleEndpoint {
        guard case .success(var endpoint) = MarbleEndpoint.parse(host) else {
            throw MarbleClientError.invalidURL
        }
        let normalized = token.map(MarbleToken.normalize) ?? ""
        endpoint.token = normalized.isEmpty ? nil : normalized
        return endpoint
    }

    private func fetch(
        _ route: MarbleRoute,
        token: String? = nil
    ) async throws -> Data {
        let request = try endpoint(token: token ?? self.token).request(
            path: route.path, query: route.query
        )
        do {
            let (data, response) = try await session.data(for: request)
            try marbleValidate(response, data: data)
            return data
        } catch let error as MarbleClientError {
            throw error
        } catch {
            throw MarbleClientError.from(error)
        }
    }

    private func fetchJSON<T: Decodable>(
        _ type: T.Type, _ route: MarbleRoute, token: String? = nil
    ) async throws -> T {
        let data = try await fetch(route, token: token)
        do {
            return try JSONDecoder().decode(T.self, from: data)
        } catch {
            throw MarbleClientError.decoding(String(describing: error))
        }
    }

    /// Same health() semantics as MarbleClient: a 503 still decodes its body.
    private func health(endpoint: MarbleEndpoint) async throws -> MarbleHealth {
        let request = try endpoint.request(path: MarbleRoute.health.path)
        do {
            let (data, response) = try await session.data(for: request)
            let status = (response as? HTTPURLResponse)?.statusCode ?? 0
            if status == 503 {
                return try JSONDecoder().decode(MarbleHealth.self, from: data)
            }
            try marbleValidate(response, data: data)
            return try JSONDecoder().decode(MarbleHealth.self, from: data)
        } catch let error as MarbleClientError {
            throw error
        } catch let error as DecodingError {
            throw MarbleClientError.decoding(String(describing: error))
        } catch {
            throw MarbleClientError.from(error)
        }
    }

    private func probe(host: String, token: String) async -> (MarbleOnboarding, MarbleEndpoint) {
        switch MarbleEndpoint.parse(host) {
        case .failure:
            return (.unreachable("bad address"), MarbleEndpoint(baseURL: URL(string: host)!, token: nil))
        case .success(let endpoint):
            let normalized = MarbleToken.normalize(token)
            var authed = endpoint
            authed.token = normalized.isEmpty ? nil : normalized
            return await MarbleOnboardingEvaluator.probe(
                endpoint: authed,
                health: { try await self.health(endpoint: authed) },
                state: { try await self.fetchJSON(MarbleState.self, .state, token: authed.token ?? "") }
            )
        }
    }

    // MARK: - Every GET route answers

    func testEveryGetRouteAnswers() async throws {
        let health = try await fetchJSON(MarbleHealth.self, .health)
        XCTAssertEqual(health.ok, true)
        XCTAssertNotNil(health.version)

        _ = try await fetchJSON(MarbleState.self, .state)
        _ = try await fetchJSON(MarblePresence.self, .presence)
        _ = try await fetchJSON(MarbleMemory.self, .memory)
        _ = try await fetchJSON(MarbleRoutines.self, .routines)
        _ = try await fetchJSON(MarblePrism.self, .prism)
        _ = try await fetchJSON(MarbleToolInventory.self, .tools)
        _ = try await fetchJSON(MarbleVoice.self, .voice)

        struct DevicesReply: Decodable { var devices: [MarbleDevice] }
        _ = try await fetchJSON(DevicesReply.self, .devices)

        // The crate map may legitimately 503 when the host has no workspace
        // map; anything else is a routing bug.
        do {
            _ = try await fetchJSON(MarbleCrateMap.self, .crates)
        } catch MarbleClientError.server(503, _) {
            // documented
        }

        // Search and transcript need a real conversation from state.
        let state = try await fetchJSON(MarbleState.self, .state)
        if let conversation = state.conversations.first {
            _ = try await fetchJSON(
                MarbleTranscriptPage.self, .transcript(conversationId: conversation.id)
            )
            let export = try await fetch(.export(conversationId: conversation.id))
            XCTAssertTrue(String(data: export, encoding: .utf8)?.contains("glass-marble-conversation") ?? false)
        }
        struct SearchReply: Decodable { var hits: [MarbleSearchHit] }
        _ = try await fetchJSON(SearchReply.self, .search("the"))

        _ = try await fetch(.export())
    }

    // MARK: - Onboarding verdicts against the live host

    func testCorrectTokenIsReady() async throws {
        let (verdict, _) = await probe(host: host, token: token)
        guard case .ready = verdict else {
            return XCTFail("expected .ready, got \(verdict.message)")
        }
    }

    func testWrongTokenIsBadToken() async throws {
        let (verdict, _) = await probe(host: host, token: "definitely-not-the-token")
        XCTAssertEqual(verdict, .badToken, verdict.message)
    }

    func testMessyPasteStillConnects() async throws {
        for pasted in [
            "  \(token!)\n",
            "\(token!.prefix(8)) \(token!.dropFirst(8))",
            "Bearer \(token!)",
            "\"\(token!)\"",
        ] {
            let (verdict, _) = await probe(host: host, token: pasted)
            guard case .ready = verdict else {
                return XCTFail("expected .ready for a pasted variant, got \(verdict.message)")
            }
        }
    }

    func testNoTokenSaysTokenRequired() async throws {
        let (verdict, _) = await probe(host: host, token: "")
        XCTAssertEqual(verdict, .tokenRequired, verdict.message)
    }

    func testWrongRouteIsNotUnreachable() async throws {
        let authed = try endpoint(token: token)
        let request = try authed.request(path: "/api/nope")
        let (data, response) = try await session.data(for: request)
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        XCTAssertEqual(status, 404)
        let outcome = MarbleOnboardingEvaluator.stateOutcome(
            from: MarbleClientError.status(status, body: String(data: data, encoding: .utf8) ?? "")
        )
        XCTAssertEqual(outcome, .http(404, "not_found"))
        let health = try await self.health(endpoint: authed)
        let verdict = MarbleOnboardingEvaluator.evaluate(
            urlTried: authed.baseURL.absoluteString,
            health: .success(health),
            state: outcome,
            hadToken: true
        )
        guard case .hostAnswered(let message) = verdict else {
            return XCTFail("expected .hostAnswered, got \(verdict.message)")
        }
        XCTAssertTrue(message.contains("404"), message)
    }

    func testUnusedPortIsUnreachable() async throws {
        let (verdict, _) = await probe(host: "http://100.98.115.95:4799", token: token)
        guard case .unreachable(let message) = verdict else {
            return XCTFail("expected .unreachable, got \(verdict.message)")
        }
        XCTAssertTrue(message.contains("Cannot reach"), message)
    }
}
