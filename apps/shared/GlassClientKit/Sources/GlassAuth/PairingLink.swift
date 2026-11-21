import Foundation
import GlassTransport

/// A private Connect link (or bare code) printed by `glass-core connect`, parsed with the same
/// rules as the web UI's boot screen.
public struct PairingLink: Sendable, Hashable, CustomStringConvertible {
    public enum Kind: Sendable, Hashable {
        /// `#owner=<64 hex | 22 base64url>`: unlocks the owner capability.
        case owner
        /// `#secret=<64 hex>` (HTTPS only): owner capability plus in-memory secret entry.
        case secret
        /// `http://127.0.0.1:<port>/#<64 hex>`: loopback local mode, bearer auth.
        case local
    }

    public let endpoint: GlassEndpoint
    public let kind: Kind
    public let token: String

    public var description: String { "PairingLink(\(kind), \(endpoint.origin), token: <redacted>)" }

    public struct Policy: Sendable, Hashable {
        /// Plain-HTTP LAN/tailnet hosts. Off by default; loopback local mode is always allowed.
        public var allowInsecureHTTP: Bool
        public init(allowInsecureHTTP: Bool = false) { self.allowInsecureHTTP = allowInsecureHTTP }
    }

    public enum ParseError: Error, Sendable, Hashable, LocalizedError {
        case empty
        case notALink
        case missingToken
        case secretRequiresHTTPS
        case insecureHTTPDisabled
        case needsHost

        public var errorDescription: String? {
            switch self {
            case .empty: "Paste the private link from your host’s terminal."
            case .notALink: "That isn’t a Glass Connect link."
            case .missingToken: "The link is missing its private code. Copy the whole line from the terminal."
            case .secretRequiresHTTPS: "Secret-entry links only work over HTTPS."
            case .insecureHTTPDisabled: "This is a plain-HTTP link. Turn on “Allow HTTP on local network” to use it."
            case .needsHost: "Paste the whole link, not just the code, the first time you pair."
            }
        }
    }

    public static func parse(
        _ input: String, policy: Policy = Policy(), knownEndpoint: GlassEndpoint? = nil
    ) throws(ParseError) -> PairingLink {
        let text = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { throw .empty }

        if isCompactOwnerToken(text) || isHexToken(text) {
            guard let knownEndpoint, knownEndpoint.mode == .remote else { throw .needsHost }
            return try validated(PairingLink(endpoint: knownEndpoint, kind: .owner, token: text), policy: policy)
        }

        guard let url = URL(string: text), url.scheme != nil, let fragment = url.fragment(percentEncoded: false) else {
            throw text.contains("://") ? .missingToken : .notALink
        }
        let isLoopbackHTTP = url.scheme?.lowercased() == "http" && url.host(percentEncoded: false) == "127.0.0.1"
        let link: PairingLink
        do {
            if isLoopbackHTTP, isHexToken(fragment) {
                link = PairingLink(endpoint: try GlassEndpoint(url: url, mode: .local), kind: .local, token: fragment)
            } else if let token = value(of: "owner", in: fragment), isHexToken(token) || isCompactOwnerToken(token) {
                link = PairingLink(endpoint: try GlassEndpoint(url: url, mode: .remote), kind: .owner, token: token)
            } else if let token = value(of: "secret", in: fragment), isHexToken(token) {
                link = PairingLink(endpoint: try GlassEndpoint(url: url, mode: .remote), kind: .secret, token: token)
            } else {
                throw ParseError.missingToken
            }
        } catch let error as ParseError {
            throw error
        } catch {
            throw .notALink
        }
        return try validated(link, policy: policy)
    }

    private static func validated(_ link: PairingLink, policy: Policy) throws(ParseError) -> PairingLink {
        if link.kind == .secret, !link.endpoint.isSecure { throw .secretRequiresHTTPS }
        if link.kind != .local, !link.endpoint.isSecure, !policy.allowInsecureHTTP { throw .insecureHTTPDisabled }
        return link
    }

    private static func value(of key: String, in fragment: String) -> String? {
        let prefix = key + "="
        return fragment.hasPrefix(prefix) ? String(fragment.dropFirst(prefix.count)) : nil
    }

    static func isHexToken(_ s: String) -> Bool {
        s.utf8.count == 64 && s.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) }
    }

    static func isCompactOwnerToken(_ s: String) -> Bool {
        s.utf8.count == 22 && s.utf8.allSatisfy {
            (48...57).contains($0) || (65...90).contains($0) || (97...122).contains($0) || $0 == 45 || $0 == 95
        }
    }
}
