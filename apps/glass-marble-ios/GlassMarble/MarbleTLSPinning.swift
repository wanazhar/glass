import Foundation
import Security

/// The leaf-certificate SHA-256 the app trusts for one host:port, kept in the
/// keychain next to the token. Trusting the token instead of a CA is what lets
/// a self-hosted host serve a self-signed cert safely.
enum MarblePinStore {
    /// The authority a TLS protection space reports: host:port.
    static func authority(host: String, port: Int) -> String {
        "\(host):\(port)"
    }

    static func authority(for endpoint: MarbleEndpoint) -> String {
        let host = endpoint.baseURL.host ?? ""
        let port = endpoint.baseURL.port
            ?? (endpoint.baseURL.scheme == "https" ? 443 : 80)
        return authority(host: host, port: port)
    }

    private static func account(_ authority: String) -> String {
        "marble.pin.\(authority)"
    }

    static func pin(for authority: String) -> String? {
        MarblePreferences.readKeychain(account: account(authority))
    }

    static func store(_ sha256Hex: String, for authority: String) {
        MarblePreferences.writeKeychain(sha256Hex, account: account(authority))
    }

    static func clear(for authority: String) {
        MarblePreferences.deleteKeychain(account: account(authority))
    }
}

/// Server-trust handling for https endpoints: a certificate the system already
/// trusts wins; otherwise the leaf hash must equal the pin the pairing flow
/// stored for this host:port.
final class MarblePinnedSessionDelegate: NSObject, URLSessionDelegate {
    func urlSession(
        _ session: URLSession,
        didReceive challenge: URLAuthenticationChallenge,
        completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void
    ) {
        guard
            challenge.protectionSpace.authenticationMethod
                == NSURLAuthenticationMethodServerTrust,
            let trust = challenge.protectionSpace.serverTrust
        else {
            completionHandler(.performDefaultHandling, nil)
            return
        }
        var error: CFError?
        if SecTrustEvaluateWithError(trust, &error) {
            completionHandler(.useCredential, URLCredential(trust: trust))
            return
        }
        let authority = MarblePinStore.authority(
            host: challenge.protectionSpace.host,
            port: challenge.protectionSpace.port
        )
        if let leaf = leafDER(of: trust),
           let pin = MarblePinStore.pin(for: authority),
           MarbleSHA256.hex(leaf) == pin {
            completionHandler(.useCredential, URLCredential(trust: trust))
            return
        }
        completionHandler(.cancelAuthenticationChallenge, nil)
    }

    /// The leaf certificate's DER bytes — what the pin hashes.
    private func leafDER(of trust: SecTrust) -> Data? {
        guard
            let chain = SecTrustCopyCertificateChain(trust) as? [SecCertificate],
            let leaf = chain.first
        else { return nil }
        return SecCertificateCopyData(leaf) as Data
    }
}

/// The delegate for the pairing probe: records the leaf it actually sees and
/// lets the connection through. It never carries Authorization or cookies, so
/// an attacker who MITMs this leg learns nothing but a hash.
private final class MarbleRecordingTrustDelegate: NSObject, URLSessionDelegate {
    final class Box: @unchecked Sendable {
        var leafSHA256: String?
    }

    let box = Box()

    func urlSession(
        _ session: URLSession,
        didReceive challenge: URLAuthenticationChallenge,
        completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void
    ) {
        guard
            challenge.protectionSpace.authenticationMethod
                == NSURLAuthenticationMethodServerTrust,
            let trust = challenge.protectionSpace.serverTrust
        else {
            completionHandler(.performDefaultHandling, nil)
            return
        }
        if let chain = SecTrustCopyCertificateChain(trust) as? [SecCertificate],
           let leaf = chain.first {
            box.leafSHA256 = MarbleSHA256.hex(SecCertificateCopyData(leaf) as Data)
        }
        completionHandler(.useCredential, URLCredential(trust: trust))
    }
}

/// Errors the pinning flow reports so onboarding can phrase them.
enum MarblePinningError: Error, Equatable {
    /// tls-proof did not return a proof (plain HTTP host, or no token on it).
    case proofUnavailable
    /// The host's proof did not match the observed certificate.
    case verificationFailed
}

enum MarblePinning {
    /// Pair an https endpoint: fetch the certificate proof on a tokenless,
    /// accepting session, verify it against the token, and store the leaf hash
    /// as the pin for this host:port.
    static func pin(_ endpoint: MarbleEndpoint) async throws -> String {
        guard let token = endpoint.token, !token.isEmpty else {
            throw MarblePinningError.proofUnavailable
        }
        let delegate = MarbleRecordingTrustDelegate()
        let configuration = URLSessionConfiguration.ephemeral
        configuration.httpShouldSetCookies = false
        configuration.httpCookieStorage = nil
        configuration.timeoutIntervalForRequest = 30
        let session = URLSession(
            configuration: configuration,
            delegate: delegate,
            delegateQueue: nil
        )
        defer { session.invalidateAndCancel() }

        let nonce = MarbleTLSPin.nonce()
        let route = MarbleRoute.tlsProof(nonce: nonce)
        guard let url = endpoint.url(path: route.path, query: route.query) else {
            throw MarbleClientError.invalidURL
        }
        var request = URLRequest(url: url)
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        let (data, response): (Data, URLResponse)
        do {
            (data, response) = try await session.data(for: request)
        } catch {
            throw MarbleClientError.requestError(error)
        }
        try marbleValidate(response, data: data)
        struct Proof: Decodable {
            var certSha256: String
            var proof: String
        }
        guard let reply = try? JSONDecoder().decode(Proof.self, from: data) else {
            throw MarbleClientError.decoding("tls-proof reply")
        }
        guard let observed = delegate.box.leafSHA256 else {
            throw MarblePinningError.proofUnavailable
        }
        guard MarbleTLSPin.verify(
            token: token,
            nonce: nonce,
            observedCertSHA256: observed,
            proofHex: reply.proof
        ) else {
            throw MarblePinningError.verificationFailed
        }
        let authority = MarblePinStore.authority(for: endpoint)
        MarblePinStore.store(observed, for: authority)
        return observed
    }
}
