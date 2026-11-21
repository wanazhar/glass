import XCTest

final class LaunchTests: XCTestCase {
    override func setUp() {
        continueAfterFailure = false
    }

    @MainActor
    func testLaunchShowsIdentityAndVersion() throws {
        let app = XCUIApplication()
        app.launch()
        XCTAssertTrue(app.staticTexts["home.title"].waitForExistence(timeout: 10))
        XCTAssertEqual(app.staticTexts["home.title"].label, "Glass")
        let version = app.descendants(matching: .any)["home.version"]
        XCTAssertTrue(version.waitForExistence(timeout: 5))
        XCTAssertTrue(version.label.contains("0.1.0"), version.label)
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = "home-\(UIDevice.current.userInterfaceIdiom == .pad ? "ipad" : "iphone")"
        shot.lifetime = .keepAlways
        add(shot)
    }

    @MainActor
    func testLaunchesInLandscape() throws {
        XCUIDevice.shared.orientation = .landscapeLeft
        defer { XCUIDevice.shared.orientation = .portrait }
        let app = XCUIApplication()
        app.launch()
        XCTAssertTrue(app.staticTexts["home.title"].waitForExistence(timeout: 10))
    }
}
