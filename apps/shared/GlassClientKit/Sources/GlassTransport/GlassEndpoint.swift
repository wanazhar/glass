import Foundation

/// One Glass gateway origin, serialized exactly as a browser would send it in `Origin`.
public struct GlassEndpoint: Sendable, Hashable, Codable, CustomStringConvertible {
    public enum Mode: String, Sendable, Hashable, Codable {
        /// Tailnet or LAN host; requests carry the owner capability.
        case remote
        /// `glass-core connect --local` on loopback; requests carry the bearer token.
        case local
    }

    /// `scheme://host[:port]`, lowercased, default ports omitted.
    public let origin: String
    public let mode: Mode

    public init(url: URL, mode: Mode) throws(GlassError) {
        guard let scheme = url.scheme?.lowercased(), scheme == "https" || scheme == "http" else {
            throw .invalidEndpoint("Glass links start with https:// or http://")
        }
        guard url.user == nil, url.password == nil else {
            throw .invalidEndpoint("Links with a user name or password are not Glass links")
        }
        guard let rawHost = url.host(percentEncoded: false)?.lowercased(), !rawHost.isEmpty else {
            throw .invalidEndpoint("The link has no host")
        }
        let host = rawHost.contains(":") ? "[\(rawHost)]" : rawHost
        let defaultPort = scheme == "https" ? 443 : 80
        let port = url.port.flatMap { $0 == defaultPort ? nil : $0 }
        self.origin = port.map { "\(scheme)://\(host):\($0)" } ?? "\(scheme)://\(host)"
        self.mode = mode
    }

    public init(origin: String, mode: Mode) throws(GlassError) {
        guard let url = URL(string: origin) else { throw .invalidEndpoint("Not a valid address") }
        try self.init(url: url, mode: mode)
    }

    public var url: URL { URL(string: origin)! }
    public var host: String { url.host(percentEncoded: false) ?? "" }
    public var isSecure: Bool { origin.hasPrefix("https://") }
    public var isLoopback: Bool { ["127.0.0.1", "localhost", "::1"].contains(host) }

    /// Tailscale MagicDNS names and CGNAT / ULA tailnet addresses.
    public var looksLikeTailnet: Bool {
        let host = host
        if host.hasSuffix(".ts.net") { return true }
        if host.hasPrefix("fd7a:115c:a1e0:") { return true }
        let octets = host.split(separator: ".").compactMap { UInt8($0) }
        return octets.count == 4 && octets[0] == 100 && (64...127).contains(octets[1])
    }

    public var description: String { origin }
}
