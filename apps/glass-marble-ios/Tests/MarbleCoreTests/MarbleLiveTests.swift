import Foundation
import XCTest
#if canImport(FoundationNetworking)
import FoundationNetworking
#endif
#if canImport(Glibc)
import Glibc
#else
import Darwin
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
                health: { try await self.health(endpoint: $0) },
                state: { try await self.fetchJSON(MarbleState.self, .state, token: $0.token ?? "") }
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

extension MarbleLiveTests {
    /// The real pinning check: fetch the leaf certificate over TLS with
    /// openssl, hash it locally, then verify the host's proof with the token —
    /// the same Core code the app uses.
    func testTLSProofVerifiesObservedCertificate() async throws {
        guard let tlsHost = ProcessInfo.processInfo.environment["MARBLE_LIVE_TLS_HOST"],
              !tlsHost.isEmpty,
              let url = URL(string: tlsHost),
              let host = url.host,
              let port = url.port
        else {
            throw XCTSkip("MARBLE_LIVE_TLS_HOST not configured")
        }
        // Leaf DER hash, taken from the certificate the client actually saw.
        let process = Process()
        let pipe = Pipe()
        process.executableURL = URL(fileURLWithPath: "/bin/sh")
        process.arguments = [
            "-c",
            "openssl s_client -connect \(host):\(port) -servername \(host) </dev/null 2>/dev/null | openssl x509 -outform DER",
        ]
        process.standardOutput = pipe
        try process.run()
        let der = pipe.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()
        guard !der.isEmpty else {
            return XCTFail("openssl produced no certificate")
        }
        let observed = MarbleSHA256.hex(der)

        // tls-proof over plain HTTP must refuse.
        let http = URL(string: "http://\(host):\(port)")!
        var plainRequest = URLRequest(
            url: http.appendingPathComponent("api/tls-proof")
        )
        plainRequest.setValue("application/json", forHTTPHeaderField: "Accept")
        let (_, plainResponse) = try await session.data(for: plainRequest)
        XCTAssertEqual((plainResponse as? HTTPURLResponse)?.statusCode, 400)

        // The proof itself, over TLS (system trust accepts the ts.net cert).
        var proofURL = URLComponents(url: url.appendingPathComponent("api/tls-proof"),
                                     resolvingAgainstBaseURL: false)!
        let nonce = MarbleTLSPin.nonce()
        proofURL.queryItems = [URLQueryItem(name: "nonce", value: nonce)]
        var request = URLRequest(url: proofURL.url!)
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        let (data, response) = try await session.data(for: request)
        XCTAssertEqual((response as? HTTPURLResponse)?.statusCode, 200)
        struct Proof: Decodable { var certSha256: String; var proof: String }
        let reply = try JSONDecoder().decode(Proof.self, from: data)
        XCTAssertEqual(reply.certSha256, observed)
        XCTAssertTrue(
            MarbleTLSPin.verify(
                token: token, nonce: nonce,
                observedCertSHA256: observed, proofHex: reply.proof
            )
        )
        XCTAssertFalse(
            MarbleTLSPin.verify(
                token: "not-the-token", nonce: nonce,
                observedCertSHA256: observed, proofHex: reply.proof
            )
        )
    }

    // MARK: - SSE status: the real stream through the app's real pipeline

    /// Linux's URLSession has no streaming `bytes(for:)`, and a buffered
    /// `data(for:)` never returns for SSE. A plain TCP socket gives the same
    /// bytes the app's splitter consumes — http hosts only (an https host
    /// skips: the pin check in the app covers trust, not framing).
    private struct EventSocket {
        var fd: Int32 = -1
        var statusCode = 0
        /// Leftover bytes after the response head.
        var rest: [UInt8] = []

