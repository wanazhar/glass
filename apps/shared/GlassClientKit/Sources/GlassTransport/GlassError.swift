import Foundation

public enum GlassError: Error, Sendable, Hashable, LocalizedError {
    case invalidEndpoint(String)
    /// The request needs HTTPS (secret entry) but the endpoint is plain HTTP.
    case insecureTransport
    case bodyTooLarge(limit: Int, actual: Int)
    /// 403: the owner capability is missing, stale, or was revoked by restarting Connect.
    case unauthorized(String)
    case hostUnreachable(tailnet: Bool, detail: String)
    case tls(String)
    case gateway(status: Int, message: String)
    case invalidResponse(String)
    case decoding(String)
    case cancelled

    public var errorDescription: String? {
        switch self {
        case .invalidEndpoint(let message): message
        case .insecureTransport: "Secrets are only sent over HTTPS. Use the Tailscale HTTPS link for this host."
        case .bodyTooLarge(let limit, _): "That is larger than the host accepts (\(ByteCountFormatter.string(fromByteCount: Int64(limit), countStyle: .binary)))."
        case .unauthorized(let message): message
        case .hostUnreachable(true, _): "Can’t reach your host. Make sure Tailscale is connected on this iPhone and the host is online."
        case .hostUnreachable(false, _): "Can’t reach your host. Check that this iPhone is on the same network and Connect is running."
        case .tls(let detail): "The secure connection failed: \(detail)"
        case .gateway(let status, let message): message.isEmpty ? "The host returned HTTP \(status)." : message
        case .invalidResponse(let detail): "Unexpected response from the host: \(detail)"
        case .decoding(let detail): "The host sent data this version of Glass can’t read: \(detail)"
        case .cancelled: "Cancelled."
        }
    }

    public var isUnauthorized: Bool { if case .unauthorized = self { true } else { false } }

    public static func from(_ error: any Error, endpoint: GlassEndpoint) -> GlassError {
        if let error = error as? GlassError { return error }
        if error is CancellationError { return .cancelled }
        guard let error = error as? URLError else { return .invalidResponse(error.localizedDescription) }
        switch error.code {
        case .cancelled: return .cancelled
        case .cannotFindHost, .cannotConnectToHost, .dnsLookupFailed, .timedOut, .networkConnectionLost,
             .notConnectedToInternet, .internationalRoamingOff, .dataNotAllowed, .resourceUnavailable:
            return .hostUnreachable(tailnet: endpoint.looksLikeTailnet, detail: error.localizedDescription)
        case .secureConnectionFailed, .serverCertificateHasBadDate, .serverCertificateUntrusted,
             .serverCertificateHasUnknownRoot, .serverCertificateNotYetValid, .clientCertificateRejected,
             .clientCertificateRequired, .appTransportSecurityRequiresSecureConnection:
            return .tls(error.localizedDescription)
        default:
            return .invalidResponse(error.localizedDescription)
        }
    }
}
