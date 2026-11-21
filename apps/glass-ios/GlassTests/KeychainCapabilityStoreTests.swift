import Foundation
import GlassAuth
import GlassTransport
import Testing

/// Runs inside the host app so it exercises the real iOS Keychain.
@Suite(.serialized) struct KeychainCapabilityStoreTests {
    let service = "com.wanazhar.glass.tests.\(UUID().uuidString)"
    let remote = try! GlassEndpoint(origin: "https://mac.tail1.ts.net", mode: .remote)
    let other = try! GlassEndpoint(origin: "https://mac.tail1.ts.net:8443", mode: .remote)
    let owner = String(repeating: "a", count: 64)

    @Test func roundTripsUpdatesAndRemoves() throws {
        let store = KeychainCapabilityStore(service: service)
        defer { try? store.remove(for: remote) }
        #expect(try store.credential(for: remote) == .none)
        try store.save(.ownerCapability(owner), for: remote)
        #expect(try store.credential(for: remote) == .ownerCapability(owner))
        let rotated = String(repeating: "b", count: 64)
        try store.save(.ownerCapability(rotated), for: remote)
        #expect(try store.credential(for: remote) == .ownerCapability(rotated))
        try store.remove(for: remote)
        #expect(try store.credential(for: remote) == .none)
        try store.remove(for: remote)
    }

    @Test func isolatesOriginsAndProducts() throws {
        let glass = KeychainCapabilityStore(service: service)
        let marble = KeychainCapabilityStore(service: service + ".marble")
        defer { try? glass.remove(for: remote); try? glass.remove(for: other) }
        try glass.save(.ownerCapability(owner), for: remote)
        #expect(try glass.credential(for: other) == .none)
        #expect(try marble.credential(for: remote) == .none)
        let local = try GlassEndpoint(origin: "http://127.0.0.1:5000", mode: .local)
        try glass.save(.bearer(owner), for: local)
        defer { try? glass.remove(for: local) }
        #expect(try glass.credential(for: local) == .bearer(owner))
    }
}
