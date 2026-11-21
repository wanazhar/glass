import XCTest

/// Captures the main screens for design review. Run through scripts/screenshots.sh, which sets
/// GLASS_SHOTS (output directory) and GLASS_E2E_CONTROL (live harness); skipped otherwise.
final class ScreenshotTourUITests: XCTestCase {
    @MainActor
    func testCaptureScreens() throws {
        let env = ProcessInfo.processInfo.environment
        guard let path = env["GLASS_SHOTS"] else { throw XCTSkip("Screenshot tour runs via scripts/screenshots.sh") }
        let shots = URL(fileURLWithPath: path)
        let prefix = env["GLASS_SHOT_PREFIX"] ?? "shot"
        try FileManager.default.createDirectory(at: shots, withIntermediateDirectories: true)
        func save(_ app: XCUIApplication, _ name: String) throws {
            sleep(1)
            try app.screenshot().pngRepresentation.write(to: shots.appending(path: "\(prefix)-\(name).png"))
        }

        let app = XCUIApplication()
        app.launchArguments = ["-GlassResetState"]
        app.launch()
        XCTAssertTrue(app.staticTexts["pair.title"].waitForExistence(timeout: 10))
        try save(app, "pairing")

        guard let control = env["GLASS_E2E_CONTROL"].flatMap(URL.init(string:)) else { return }
        let link = String(decoding: try Data(contentsOf: control.appending(path: "link/owner")), as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let field = app.textFields["pair.link"].exists ? app.textFields["pair.link"] : app.textViews["pair.link"]
        field.tap()
        field.typeText(link)
        try save(app, "pairing-filled")
        app.buttons["pair.connect"].tap()

        let hostTab = app.tabBars.buttons["Host"]
        XCTAssertTrue(hostTab.waitForExistence(timeout: 20))
        hostTab.tap()
        XCTAssertTrue(app.descendants(matching: .any)["host.connected"].waitForExistence(timeout: 20))
        try save(app, "overview")

        let conversations = app.tabBars.buttons["Conversations"]
        guard conversations.exists else { return }
        conversations.tap()
        XCTAssertTrue(app.waitForThreadList(timeout: 20))
        try save(app, "threads")
        let row = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'thread.'")).firstMatch
        if row.waitForExistence(timeout: 10) {
            row.tap()
            XCTAssertTrue(app.descendants(matching: .any)["conversation"].waitForExistence(timeout: 20))
            sleep(2)
            try save(app, "conversation")
        }
    }
}

extension XCUIApplication {
    /// The conversation list finished loading: either its count header or the empty state is shown.
    @MainActor
    func waitForThreadList(timeout: TimeInterval) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        repeat {
            if staticTexts["threads.count"].exists || descendants(matching: .any)["threads.empty"].exists { return true }
            Thread.sleep(forTimeInterval: 0.25)
        } while Date() < deadline
        return false
    }
}
