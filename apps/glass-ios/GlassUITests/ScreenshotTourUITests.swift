import XCTest

/// Captures the main screens for design review. Run through scripts/screenshots.sh, which sets
/// GLASS_SHOTS (output directory) and GLASS_E2E_CONTROL (live harness); skipped otherwise.
final class ScreenshotTourUITests: XCTestCase {
    @MainActor
    func testChatRenderingInLightAndDark() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["GLASS_CHAT_FIXTURE"] == "1",
                          "Start scripts/e2e/chat-fixture.py to run the chat rendering tour")
        continueAfterFailure = false

        for appearance in ["light", "dark"] {
            let app = XCUIApplication()
            app.launchArguments = ["-GlassResetState", "-glass.theme", appearance]
            app.launch()
            XCTAssertTrue(app.staticTexts["pair.title"].waitForExistence(timeout: 10))
            let field = app.textFields["pair.link"].exists ? app.textFields["pair.link"] : app.textViews["pair.link"]
            field.tap()
            field.typeText("http://127.0.0.1:51998/#" + String(repeating: "a", count: 64))
            app.buttons["pair.connect"].tap()
            XCTAssertTrue(app.waitForThreadList(timeout: 20))

            func capture(_ name: String) {
                let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
                attachment.name = "\(appearance)-\(name)"
                attachment.lifetime = .keepAlways
                add(attachment)
            }

            app.descendants(matching: .any)["thread.reasoning"].firstMatch.tap()
            let conversation = app.descendants(matching: .any)["conversation"].firstMatch
            XCTAssertTrue(conversation.waitForExistence(timeout: 10))
            XCTAssertTrue(app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "I’ll compare the two build paths")).firstMatch
                .waitForExistence(timeout: 10))
            let reasoning = app.descendants(matching: .any)["timeline.reasoning"].firstMatch
            XCTAssertTrue(reasoning.exists)
            XCTAssertEqual(conversation.value as? String, "4 entries")
            capture("reasoning-collapsed")

            reasoning.tap()
            let completeReasoning = app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "without opening a second screen")).firstMatch
            XCTAssertTrue(completeReasoning.waitForExistence(timeout: 12))
            XCTAssertEqual(conversation.value as? String, "4 entries")
            capture("reasoning-expanded")

            app.buttons["conversation.back"].tap()
            XCTAssertTrue(app.waitForThreadList(timeout: 10))
            app.descendants(matching: .any)["thread.table"].firstMatch.tap()
            let table = app.descendants(matching: .any)["markdown.table"].firstMatch
            XCTAssertTrue(table.waitForExistence(timeout: 10))
            let header = app.descendants(matching: .any)["conversation.connection"].firstMatch
            XCTAssertTrue(header.label.contains("Swift workspace"))
            XCTAssertTrue(app.buttons["conversation.commands"].isHittable)
            XCTAssertTrue(app.buttons["conversation.menu"].isHittable)
            capture("wide-table")
            table.swipeLeft()
            XCTAssertTrue(app.staticTexts["Platform"].firstMatch.isHittable)
            capture("wide-table-scrolled")
            app.terminate()
        }
    }

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

        XCTAssertTrue(app.tabBars.buttons["Settings"].waitForExistence(timeout: 20))
        app.openSettings("host")
        XCTAssertTrue(app.descendants(matching: .any)["host.connected"].waitForExistence(timeout: 20))
        try save(app, "overview")

        let conversations = app.tabBars.buttons["Conversations"]
        guard conversations.exists else { return }
        conversations.tap()
        XCTAssertTrue(app.waitForThreadList(timeout: 20))
        try save(app, "threads")
        let row = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'thread.'")).firstMatch
        if row.waitForExistence(timeout: 10) { row.tap() } else { app.buttons["threads.new"].tap() }
        do {
            XCTAssertTrue(app.descendants(matching: .any)["conversation"].waitForExistence(timeout: 20))
            sleep(2)
            try save(app, "conversation")

            let composer = app.textViews["composer.field"]
            guard composer.waitForExistence(timeout: 10) else { return }
            composer.tap()
            composer.typeText("Summarize the open pull requests")
            try save(app, "composer-draft")
            app.buttons["composer.attach"].tap()
            try save(app, "composer-attach-menu")
            app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.3)).tap()
            app.buttons["composer.send"].tap()
            XCTAssertTrue(app.staticTexts.containing(NSPredicate(format: "label CONTAINS 'pull requests'")).firstMatch
                .waitForExistence(timeout: 30))
            composer.tap()
            composer.typeText("ask question")
            app.buttons["composer.send"].tap()
            let interaction = app.descendants(matching: .any)["interaction"]
            if interaction.waitForExistence(timeout: 30) {
                try save(app, "interaction-question")
                // Leave the shared thread idle for the next variant.
                app.buttons["interaction.decline"].tap()
                XCTAssertTrue(interaction.waitForNonExistence(timeout: 30))
            }

            app.buttons["conversation.commands"].tap()
            if app.buttons["palette.conversations"].firstMatch.waitForExistence(timeout: 15) {
                try save(app, "commands")
                let search = app.searchFields.firstMatch
                search.tap()
                search.typeText("usage")
                let usage = app.buttons["palette.usage"].firstMatch
                if usage.waitForExistence(timeout: 10) {
                    usage.tap()
                    _ = app.descendants(matching: .any)["command.result"].firstMatch.waitForExistence(timeout: 20)
                    try save(app, "command-usage")
                }
            }
        }

        // Relaunch (still paired) so Settings opens from a clean tab state.
        app.terminate()
        app.launchArguments = []
        app.launch()
        if app.waitForThreadList(timeout: 20) {
            let first = app.descendants(matching: .any).matching(NSPredicate(format: "identifier BEGINSWITH 'thread.'")).firstMatch
            if first.waitForExistence(timeout: 10) {
                first.tap()
                app.buttons["conversation.menu"].firstMatch.tap()
                app.buttons["conversation.details"].firstMatch.tap()
                if app.buttons["details.done"].firstMatch.waitForExistence(timeout: 10) {
                    sleep(1)
                    try save(app, "details")
                    app.buttons["details.done"].firstMatch.tap()
                }
            }
        }
        let desktopTab = app.tabBars.buttons["Desktop"]
        if desktopTab.waitForExistence(timeout: 10) {
            desktopTab.tap()
            _ = app.buttons["desktop.start.reusable"].firstMatch.waitForExistence(timeout: 20)
            try save(app, "desktop")
        }
        let settingsTab = app.tabBars.buttons["Settings"]
        XCTAssertTrue(settingsTab.waitForExistence(timeout: 20))
        app.openSettings(nil)
        XCTAssertTrue(app.buttons["settings.host"].waitForExistence(timeout: 10))
        try save(app, "settings")
        for route in ["appearance", "notifications", "voice", "diagnostics", "about"] {
            app.openSettings(route)
            sleep(1)
            try save(app, "settings-\(route)")
        }
        app.openSettings("defaults")
        let general = app.buttons["settings.section.general"].firstMatch
        for _ in 0..<6 where !(general.exists && general.isHittable) { app.swipeUp() }
        XCTAssertTrue(general.waitForExistence(timeout: 10))
        general.tap()
        XCTAssertTrue(app.navigationBars["General"].waitForExistence(timeout: 20))
        sleep(1)
        try save(app, "settings-general")
        let imageInput = app.switches["settings.image_input"].firstMatch
        for _ in 0..<8 where !imageInput.exists { app.swipeUp() }
        XCTAssertTrue(imageInput.exists)
    }
}

extension XCUIApplication {
    /// Settings tab, then optionally one of its submenus by route name (`settings.<route>`).
    @MainActor
    func openSettings(_ route: String?) {
        let tab = tabBars.buttons["Settings"]
        _ = tab.waitForExistence(timeout: 10)
        tab.tap()
        // A second tap pops the Settings stack back to its root.
        if !buttons["settings.host"].waitForExistence(timeout: 2) { tab.tap() }
        let back = navigationBars.buttons["Settings"].firstMatch
        while back.exists { back.tap() }
        guard let route else { return }
        let row = buttons["settings.\(route)"].firstMatch
        _ = row.waitForExistence(timeout: 10)
        for _ in 0..<4 where !row.isHittable { swipeUp() }
        row.tap()
    }

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
