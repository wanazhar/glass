import XCTest

/// Drives a real glass-core gateway behind a simulated Tailscale Serve front end.
/// Run through scripts/e2e/run-e2e.sh, which sets GLASS_E2E_CONTROL; skipped otherwise.
final class LiveHostE2ETests: XCTestCase {
    private var control: URL!

    override func setUpWithError() throws {
        continueAfterFailure = false
        let value = ProcessInfo.processInfo.environment["GLASS_E2E_CONTROL"]
        try XCTSkipIf(value == nil, "Live-host E2E runs via scripts/e2e/run-e2e.sh")
        control = URL(string: value!)
    }

    private func call(_ path: String, method: String = "GET") throws -> String {
        var request = URLRequest(url: control.appending(path: path))
        request.httpMethod = method
        request.timeoutInterval = 60
        let done = expectation(description: path)
        nonisolated(unsafe) var result: Result<String, Error> = .failure(URLError(.unknown))
        URLSession.shared.dataTask(with: request) { data, response, error in
            if let error { result = .failure(error) }
            else if (response as? HTTPURLResponse)?.statusCode == 200, let data { result = .success(String(decoding: data, as: UTF8.self)) }
            else { result = .failure(URLError(.badServerResponse)) }
            done.fulfill()
        }.resume()
        wait(for: [done], timeout: 90)
        return try result.get().trimmingCharacters(in: .whitespacesAndNewlines)
    }

    @MainActor
    private func pair(_ app: XCUIApplication, link: String) {
        let field = app.textFields["pair.link"].exists ? app.textFields["pair.link"] : app.textViews["pair.link"]
        field.tap()
        if let current = field.value as? String, !current.isEmpty, current != field.placeholderValue {
            field.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: current.count))
        }
        field.typeText(link)
        app.buttons["pair.connect"].tap()
    }

    @MainActor
    func testTailnetPairingStatusDiagnosticsRevocationAndSecretEntry() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-GlassResetState"]
        app.launch()
        XCTAssertTrue(app.staticTexts["pair.title"].waitForExistence(timeout: 10))

        let ownerLink = try call("link/owner")
        XCTAssertTrue(ownerLink.contains("#owner="), ownerLink)
        pair(app, link: ownerLink)
        XCTAssertTrue(app.descendants(matching: .any)["host.connected"].waitForExistence(timeout: 20))
        XCTAssertEqual(app.staticTexts["host.workspace"].label, "ws")
        XCTAssertTrue(app.staticTexts["Tailscale HTTPS"].exists)
        XCTAssertTrue(app.descendants(matching: .any)["host.protocol"].label.contains("v1"))
        attach(app, "e2e-connected")

        app.tabBars.buttons["Diagnostics"].tap()
        XCTAssertTrue(app.staticTexts["diag.core"].firstMatch.waitForExistence(timeout: 20), "host diagnostics report should load")
        let hostSection = app.staticTexts["Host machine"]
        for _ in 0..<6 where !hostSection.exists { app.swipeUp() }
        XCTAssertTrue(hostSection.exists)
        attach(app, "e2e-diagnostics")

        _ = try call("revoke", method: "POST")
        app.tabBars.buttons["Host"].tap()
        app.buttons["host.refresh"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["pair.repair"].waitForExistence(timeout: 20))
        attach(app, "e2e-revoked")

        pair(app, link: try call("link/secret"))
        XCTAssertTrue(app.descendants(matching: .any)["host.connected"].waitForExistence(timeout: 20))
        XCTAssertTrue(app.staticTexts["Unlocked"].exists, "secret link should unlock secret entry")

        app.tabBars.buttons["Settings"].tap()
        app.buttons["settings.forget"].tap()
        app.buttons["settings.forget.confirm"].firstMatch.tap()
        XCTAssertTrue(app.staticTexts["pair.title"].waitForExistence(timeout: 10))
    }

    @MainActor
    func testLocalModePairsWithBearerToken() throws {
        let app = XCUIApplication()
        app.launchArguments = ["-GlassResetState"]
        app.launch()
        XCTAssertTrue(app.staticTexts["pair.title"].waitForExistence(timeout: 10))
        pair(app, link: try call("link/local"))
        XCTAssertTrue(app.descendants(matching: .any)["host.connected"].waitForExistence(timeout: 20))
        XCTAssertTrue(app.staticTexts["Local"].firstMatch.exists)
        attach(app, "e2e-local")
    }
}
