import XCTest

final class MarbleEvidenceTests: XCTestCase {
    private let app = XCUIApplication()

    private func shot(_ name: String) {
        let image = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        image.name = name
        image.lifetime = .keepAlways
        add(image)
    }

    private func button(_ label: String) -> XCUIElement {
        app.buttons.matching(NSPredicate(format: "label == %@ OR identifier == %@", label, label)).firstMatch
    }

    private func check(_ element: XCUIElement, timeout: Double = 9) {
        XCTAssertTrue(element.waitForExistence(timeout: timeout), "Missing \(element)")
    }

    private func back() { app.navigationBars.buttons.element(boundBy: 0).tap() }

    private func start(address: String = "http://127.0.0.1:4783") throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["MARBLE_EVIDENCE"] == "1",
                          "Requires a seeded local Marble host")
        continueAfterFailure = false
        app.launch()
        let connect = button("Connect to host")
        let picker = app.staticTexts["Ada"].firstMatch
        if connect.waitForExistence(timeout: 5) {
            shot("disconnected")
            connect.tap()
            let host = app.textFields.firstMatch
            check(host)
            shot("pairing")
            host.tap()
            host.typeText(address)
            let token = app.secureTextFields.firstMatch
            token.tap()
            token.typeText("marble-local-test")
            if app.keyboards.buttons["Done"].exists { app.keyboards.buttons["Done"].tap() }
            button("Connect").tap()
        }
        check(picker, timeout: 18)
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        for action in ["Not Now", "Don't Allow"] {
            let systemButton = springboard.buttons[action].firstMatch
            let appButton = app.buttons[action].firstMatch
            if systemButton.exists { systemButton.tap() }
            else if appButton.exists { appButton.tap() }
        }
        if app.buttons["Allow"].firstMatch.exists { app.buttons["Allow"].firstMatch.tap() }
        shot("picker")
    }

    func testHostedScreens() throws {
        try start()
        app.staticTexts["Cleo"].firstMatch.tap()
        check(app.staticTexts["Nothing yet"])
        shot("empty")
        back()
        button("New Marble").tap()
        check(app.textFields["Name your Marble"])
        shot("new-marble")
        button("Cancel").tap()
        app.staticTexts["Ada"].firstMatch.tap()
        check(app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "packing list")).firstMatch)
        shot("chat")
        button("About this Marble").tap()
        check(app.staticTexts["Profile"].firstMatch)
        shot("profile")
        button("Memory").tap()
        check(app.staticTexts["Keep Sunday mornings free."].firstMatch)
        shot("memory")
        app.staticTexts["Keep Sunday mornings free."].firstMatch.tap()
        check(app.staticTexts["Edit memory"].firstMatch)
        shot("edit-memory")
        button("Cancel").tap()
        back()
        back()
        button("Chat options").tap()
        button("Threads").tap()
        check(app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "packing list")).firstMatch)
        shot("threads")
        back()
        back()
        app.tabBars.buttons["Everyone"].tap()
        check(app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "What should we plan first")).firstMatch)
        shot("everyone")
        button("Find").tap()
        let search = app.searchFields.firstMatch
        check(search)
        search.tap()
        search.typeText("plan")
        check(app.staticTexts.containing(NSPredicate(format: "label CONTAINS[c] %@", "What should we plan first")).firstMatch)
        shot("find")
    }

    func testHostedActivityAndSettings() throws {
        try start()
        app.tabBars.buttons["Settings"].tap()
        check(button("settings.connection"))
        shot("settings")
        button("settings.activity").tap()
        check(app.staticTexts["Needs you"])
        shot("activity")
        button("Review").tap()
        check(app.staticTexts["Review suggestion"].firstMatch)
        shot("confirm-memory")
    }

    func testPairingError() throws {
        guard let address = ProcessInfo.processInfo.environment["MARBLE_PAIRING_EVIDENCE_URL"] else {
            throw XCTSkip("Requires a deliberately delayed local Marble host")
        }
        continueAfterFailure = false
        app.launch()
        let connect = button("Connect to host")
        check(connect)
        connect.tap()
        let host = app.textFields.firstMatch
        check(host)
        shot("pairing")
        host.tap()
        host.typeText(address)
        let token = app.secureTextFields.firstMatch
        token.tap()
        token.typeText("invalid-test-token")
        if app.keyboards.buttons["Done"].exists { app.keyboards.buttons["Done"].tap() }
        button("Connect").tap()
        check(app.staticTexts["Connecting to your host…"])
        shot("connecting")
        check(app.staticTexts.containing(NSPredicate(format: "label BEGINSWITH %@", "The host rejected that token")).firstMatch)
        shot("error")
    }

    func testHostedTablet() throws {
        try start()
        check(app.staticTexts["Choose a Marble to start a conversation."])
        shot("ipad-home")
        app.staticTexts["Ada"].firstMatch.tap()
        check(app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "packing list")).firstMatch)
        XCTAssertTrue(app.staticTexts["Cleo"].firstMatch.isHittable)
        shot("ipad-chat")
    }

    func testHostedXL() throws {
        try start()
        app.swipeUp()
        let create = button("New Marble")
        check(create)
        XCTAssertTrue(create.isHittable)
        create.tap()
        check(app.textFields["Name your Marble"])
        shot("new-marble-xl")
        app.swipeUp()
        check(app.staticTexts["Look"])
        shot("new-marble-xl-scrolled")
    }

    func testHostedRecovery() throws {
        guard let address = ProcessInfo.processInfo.environment["MARBLE_RECOVERY_EVIDENCE_URL"],
              let url = URL(string: address) else {
            throw XCTSkip("Requires the local recovery relay")
        }
        func select(_ mode: String) {
            let changed = expectation(description: "Relay set to \(mode)")
            URLSession.shared.dataTask(with: url.appending(path: "__evidence/\(mode)")) { _, response, _ in
                XCTAssertEqual((response as? HTTPURLResponse)?.statusCode, 204)
                changed.fulfill()
            }.resume()
            wait(for: [changed], timeout: 5)
        }

        select("live")
        defer { select("live") }
        try start(address: address)
        app.terminate()
        select("offline")
        app.launch()
        check(app.staticTexts["Marble is offline"], timeout: 15)
        shot("offline")
        app.terminate()
        select("expired")
        app.launch()
        check(button("Replace token"), timeout: 15)
        shot("expired-token")
        button("Replace token").tap()
        check(app.staticTexts["Your host is still saved. Enter a new token to reconnect."])
        shot("replace-token")
    }

    func testHostedNamedStop() throws {
        guard let address = ProcessInfo.processInfo.environment["MARBLE_STREAM_EVIDENCE_URL"] else {
            throw XCTSkip("Requires the local streaming fixture")
        }
        try start(address: address)
        let marble = app.buttons.containing(.staticText, identifier: "Ada").firstMatch
        check(marble)
        marble.tap()
        let composer = app.descendants(matching: .any)["composer.field"].firstMatch
        check(composer)
        composer.tap()
        composer.typeText("Help me pick a shaded walking trail.")
        button("Send").tap()
        let stop = button("Stop")
        check(stop)
        check(app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "short list of trails")).firstMatch)
        check(app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "Stop and resend")).firstMatch)
        XCTAssertFalse(button("Steer").exists)
        if button("Hide keyboard").exists { button("Hide keyboard").tap() }
        shot("named-streaming-stop")
        stop.tap()
        XCTAssertTrue(stop.waitForNonExistence(timeout: 15))
        shot("named-stopped")
    }

    func testEnableReduceMotion() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["MARBLE_REDUCE_MOTION_EVIDENCE"] == "1",
                          "Opt-in simulator accessibility setup")
        let settings = XCUIApplication(bundleIdentifier: "com.apple.Preferences")
        settings.launch()
        let accessibility = settings.cells.containing(.staticText, identifier: "Accessibility").firstMatch
        if !accessibility.isHittable { settings.swipeUp() }
        check(accessibility)
        accessibility.tap()
        let motion = settings.cells.containing(.staticText, identifier: "Motion").firstMatch
        if !motion.isHittable { settings.swipeUp() }
        check(motion)
        motion.tap()
        let toggle = settings.switches["Reduce Motion"]
        check(toggle)
        if toggle.value as? String != "1" {
            toggle.coordinate(withNormalizedOffset: CGVector(dx: 0.92, dy: 0.5)).tap()
        }
        let enabled = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value == '1'"), object: toggle)
        XCTAssertEqual(XCTWaiter.wait(for: [enabled], timeout: 5), .completed)
        shot("reduce-motion-enabled")
        settings.terminate()
    }
}