        init(host: String, token: String) throws {
            guard let url = URLComponents(string: host),
                  url.scheme == "http",
                  let name = url.host
            else {
                throw XCTSkip("socket streaming test needs an http host")
            }
            let port = UInt16(url.port ?? 80)
            var hints = addrinfo()
            hints.ai_family = AF_INET
            #if canImport(Darwin)
            hints.ai_socktype = SOCK_STREAM
            #else
            hints.ai_socktype = Int32(SOCK_STREAM.rawValue)
            #endif
            var info: UnsafeMutablePointer<addrinfo>?
            guard getaddrinfo(name, String(port), &hints, &info) == 0, let info else {
                throw MarbleClientError.invalidURL
            }
            defer { freeaddrinfo(info) }
            fd = socket(info.pointee.ai_family, info.pointee.ai_socktype, info.pointee.ai_protocol)
            guard fd >= 0, connect(fd, info.pointee.ai_addr, info.pointee.ai_addrlen) == 0 else {
                throw MarbleClientError.unreachable("could not open the event socket")
            }
            var timeout = timeval(tv_sec: 30, tv_usec: 0)
            setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size))
            let request = "GET /api/events?since=0 HTTP/1.1\r\n"
                + "Host: \(name):\(port)\r\n"
                + "Authorization: Bearer \(token)\r\n"
                + "Accept: text/event-stream\r\n"
                + "Connection: close\r\n\r\n"
            _ = request.utf8.withContiguousStorageIfAvailable { send(fd, $0.baseAddress, $0.count, 0) }
            // Read until the response head is complete.
            var head: [UInt8] = []
            var buffer = [UInt8](repeating: 0, count: 8192)
            let marker = Array<UInt8>([13, 10, 13, 10])
            var headEnd = 0
            while headEnd == 0 {
                let count = recv(fd, &buffer, buffer.count, 0)
                guard count > 0 else { throw MarbleClientError.unreachable("socket closed") }
                head.append(contentsOf: buffer[0..<count])
                if head.count >= 4 {
                    for index in 0...(head.count - 4) where headEnd == 0 {
                        if Array(head[index..<(index + 4)]) == marker { headEnd = index + 4 }
                    }
                }
            }
            let cut = headEnd
            let responseHead = String(decoding: head[0..<cut], as: UTF8.self)
            let statusLine = responseHead.split(separator: "\r\n").first.map(String.init) ?? ""
            statusCode = Int(statusLine.split(separator: " ").dropFirst().first ?? "") ?? 0
            rest = Array(head[cut...])
        }

        /// Read one chunk of body bytes; nil on timeout or close.
        mutating func read() -> [UInt8]? {
            var buffer = [UInt8](repeating: 0, count: 8192)
            let count = recv(fd, &buffer, buffer.count, 0)
            guard count > 0 else { return nil }
            return Array(buffer[0..<count])
        }

        func close() { _ = Foundation.close(fd) }
    }

    /// Opens /api/events, feeds raw bytes through the line splitter → frame
    /// parser → status reducer, and asserts Connected after the 200 and
    /// still Connected after a keepalive. Runs against any configured host.
    func testStreamStatusConnected() async throws {
        var socket = try EventSocket(host: host, token: token)
        defer { socket.close() }
        XCTAssertEqual(socket.statusCode, 200)

        var status = MarbleConnectionStatus()
        status.streamOpened()
        XCTAssertEqual(status.label, "Connected")

        var splitter = MarbleSSELineSplitter()
        var parser = MarbleSSEParser()
        var sawKeepalive = false
        var sawAnyLine = false
        var replayCursor: UInt64?
        var pending = socket.rest
        let deadline = Date().addingTimeInterval(25)
        while (!sawKeepalive || replayCursor == nil) && Date() < deadline {
            if pending.isEmpty {
                guard let chunk = socket.read() else { break }
                pending = chunk
            }
            let lines = splitter.feed(pending)
            pending = []
            for line in lines {
                sawAnyLine = true
                status.streamEvent()
                if line.hasPrefix(":") { sawKeepalive = true }
                if let cursor = parser.feed(line)?.replayCursor { replayCursor = cursor }
            }
        }
        XCTAssertTrue(sawAnyLine, "no bytes arrived on the event stream")
        XCTAssertTrue(sawKeepalive, "no keepalive arrived within 25 s")
        XCTAssertNotNil(replayCursor, "no replay boundary arrived on the stream")
        XCTAssertEqual(status.label, "Connected")
    }

    /// A state change while the stream is open must surface as a decoded
    /// frame — proof events are dispatched, not just that the socket is open.
    /// Isolated fixture only: it writes to host config.
    func testStreamDispatchesARealEvent() async throws {
        guard ProcessInfo.processInfo.environment["MARBLE_LIVE_ISOLATED"] == "1" else {
            throw XCTSkip("isolated fixture only (it writes host config)")
        }
        var socket = try EventSocket(host: host, token: token)
        defer { socket.close() }
        XCTAssertEqual(socket.statusCode, 200)

        var status = MarbleConnectionStatus()
        status.streamOpened()
        var splitter = MarbleSSELineSplitter()
        var parser = MarbleSSEParser()
        var kinds: [String] = []
        var pending = socket.rest

        // A model save emits a "control" event on the stream.
        try await post("/api/model", body: ["model": ""])

        let deadline = Date().addingTimeInterval(25)
        while kinds.isEmpty && Date() < deadline {
            if pending.isEmpty {
                guard let chunk = socket.read() else { break }
                pending = chunk
            }
            let lines = splitter.feed(pending)
            pending = []
            for line in lines {
                status.streamEvent()
                if let frame = parser.feed(line),
                   let data = frame.data.data(using: .utf8),
                   let event = try? JSONDecoder().decode(MarbleEvent.self, from: data) {
                    kinds.append(event.kind)
                }
            }
        }
        XCTAssertEqual(status.label, "Connected")
        XCTAssertFalse(kinds.isEmpty, "no event frame was dispatched")
    }

    private func post(_ path: String, body: [String: Any]) async throws {
        var request = try endpoint(token: token).request(path: path)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONSerialization.data(withJSONObject: body)
        let (data, response) = try await session.data(for: request)
        try marbleValidate(response, data: data)
    }
}

// MARK: - Everyone plane (isolated fixture host)

