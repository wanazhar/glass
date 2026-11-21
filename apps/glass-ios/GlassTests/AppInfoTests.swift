import Foundation
import GlassModels
import Testing
@testable import Glass

@Suite struct AppInfoTests {
    @Test func bundleCarriesTheShippingIdentity() {
        let info = AppInfo(bundle: .main)
        #expect(info.bundleIdentifier == "com.wanazhar.glass")
        #expect(info.displayName == "Glass")
        #expect(info.version == "0.1.0")
        #expect(Int(info.build) != nil)
    }

    @Test func versionLabelMatchesAboutScreenFormat() {
        let info = AppInfo(displayName: "Glass", bundleIdentifier: "x", version: "0.1.0", build: "7")
        #expect(info.versionLabel == "0.1.0 (7)")
    }

    @Test func linksGlassClientKit() throws {
        let status = try JSONDecoder().decode(GatewayStatus.self, from: Data(#"{"workspace":"/w"}"#.utf8))
        #expect(status.workspaceName == "w")
    }
}
