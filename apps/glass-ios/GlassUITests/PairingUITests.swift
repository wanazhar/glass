import XCTest

final class PairingUITests: XCTestCase {
    override func setUp() { continueAfterFailure = false }

    @MainActor
    private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-GlassResetState"]
        app.launch()
        XCTAssertTrue(app.staticTexts["pair.title"].waitForExistence(timeout: 10))
        return app
    }

    @MainActor
    func testFirstLaunchShowsPairing() throws {
        let app = launch()
        XCTAssertEqual(app.staticTexts["pair.title"].label, "Connect to your host")
        XCTAssertTrue(app.textFields["pair.link"].exists || app.textViews["pair.link"].exists)
        XCTAssertTrue(app.buttons["pair.scan"].exists)
        XCTAssertFalse(app.buttons["pair.connect"].isEnabled)
        XCTAssertTrue(app.switches["pair.allowHTTP"].exists)
        attach(app, "pairing")
    }

    @MainActor
    func testRejectsTextThatIsNotALink() throws {
        let app = launch()
        enter("hello there", in: app)
        app.buttons["pair.connect"].tap()
        let error = app.descendants(matching: .any)["pair.error"]
        XCTAssertTrue(error.waitForExistence(timeout: 5))
        XCTAssertTrue(error.label.contains("isn’t a Glass Connect link"), error.label)
    }

    @MainActor
    func testPlainHTTPLinkNeedsTheLANToggle() throws {
        let app = launch()
        enter("http://192.168.1.10:8080#owner=AbCdEfGhIjKlMnOpQr_-12", in: app)
        app.buttons["pair.connect"].tap()
        let error = app.descendants(matching: .any)["pair.error"]
        XCTAssertTrue(error.waitForExistence(timeout: 5))
        XCTAssertTrue(error.label.contains("Allow HTTP on local network"), error.label)
        attach(app, "pairing-http-refused")
    }

    @MainActor
    func testScannerExplainsWhenTheCameraIsUnavailable() throws {
        let app = launch()
        app.buttons["pair.scan"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["scan.unavailable"].waitForExistence(timeout: 5))
        app.buttons["Cancel"].tap()
        XCTAssertTrue(app.staticTexts["pair.title"].waitForExistence(timeout: 5))
    }

    @MainActor
    func testPairingLaysOutInLandscape() throws {
        XCUIDevice.shared.orientation = .landscapeLeft
        defer { XCUIDevice.shared.orientation = .portrait }
        let app = launch()
        XCTAssertTrue(app.buttons["pair.connect"].exists)
    }
}

extension XCTestCase {
    @MainActor
    func enter(_ text: String, in app: XCUIApplication) {
        let field = app.textFields["pair.link"].exists ? app.textFields["pair.link"] : app.textViews["pair.link"]
        field.tap()
        field.typeText(text)
    }

    @MainActor
    func attach(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }
}
