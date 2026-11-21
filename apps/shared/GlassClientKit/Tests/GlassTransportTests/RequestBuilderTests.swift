import Foundation
import GlassTransport
import Testing

@Suite struct EndpointTests {
    @Test func originMatchesBrowserSerialization() throws {
        #expect(try GlassEndpoint(origin: "HTTPS://Host.Tail1.ts.net:443/path?x#owner=a", mode: .remote).origin == "https://host.tail1.ts.net")
        #expect(try GlassEndpoint(origin: "https://host.tail1.ts.net:8443", mode: .remote).origin == "https://host.tail1.ts.net:8443")
        #expect(try GlassEndpoint(origin: "http://100.101.102.103:80", mode: .remote).origin == "http://100.101.102.103")
        #expect(try GlassEndpoint(origin: "http://[fd7a:115c:a1e0::1]:8080", mode: .remote).origin == "http://[fd7a:115c:a1e0::1]:8080")
    }

    @Test func rejectsNonWebSchemesAndUserInfo() {
        #expect(throws: GlassError.self) { try GlassEndpoint(origin: "ftp://host", mode: .remote) }
        #expect(throws: GlassError.self) { try GlassEndpoint(origin: "https://user:pw@host", mode: .remote) }
    }

    @Test func detectsTailnetHosts() throws {
        #expect(try GlassEndpoint(origin: "https://mac.tail1.ts.net", mode: .remote).looksLikeTailnet)
        #expect(try GlassEndpoint(origin: "http://100.64.0.1", mode: .remote).looksLikeTailnet)
        #expect(try !GlassEndpoint(origin: "http://192.168.1.10:8080", mode: .remote).looksLikeTailnet)
        #expect(try !GlassEndpoint(origin: "http://100.128.0.1", mode: .remote).looksLikeTailnet)
    }
}

@Suite struct RequestBuilderTests {
    let remote = try! GlassEndpoint(origin: "https://mac.tail1.ts.net", mode: .remote)
    let lan = try! GlassEndpoint(origin: "http://192.168.1.10:8080", mode: .remote)
    let cap = String(repeating: "ab", count: 32)

    @Test func getCarriesOwnerCapabilityWithoutOriginOrCookies() throws {
        let r = try GlassRequestBuilder.urlRequest(for: .get("/api/status"), endpoint: remote, credential: .ownerCapability(cap))
        #expect(r.url?.absoluteString == "https://mac.tail1.ts.net/api/status")
        #expect(r.httpMethod == "GET")
        #expect(r.value(forHTTPHeaderField: "X-Glass-Owner-Capability") == cap)
        #expect(r.value(forHTTPHeaderField: "Origin") == nil)
        #expect(r.value(forHTTPHeaderField: "Authorization") == nil)
        #expect(r.httpShouldHandleCookies == false)
    }

    @Test func postSetsOriginAndJSONContentType() throws {
        let r = try GlassRequestBuilder.urlRequest(
            for: .post("/api/local", json: ["token": "x"]), endpoint: remote, credential: .none)
        #expect(r.value(forHTTPHeaderField: "Origin") == "https://mac.tail1.ts.net")
        #expect(r.value(forHTTPHeaderField: "Content-Type") == "application/json")
        #expect(r.value(forHTTPHeaderField: "X-Glass-Owner-Capability") == nil)
        #expect(String(data: r.httpBody ?? Data(), encoding: .utf8) == #"{"token":"x"}"#)
    }

    @Test func localModeUsesBearer() throws {
        let local = try GlassEndpoint(origin: "http://127.0.0.1:51718", mode: .local)
        let r = try GlassRequestBuilder.urlRequest(for: .get("/api/status"), endpoint: local, credential: .bearer(cap))
        #expect(r.value(forHTTPHeaderField: "Authorization") == "Bearer \(cap)")
    }

    @Test func queryItemsAreEncoded() throws {
        let r = try GlassRequestBuilder.urlRequest(
            for: .get("/api/history", query: [URLQueryItem(name: "thread", value: "a b&c")]), endpoint: remote, credential: .none)
        #expect(r.url?.absoluteString == "https://mac.tail1.ts.net/api/history?thread=a%20b%26c")
    }

    @Test func enforcesBodyLimitsBeforeSending() throws {
        let big = GlassRequest(method: .post, path: "/api/send", body: Data(count: GlassBodyLimit.json + 1))
        #expect(throws: GlassError.bodyTooLarge(limit: GlassBodyLimit.json, actual: GlassBodyLimit.json + 1)) {
            try GlassRequestBuilder.urlRequest(for: big, endpoint: remote, credential: .ownerCapability(cap))
        }
        let ok = GlassRequest(method: .post, path: "/api/upload", body: Data(count: GlassBodyLimit.json + 1), bodyLimit: GlassBodyLimit.upload)
        _ = try GlassRequestBuilder.urlRequest(for: ok, endpoint: remote, credential: .ownerCapability(cap))
    }

    @Test func secretsAreRefusedOverHTTP() throws {
        let secret = GlassRequest(method: .post, path: "/api/secret", body: Data("{}".utf8), carriesSecret: true)
        #expect(throws: GlassError.insecureTransport) {
            try GlassRequestBuilder.urlRequest(for: secret, endpoint: lan, credential: .ownerCapability(cap), secretCapability: cap)
        }
        let r = try GlassRequestBuilder.urlRequest(for: secret, endpoint: remote, credential: .ownerCapability(cap), secretCapability: cap)
        #expect(r.value(forHTTPHeaderField: "X-Glass-Secret-Capability") == cap)
        let plain = try GlassRequestBuilder.urlRequest(for: .get("/api/status"), endpoint: remote, credential: .ownerCapability(cap), secretCapability: cap)
        #expect(plain.value(forHTTPHeaderField: "X-Glass-Secret-Capability") == nil)
    }

    @Test func rejectsProtocolRelativePaths() {
        #expect(throws: GlassError.self) {
            try GlassRequestBuilder.urlRequest(for: .get("//evil.example/api"), endpoint: remote, credential: .none)
        }
    }
}

@Suite struct ResponseTests {
    func response(_ code: Int) -> HTTPURLResponse {
        HTTPURLResponse(url: URL(string: "https://h")!, statusCode: code, httpVersion: nil, headerFields: nil)!
    }

    @Test func maps403ToUnauthorizedWithGatewayMessage() {
        let body = Data(#"{"error":"Open the private Connect owner link","ownerAuthRequired":true}"#.utf8)
        #expect(throws: GlassError.unauthorized("Open the private Connect owner link")) {
            try GlassResponse.validate(body, response(403))
        }
    }

    @Test func mapsOtherStatuses() throws {
        #expect(throws: GlassError.gateway(status: 413, message: "too big")) {
            try GlassResponse.validate(Data(#"{"error":"too big"}"#.utf8), response(413))
        }
        #expect(throws: GlassError.self) { try GlassResponse.validate(Data(), response(302)) }
        #expect(try GlassResponse.validate(Data("{}".utf8), response(200)) == Data("{}".utf8))
    }

    @Test func mapsUnreachableTailnetHosts() throws {
        let tailnet = try GlassEndpoint(origin: "https://mac.tail1.ts.net", mode: .remote)
        guard case .hostUnreachable(true, _) = GlassError.from(URLError(.cannotFindHost), endpoint: tailnet) else {
            Issue.record("expected tailnet unreachable"); return
        }
        #expect(GlassError.hostUnreachable(tailnet: true, detail: "").errorDescription?.contains("Tailscale") == true)
    }
}
