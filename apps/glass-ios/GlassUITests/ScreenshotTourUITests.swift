import XCTest

/// Captures the main screens for design review. Run through scripts/screenshots.sh, which sets
/// GLASS_SHOTS (output directory) and GLASS_E2E_CONTROL (live harness); skipped otherwise.
final class ScreenshotTourUITests: XCTestCase {
    @MainActor
    func testAgentCardsAndSeededChild() throws {
        let environment = ProcessInfo.processInfo.environment
        try XCTSkipUnless(environment["GLASS_CHAT_FIXTURE"] == "1", "Start scripts/e2e/chat-fixture.py")
        continueAfterFailure = false
        let tablet = UIDevice.current.userInterfaceIdiom == .pad
        let appearances = environment["GLASS_ROUND8_APPEARANCE"].map { [$0] } ?? ["light", "dark"]
        for appearance in appearances {
            let app = pairFixture(appearance)

            app.buttons["thread.delegation"].tap()
            let group = app.buttons["agents.group"].firstMatch
            XCTAssertTrue(group.waitForExistence(timeout: 10))
            for count in ["3 agents", "1 working", "1 done", "1 failed"] {
                XCTAssertTrue(group.label.contains(count), group.label)
            }
            try captureRound8(appearance, "agents-collapsed")
            group.tap()
            let delivery = app.buttons["agents.child.delivery"]
            XCTAssertTrue(delivery.waitForExistence(timeout: 5))
            XCTAssertTrue(delivery.label.contains("Editing alert_router.py"))
            XCTAssertFalse(delivery.label.contains("**"))
            let done = app.buttons["agents.child.tests"].label
            let failed = app.buttons["agents.child.replay"].label
            XCTAssertTrue(done.contains("Done") && done.contains("All 18 delivery tests pass"))
            XCTAssertTrue(failed.contains("Failed") && failed.contains("The replay fixture is missing"))
            try captureRound8(appearance, "agents-expanded")
            delivery.tap()
            let seededReply = app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "The retry timer can enqueue")).firstMatch
            XCTAssertTrue(seededReply.exists, "The child timeline already contains the prefetched reply")
            XCTAssertFalse(app.staticTexts["Loading messages…"].exists)
            XCTAssertFalse(app.staticTexts["Syncing messages…"].exists)
            let back = app.buttons["agent.parent"]
            let stop = app.buttons["agent.stop"]
            XCTAssertTrue(back.exists && stop.isEnabled)
            if tablet {
                XCTAssertTrue(group.exists, "The parent timeline stays visible next to the child")
                XCTAssertFalse(app.staticTexts["Date Unknown"].exists)
                XCTAssertFalse(app.buttons["threads.agent.delivery"].exists)
                app.buttons["threads.agents.delegation"].tap()
                XCTAssertTrue(app.buttons["threads.agent.delivery"].waitForExistence(timeout: 5))
            } else {
                XCTAssertTrue(app.textViews["composer.field"].waitForNonExistence(timeout: 5))
            }
            try captureRound8(appearance, tablet ? "agent-inspector" : "agent-thread")
            if environment["GLASS_ROUND8_WALKTHROUGH"] == "1" {
                stop.tap()
                expectation(for: NSPredicate(format: "enabled == false"), evaluatedWith: stop)
                waitForExpectations(timeout: 5)
            }
            back.tap()
            XCTAssertTrue(group.waitForExistence(timeout: 5))
            XCTAssertFalse(app.buttons["agent.parent"].exists)
            app.terminate()
        }
    }

    @MainActor
    func testRoundEightStreamingAndHeader() throws {
        let environment = ProcessInfo.processInfo.environment
        try XCTSkipUnless(environment["GLASS_CHAT_FIXTURE"] == "1", "Start scripts/e2e/chat-fixture.py")
        continueAfterFailure = false
        let appearances = environment["GLASS_ROUND8_APPEARANCE"].map { [$0] } ?? ["light", "dark"]
        for appearance in appearances {
            let app = pairFixture(appearance)
            app.buttons["thread.stream-review"].tap()
            let partial = app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "The device build and the simulator build")).firstMatch
            XCTAssertTrue(partial.waitForExistence(timeout: 8))
            let finished = app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "Verify the build number from the bundle")).firstMatch
            XCTAssertTrue(finished.waitForExistence(timeout: 20))
            XCTAssertTrue(app.buttons["composer.stop"].waitForNonExistence(timeout: 5))
            XCTAssertFalse(app.buttons["composer.model"].exists, "No empty model rail is shown")
            try captureRound8(appearance, "composer")
            let scroll = app.scrollViews.firstMatch
            let from = scroll.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.35))
            let to = scroll.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.66))
            from.press(forDuration: 0.05, thenDragTo: to, withVelocity: .slow, thenHoldForDuration: 0.2)
            XCTAssertTrue(app.buttons["conversation.latest"].waitForExistence(timeout: 5))
            try captureRound8(appearance, "header-fade")
            app.terminate()
        }
    }

    @MainActor
    func testRoundEightLiveActivityAgentCount() throws {
        let environment = ProcessInfo.processInfo.environment
        try XCTSkipUnless(environment["GLASS_CHAT_FIXTURE"] == "1", "Start scripts/e2e/chat-fixture.py")
        continueAfterFailure = false
        let appearance = environment["GLASS_ROUND8_APPEARANCE"] ?? "light"
        let app = pairFixture(appearance)
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        app.openSettings("connection")
        let keep = app.switches["settings.keepAlive"].firstMatch
        XCTAssertTrue(keep.waitForExistence(timeout: 10))
        keep.coordinate(withNormalizedOffset: CGVector(dx: 0.94, dy: 0.5)).tap()
        let allow = springboard.buttons["Allow While Using App"]
        if allow.waitForExistence(timeout: 3) { allow.tap() }
        XCTAssertEqual(keep.value as? String, "1")
        app.closeSettings()
        XCTAssertTrue(app.waitForThreadList(timeout: 10))
        app.buttons["thread.delegation"].tap()
        let group = app.buttons["agents.group"].firstMatch
        XCTAssertTrue(group.waitForExistence(timeout: 10) && group.label.contains("1 working"))
        XCUIDevice.shared.press(.home)
        if UIDevice.current.userInterfaceIdiom == .phone {
            let activity = springboard.descendants(matching: .any)
                .matching(NSPredicate(format: "identifier CONTAINS 'element-identifier:activity:'")).firstMatch
            XCTAssertTrue(activity.waitForExistence(timeout: 10))
            try captureRound8(appearance, "island")
            activity.press(forDuration: 1.2)
            XCTAssertTrue(springboard.buttons["Disconnect"].firstMatch.waitForExistence(timeout: 5))
            try captureRound8(appearance, "island-expanded")
            XCUIDevice.shared.press(.home)
        }
        let lock = NSSelectorFromString("pressLockButton")
        XCTAssertTrue(XCUIDevice.shared.responds(to: lock))
        XCUIDevice.shared.perform(lock)
        sleep(2)
        XCUIDevice.shared.perform(lock)
        sleep(3)
        for label in ["Allow", "Always Allow"] {
            let permission = springboard.buttons[label].firstMatch
            if permission.exists {
                permission.tap()
                XCTAssertTrue(permission.waitForNonExistence(timeout: 5))
                break
            }
        }
        try captureRound8(appearance, "lock-screen")
        let homeIndicator = springboard.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.992))
        let center = springboard.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.25))
        homeIndicator.press(forDuration: 0.05, thenDragTo: center)
        app.activate()
        XCTAssertTrue(group.waitForExistence(timeout: 10))
        app.terminate()
    }

    @MainActor
    private func pairFixture(_ appearance: String) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-GlassResetState", "-glass.theme", appearance]
        app.launch()
        XCTAssertTrue(app.staticTexts["pair.title"].waitForExistence(timeout: 10))
        let field = app.textFields["pair.link"].exists ? app.textFields["pair.link"] : app.textViews["pair.link"]
        field.tap()
        field.typeText("http://127.0.0.1:51998/#" + String(repeating: "a", count: 64))
        app.buttons["pair.connect"].tap()
        XCTAssertTrue(app.waitForThreadList(timeout: 20))
        return app
    }

    @MainActor
    private func captureRound8(_ appearance: String, _ name: String) throws {
        let device = UIDevice.current.userInterfaceIdiom == .pad ? "ipad" : "iphone"
        let filename = "\(device)-\(appearance)-\(name)"
        let shot = XCUIScreen.main.screenshot()
        let attachment = XCTAttachment(screenshot: shot)
        attachment.name = filename
        attachment.lifetime = .keepAlways
        add(attachment)
        if let directory = ProcessInfo.processInfo.environment["GLASS_SHOTS"] {
            try shot.pngRepresentation.write(to: URL(filePath: directory).appending(path: filename + ".png"))
        }
    }

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
    func testExpandingLongReasoningKeepsItsBeginningVisible() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["GLASS_CHAT_FIXTURE"] == "1",
                          "Start scripts/e2e/chat-fixture.py to run the chat rendering tour")
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-GlassResetState"]
        app.launch()
        XCTAssertTrue(app.staticTexts["pair.title"].waitForExistence(timeout: 10))
        let field = app.textFields["pair.link"].exists ? app.textFields["pair.link"] : app.textViews["pair.link"]
        field.tap()
        field.typeText("http://127.0.0.1:51998/#" + String(repeating: "a", count: 64))
        app.buttons["pair.connect"].tap()
        XCTAssertTrue(app.waitForThreadList(timeout: 20))

        app.descendants(matching: .any)["thread.long-reasoning"].firstMatch.tap()
        let reasoning = app.descendants(matching: .any)["timeline.reasoning"].firstMatch
        XCTAssertTrue(reasoning.waitForExistence(timeout: 10))
        reasoning.tap()
        let fullText = app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "Beginning of the long reasoning")).firstMatch
        XCTAssertTrue(fullText.waitForExistence(timeout: 10))
        let header = app.buttons["conversation.back"]
        XCTAssertGreaterThanOrEqual(fullText.frame.minY, header.frame.maxY)
        XCTAssertLessThan(fullText.frame.minY, app.frame.midY)
    }

    @MainActor
    func testExpandingLatestReasoningAfterLongHistory() throws {
        try checkReasoningAfterLongHistory(scrolledUp: false)
    }

    @MainActor
    func testExpandingScrolledUpReasoningAfterLongHistory() throws {
        try checkReasoningAfterLongHistory(scrolledUp: true)
    }

    @MainActor
    private func checkReasoningAfterLongHistory(scrolledUp: Bool) throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["GLASS_CHAT_FIXTURE"] == "1",
                          "Start scripts/e2e/chat-fixture.py to run the chat rendering tour")
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-GlassResetState"]
        app.launch()
        XCTAssertTrue(app.staticTexts["pair.title"].waitForExistence(timeout: 10))
        let field = app.textFields["pair.link"].exists ? app.textFields["pair.link"] : app.textViews["pair.link"]
        field.tap()
        field.typeText("http://127.0.0.1:51998/#" + String(repeating: "a", count: 64))
        app.buttons["pair.connect"].tap()
        XCTAssertTrue(app.waitForThreadList(timeout: 20))

        let thread = scrolledUp ? "thread.scrolled-reasoning" : "thread.deep-reasoning"
        app.descendants(matching: .any)[thread].firstMatch.tap()
        let conversation = app.descendants(matching: .any)["conversation"].firstMatch
        XCTAssertTrue(conversation.waitForExistence(timeout: 10))
        let header = app.buttons["conversation.back"]
        let reasoning = app.buttons.matching(identifier: "timeline.reasoning")
        XCTAssertTrue(reasoning.firstMatch.waitForExistence(timeout: 10))
        let separator = app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", "Separate the two reasoning segments.")).firstMatch
        if scrolledUp {
            let start = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.35))
            let end = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.48))
            for _ in 0..<5 {
                start.press(forDuration: 0.1, thenDragTo: end, withVelocity: .slow, thenHoldForDuration: 0)
                if reasoning.allElementsBoundByIndex.contains(where: { $0.isHittable && $0.frame.maxY <= separator.frame.minY }) { break }
            }
        }
        let buttons = reasoning.allElementsBoundByIndex.filter {
            $0.isHittable && $0.frame.minY >= header.frame.maxY && (!scrolledUp || $0.frame.maxY <= separator.frame.minY)
        }
        let block = try XCTUnwrap(scrolledUp ? buttons.first : buttons.last)
        block.tap()
        let opening = scrolledUp ? "Beginning of the earlier block." : "Beginning of the latest block."
        let text = app.staticTexts.matching(NSPredicate(format: "label BEGINSWITH %@", opening)).firstMatch
        XCTAssertTrue(text.waitForExistence(timeout: 10))
        XCTAssertGreaterThanOrEqual(text.frame.minY, header.frame.maxY)
        XCTAssertLessThan(text.frame.minY, app.frame.midY)
        if scrolledUp {
            let initialTop = text.frame.minY
            app.swipeUp()
            XCTAssertLessThan(text.frame.minY, initialTop - 50)
        }
    }

    @MainActor
    func testEarlierHistoryPreservesTheExpandedReasoningPosition() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["GLASS_CHAT_FIXTURE"] == "1",
                          "Start scripts/e2e/chat-fixture.py to run the chat rendering tour")
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-GlassResetState"]
        app.launch()
        XCTAssertTrue(app.staticTexts["pair.title"].waitForExistence(timeout: 10))
        let field = app.textFields["pair.link"].exists ? app.textFields["pair.link"] : app.textViews["pair.link"]
        field.tap()
        field.typeText("http://127.0.0.1:51998/#" + String(repeating: "a", count: 64))
        app.buttons["pair.connect"].tap()
        XCTAssertTrue(app.waitForThreadList(timeout: 20))

        app.descendants(matching: .any)["thread.paged-reasoning"].firstMatch.tap()
        let reasoning = app.descendants(matching: .any)["timeline.reasoning"].firstMatch
        XCTAssertTrue(reasoning.waitForExistence(timeout: 10))
        reasoning.tap()
        let before = app.staticTexts.containing(NSPredicate(format: "label BEGINSWITH %@", "Already visible reasoning")).firstMatch
        XCTAssertTrue(before.waitForExistence(timeout: 10))
        let earlier = app.buttons["conversation.older"]
        if !earlier.isHittable { app.swipeDown() }
        XCTAssertTrue(earlier.isHittable)
        let previousBottom = before.frame.maxY
        earlier.tap()

        let after = app.staticTexts.containing(NSPredicate(format: "label BEGINSWITH %@", "Earlier reasoning")).firstMatch
        XCTAssertTrue(after.waitForExistence(timeout: 10))
        XCTAssertTrue(after.label.contains("Already visible reasoning"))
        XCTAssertEqual(after.frame.maxY, previousBottom, accuracy: 24)
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
        if tab.waitForExistence(timeout: 2) {
            tab.tap()
            // A second tap pops the Settings stack back to its root.
            if !buttons["settings.host"].waitForExistence(timeout: 2) { tab.tap() }
        } else if !buttons["settings.host"].exists {
            // Settings opens from the Threads panel's footer.
            _ = waitForThreadList(timeout: 10)
            buttons["tab.settings"].firstMatch.tap()
            _ = buttons["settings.host"].waitForExistence(timeout: 5)
        }
        let back = navigationBars.buttons["Settings"].firstMatch
        while back.exists { back.tap() }
        guard let route else { return }
        let row = buttons["settings.\(route)"].firstMatch
        _ = row.waitForExistence(timeout: 10)
        for _ in 0..<4 where !row.isHittable { swipeUp() }
        row.tap()
    }

    /// Leaves Settings, whether it is a tab or the sheet the Threads panel opens.
    @MainActor
    func closeSettings() {
        let back = navigationBars.buttons["Settings"].firstMatch
        while back.exists { back.tap() }
        let done = buttons["Done"].firstMatch
        if done.waitForExistence(timeout: 2) { done.tap() } else { tabBars.buttons["Conversations"].tap() }
    }

    /// Opens the compact drawer if needed and waits for its loaded content.
    @MainActor
    func waitForThreadList(timeout: TimeInterval) -> Bool {
        let drawer = buttons["conversation.back"].firstMatch
        if !buttons["threads.new"].exists, drawer.waitForExistence(timeout: timeout) { drawer.tap() }
        let settled = descendants(matching: .any)
            .matching(NSPredicate(format: "identifier IN %@", ["threads.ready", "threads.empty"]))
            .firstMatch
        return settled.waitForExistence(timeout: timeout)
    }
}