extension MarbleLiveTests {
    private struct MarbleCreated: Decodable {
        var marble: MarblePrismMarble?
    }
    private struct EveryoneReply: Decodable { var entries: [MarbleEntry] }

    private func post(_ path: String, _ body: [String: Any] = [:]) async throws -> Data {
        let request = try endpoint(token: token).request(
            path: path,
            method: "POST",
            body: JSONSerialization.data(withJSONObject: body)
        )
        let (data, response) = try await session.data(for: request)
        try marbleValidate(response, data: data)
        return data
    }

    private func makeMarble(name: String, persona: String = "") async throws -> MarblePrismMarble? {
        let data = try await post("/api/prism/marbles", [
            "name": name, "persona": persona, "glyph": "circle", "hue": 90,
        ])
        let marble = try JSONDecoder().decode(MarbleCreated.self, from: data).marble
        if let marble {
            _ = try await post("/api/prism/marbles/\(marble.id)/update", [
                "card": [
                    "inEveryone": true,
                    "tools": ["native": false, "allowed": ["glass.workspace.list", "glass.marble.ask"]],
                ],
            ])
        }
        return marble
    }

    private func timeline() async throws -> [MarbleEntry] {
        try await fetchJSON(EveryoneReply.self, .everyone(after: nil)).entries
    }

    /// Poll until `where` matches an Everyone entry or the deadline runs out.
    private func waitForEntry(
        timeout: TimeInterval = 60,
        where match: (MarbleEntry) -> Bool
    ) async throws -> MarbleEntry? {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if let entry = try await timeline().first(where: match) {
                return entry
            }
            try await Task.sleep(nanoseconds: 700_000_000)
        }
        return nil
    }

    /// A named mention is answered by that Marble, mirrored onto the shared
    /// plane with its id and display name.
    func testEveryoneMentionMirrorsTheMarble() async throws {
        // The fixture engine reaches for a tool every turn; unattended lets
        // it run without an approval card stalling the mirror.
        _ = try await post("/api/control/profile", ["profile": "unattended"])
        let marble = try await makeMarble(name: "Livy", persona: "brief")
        let marbleId = try XCTUnwrap(marble?.id)
        addTeardownBlock { [self] in
            _ = try? await post("/api/prism/marbles/\(marbleId)/remove")
            _ = try? await post("/api/everyone/clear")
            _ = try? await post("/api/control/profile", ["profile": "workspace"])
        }

        _ = try await post("/api/everyone", ["text": "@Livy say hi"])
        let reply = try await waitForEntry {
            $0.kind == "assistant" && $0.speaker == marbleId
        }
        let entry = try XCTUnwrap(reply, "no mirrored reply from Livy")
        XCTAssertEqual(entry.speakerName, "Livy")
        XCTAssertFalse(entry.text.isEmpty)
        // The person's line sits in the same timeline, stamped with the task
        // that answered it.
        let userLine = try await timeline().first { $0.kind == "user" }
        XCTAssertEqual(userLine?.speaker, "user")
        XCTAssertEqual(userLine?.taskId.isEmpty, false)

    }

    /// A Marble asking another for help lands on Everyone as plain handoff
    /// lines that decode and render through Core.
    func testMarbleAskRendersHandoffLinesOnEveryone() async throws {
        _ = try await post("/api/control/profile", ["profile": "unattended"])
        let asker = try await makeMarble(name: "Askbot", persona: "")
        let helper = try await makeMarble(name: "Helper", persona: "")
        let askerId = try XCTUnwrap(asker?.id)
        let helperId = try XCTUnwrap(helper?.id)
        addTeardownBlock { [self] in
            _ = try? await post("/api/prism/marbles/\(askerId)/remove")
            _ = try? await post("/api/prism/marbles/\(helperId)/remove")
            _ = try? await post("/api/everyone/clear")
            _ = try? await post("/api/control/profile", ["profile": "workspace"])
        }

        _ = try await post("/api/everyone", ["text": "@Askbot ASK Helper: check the lights"])

        // "Askbot asked Helper to check the lights."
        let asked = try await waitForEntry {
            $0.kind == "handoff" && $0.handoff?.status == "asked"
        }
        let askedEntry = try XCTUnwrap(asked, "no asked handoff on Everyone")
        let askedHandoff = try XCTUnwrap(askedEntry.handoff)
        XCTAssertEqual(askedHandoff.from, askerId)
        XCTAssertEqual(askedHandoff.to, helperId)
        XCTAssertEqual(
            MarbleHandoffLine.render(askedHandoff),
            "Askbot asked Helper to check the lights."
        )

        // "Helper finished: …" once the ask settles.
        let finished = try await waitForEntry(timeout: 90) {
            $0.kind == "handoff" && $0.handoff?.status == "finished"
        }
        let finishedHandoff = try XCTUnwrap(
            finished?.handoff, "no finished handoff on Everyone")
        XCTAssertTrue(MarbleHandoffLine.isExpandable(finishedHandoff))
        XCTAssertTrue(
            MarbleHandoffLine.render(finishedHandoff).hasPrefix("Helper finished:"),
            MarbleHandoffLine.render(finishedHandoff)
        )

    }
}
