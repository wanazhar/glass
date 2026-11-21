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

    override func tearDownWithError() throws {
        // A failed network test must not leave the simulated tailnet down for the next one.
        if control != nil { _ = try? call("net/restore", method: "POST") }
    }

    private func call(_ path: String, method: String = "GET") throws -> String {
        var request = URLRequest(url: URL(string: path, relativeTo: control)!.absoluteURL)
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

    /// Pairing lands on Conversations; host status lives on the Host tab.
    @MainActor
    private func waitForHost(_ app: XCUIApplication) -> Bool {
        let tab = app.tabBars.buttons["Host"]
        guard tab.waitForExistence(timeout: 20) else { return false }
        tab.tap()
        return app.descendants(matching: .any)["host.connected"].waitForExistence(timeout: 20)
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
        XCTAssertTrue(waitForHost(app))
        XCTAssertTrue(text(app, containing: "Workspace, ws").exists)
        XCTAssertTrue(text(app, containing: "Tailscale HTTPS").exists)
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
        XCTAssertTrue(waitForHost(app))
        XCTAssertTrue(text(app, containing: "Unlocked").exists, "secret link should unlock secret entry")

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
        XCTAssertTrue(waitForHost(app))
        XCTAssertTrue(text(app, containing: "Transport, Local").exists)
        attach(app, "e2e-local")
    }

    @MainActor
    private func pairedApp() throws -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-GlassResetState"]
        app.launch()
        XCTAssertTrue(app.staticTexts["pair.title"].waitForExistence(timeout: 10))
        pair(app, link: try call("link/owner"))
        XCTAssertTrue(app.waitForThreadList(timeout: 20), "conversations load after pairing")
        return app
    }

    @MainActor
    private func text(_ app: XCUIApplication, containing value: String) -> XCUIElement {
        app.descendants(matching: .any).matching(NSPredicate(format: "label CONTAINS %@", value)).firstMatch
    }

    @MainActor
    private func waitForConnection(_ app: XCUIApplication, _ fragment: String, timeout: TimeInterval = 30) -> Bool {
        let pill = app.descendants(matching: .any)["conversation.connection"]
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if pill.exists, (pill.value as? String)?.contains(fragment) == true { return true }
            usleep(250_000)
        }
        return false
    }

    @MainActor
    private func connectionValue(_ app: XCUIApplication) -> String {
        app.descendants(matching: .any)["conversation.connection"].firstMatch.value as? String ?? "missing"
    }

    @MainActor
    private func open(_ app: XCUIApplication, thread id: String) {
        let row = app.descendants(matching: .any)["thread.\(id)"].firstMatch
        XCTAssertTrue(row.waitForExistence(timeout: 20), "thread \(id) listed")
        row.tap()
        XCTAssertTrue(app.descendants(matching: .any)["conversation"].waitForExistence(timeout: 20))
    }

    @MainActor
    func testThreadListSearchOlderHistoryAndNewThread() throws {
        let seeded = try call("thread/new?title=Seeded%20history", method: "POST")
        _ = try call("thread/seed?id=\(seeded)&pairs=40&size=20000", method: "POST")
        let other = try call("thread/new?title=Grocery%20notes", method: "POST")
        let app = try pairedApp()
        XCTAssertTrue(app.descendants(matching: .any)["thread.\(seeded)"].waitForExistence(timeout: 20))
        XCTAssertTrue(app.descendants(matching: .any)["thread.\(other)"].exists)
        attach(app, "e2e-threads")

        let search = app.searchFields.firstMatch
        if !search.exists { app.swipeDown() }
        XCTAssertTrue(search.waitForExistence(timeout: 5))
        search.tap()
        search.typeText("Grocery")
        XCTAssertTrue(app.descendants(matching: .any)["thread.\(other)"].waitForExistence(timeout: 10))
        let hidden = NSPredicate(format: "exists == false")
        expectation(for: hidden, evaluatedWith: app.descendants(matching: .any)["thread.\(seeded)"])
        waitForExpectations(timeout: 10)
        attach(app, "e2e-threads-search")
        let dismiss = app.buttons["Close"].exists ? app.buttons["Close"] : app.buttons["Cancel"]
        dismiss.firstMatch.tap()

        open(app, thread: seeded)
        let older = app.buttons["conversation.older"]
        XCTAssertTrue(text(app, containing: "Seeded answer 40").waitForExistence(timeout: 20), "latest window shows the newest answer")
        XCTAssertFalse(text(app, containing: "Ready when you are.").exists, "first record is outside the latest window")
        let conversation = app.descendants(matching: .any)["conversation"].firstMatch
        let statusBar = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.005))
        for _ in 0..<12 {
            guard let value = conversation.value as? String, value.contains("earlier") else { break }
            // Tapping the status bar scrolls to the top, where the older-history control lives.
            for _ in 0..<5 where !older.isHittable { statusBar.tap(); sleep(1) }
            guard older.isHittable else { break }
            older.tap()
            let loaded = NSPredicate(format: "value != %@", value)
            expectation(for: loaded, evaluatedWith: conversation)
            waitForExpectations(timeout: 20)
        }
        XCTAssertEqual(conversation.value as? String, "82 entries", "paging with before loads every record exactly once")
        attach(app, "e2e-older-history")

        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["threads.new"].waitForExistence(timeout: 10))
        app.buttons["threads.new"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["conversation"].waitForExistence(timeout: 20))
        XCTAssertTrue(app.descendants(matching: .any)["conversation.empty"].waitForExistence(timeout: 20))
        attach(app, "e2e-new-thread")
        app.buttons["conversation.menu"].tap()
        app.buttons["conversation.close"].tap()
        XCTAssertTrue(app.staticTexts["threads.count"].waitForExistence(timeout: 20))
    }

    @MainActor
    func testLiveEventsResumeAfterDropBackgroundAndResetAfterOverflow() throws {
        let thread = try call("thread/new?title=Live%20thread", method: "POST")
        let app = try pairedApp()
        open(app, thread: thread)
        XCTAssertTrue(waitForConnection(app, "Live"), "event stream connects")

        _ = try call("thread/send?id=\(thread)&text=hello%20from%20the%20web&wait=1", method: "POST")
        XCTAssertTrue(text(app, containing: "Echo from the host").waitForExistence(timeout: 20), "web-originated turn streams in live")
        XCTAssertTrue(app.descendants(matching: .any)["timeline.code"].firstMatch.exists, "fenced code renders as a code block")
        attach(app, "e2e-live")

        _ = try call("net/drop", method: "POST")
        XCTAssertTrue(waitForConnection(app, "Reconnecting"), "dropped stream shows reconnecting")
        attach(app, "e2e-reconnecting")
        _ = try call("thread/send?id=\(thread)&text=sent%20while%20offline&wait=1", method: "POST")
        _ = try call("net/restore", method: "POST")
        XCTAssertTrue(text(app, containing: "sent while offline").waitForExistence(timeout: 40), "resume replays missed events")
        XCTAssertTrue(waitForConnection(app, "Live, 0 resets"), "small gap resumes without a reset: \(connectionValue(app))")
        let resumes = try JSONDecoder().decode([String].self, from: Data(try call("net/events").utf8))
        XCTAssertTrue(resumes.dropFirst().contains { !$0.isEmpty }, "reconnect sends Last-Event-ID: \(resumes)")

        XCUIDevice.shared.press(.home)
        _ = try call("thread/send?id=\(thread)&text=sent%20while%20backgrounded&wait=1", method: "POST")
        app.activate()
        XCTAssertTrue(text(app, containing: "sent while backgrounded").waitForExistence(timeout: 40), "foregrounding resumes the stream")

        _ = try call("net/drop", method: "POST")
        XCTAssertTrue(waitForConnection(app, "Reconnecting"))
        _ = try call("thread/send?id=\(thread)&text=flood%203000&wait=1", method: "POST")
        _ = try call("net/restore", method: "POST")
        XCTAssertTrue(waitForConnection(app, "1 resets", timeout: 60), "cursor older than retention triggers a reset snapshot: \(connectionValue(app))")
        XCTAssertTrue(text(app, containing: "xxxxxxxxxx").waitForExistence(timeout: 20), "reset reloads history with the missed turn")
        attach(app, "e2e-reset")
    }
}
