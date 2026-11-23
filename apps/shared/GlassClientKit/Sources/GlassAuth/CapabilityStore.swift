import Foundation
import GlassTransport

/// Persists one credential per gateway origin. Secret capabilities are never persisted.
public protocol CapabilityStore: Sendable {
    func credential(for endpoint: GlassEndpoint) throws -> GlassCredential
    func save(_ credential: GlassCredential, for endpoint: GlassEndpoint) throws
    func remove(for endpoint: GlassEndpoint) throws
}

public final class InMemoryCapabilityStore: CapabilityStore, @unchecked Sendable {
    private let lock = NSLock()
    private var values: [GlassEndpoint: GlassCredential] = [:]

    public init() {}

    public func credential(for endpoint: GlassEndpoint) throws -> GlassCredential {
        lock.withLock { values[endpoint] ?? .none }
    }

    public func save(_ credential: GlassCredential, for endpoint: GlassEndpoint) throws {
        lock.withLock { values[endpoint] = credential }
    }

    public func remove(for endpoint: GlassEndpoint) throws {
        lock.withLock { _ = values.removeValue(forKey: endpoint) }
    }
}

public struct KeychainError: Error, Sendable, Hashable, LocalizedError {
    public let status: OSStatus
    public var errorDescription: String? {
        "Keychain error \(status): \((SecCopyErrorMessageString(status, nil) as String?) ?? "unknown")"
    }
}

/// Generic-password items keyed by `service` + `mode|origin`, device-only, never synced.
/// Glass and Marble use different `service` values and no access group, so they never share items.
public struct KeychainCapabilityStore: CapabilityStore {
    public let service: String

    public init(service: String) { self.service = service }

    private func account(_ endpoint: GlassEndpoint) -> String { "\(endpoint.mode.rawValue)|\(endpoint.origin)" }

    private func query(_ endpoint: GlassEndpoint) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account(endpoint),
            kSecAttrSynchronizable as String: false,
            kSecUseDataProtectionKeychain as String: true,
        ]
    }

    public func credential(for endpoint: GlassEndpoint) throws -> GlassCredential {
        var q = query(endpoint)
        q[kSecReturnData as String] = true
        q[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        let status = SecItemCopyMatching(q as CFDictionary, &item)
        if status == errSecItemNotFound { return .none }
        guard status == errSecSuccess, let data = item as? Data else { throw KeychainError(status: status) }
        return (try? JSONDecoder().decode(GlassCredential.self, from: data)) ?? .none
    }

    public func save(_ credential: GlassCredential, for endpoint: GlassEndpoint) throws {
        guard credential != .none else { return try remove(for: endpoint) }
        let data = try JSONEncoder().encode(credential)
        let update = SecItemUpdate(query(endpoint) as CFDictionary, [kSecValueData as String: data] as CFDictionary)
        if update == errSecSuccess { return }
        guard update == errSecItemNotFound else { throw KeychainError(status: update) }
        var add = query(endpoint)
        add[kSecValueData as String] = data
        add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        let status = SecItemAdd(add as CFDictionary, nil)
        guard status == errSecSuccess else { throw KeychainError(status: status) }
    }

    public func remove(for endpoint: GlassEndpoint) throws {
        let status = SecItemDelete(query(endpoint) as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else { throw KeychainError(status: status) }
    }
}
