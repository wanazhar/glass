import Foundation
import GlassModels
import GlassTestSupport
import Testing

@Suite struct GatewayStatusTests {
    @Test func decodesRecordedRemoteStatus() throws {
        let status = try JSONDecoder().decode(GatewayStatus.self, from: Fixtures.data("status-remote"))
        #expect(status.name == "Glass")
        #expect(status.remote)
        #expect(status.workspaceName == "glass")
        #expect(status.connectCommand == "glass connect")
        #expect(status.secretSubmissionUnlocked == false)
        #expect(status.profile == "unrestricted")
        #expect(status.security?["mode"]?.stringValue == "standard")
        #expect(status.protocolVersion == nil)
    }

    @Test func toleratesMissingAndUnknownFields() throws {
        let json = Data(#"{"workspace":"/srv/w/","future":{"x":[1,true,null]},"protocolVersion":1}"#.utf8)
        let status = try JSONDecoder().decode(GatewayStatus.self, from: json)
        #expect(status.name == "Glass")
        #expect(status.remote == false)
        #expect(status.workspaceName == "w")
        #expect(status.protocolVersion == GlassClientKitVersion.supportedProtocolVersion)
    }

    @Test func jsonValueRoundTrips() throws {
        let value: JSONValue = .object(["a": .array([.number(1), .bool(true), .null, .string("s")])])
        let data = try JSONEncoder().encode(value)
        #expect(try JSONDecoder().decode(JSONValue.self, from: data) == value)
    }
}
