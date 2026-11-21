/// How a gateway's advertised protocol relates to what this client speaks.
public enum ProtocolCompatibility: Sendable, Hashable {
    case compatible
    /// The gateway predates `protocolVersion` on `/api/status`; it works, but cannot advertise features.
    case legacyGateway
    /// The gateway speaks a newer, breaking protocol. The app must be updated.
    case appUpdateRequired(gatewayVersion: Int)

    public init(gatewayVersion: Int?, supported: Int = GlassClientKitVersion.supportedProtocolVersion) {
        switch gatewayVersion {
        case nil: self = .legacyGateway
        case let version? where version > supported: self = .appUpdateRequired(gatewayVersion: version)
        default: self = .compatible
        }
    }
}

extension GatewayStatus {
    public var compatibility: ProtocolCompatibility { ProtocolCompatibility(gatewayVersion: protocolVersion) }

    public func supports(_ capability: String) -> Bool { capabilities?.contains(capability) ?? false }
}
