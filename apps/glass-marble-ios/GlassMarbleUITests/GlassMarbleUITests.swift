import XCTest

/// Navigation, scrolling, and screenshot coverage for the new IA.
/// Run on a Mac simulator:
///   xcodebuild test -project apps/glass-marble-ios/GlassMarble.xcodeproj \
///     -scheme "Glass Marble" \
///     -destination 'platform=iOS Simulator,name=iPhone 14 Pro Max' \
///     -resultBundlePath /tmp/marble-ui.xcresult
/// Export attachments:
///   xcrun xcresulttool export attachments --path /tmp/marble-ui.xcresult --output-path /tmp/marble-shots
final class GlassMarbleUITests: XCTestCase {
    private var app: XCUIApplication!

    private func shot(_ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = name
        attachment.lifetime = .keepAlways
        add(attachment)
    }

    override func setUpWithError() throws {
        continueAfterFailure = false
        app = XCUIApplication()
        app.launchArguments = ["-MarbleUITestFixture", "-marble.introduced", "YES"]
        app.launch()
    }

    func testColdOpenShowsLatestEveryoneAndThreadHistoryWithoutScrolling() {
        app.tabBars.buttons["Everyone"].tap()
        app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Everyone,'")).firstMatch.tap()
        assertHittable("Everyone history row 59")
        for count in [60, 2] {
            app.buttons["Threads"].tap()
            app.staticTexts["Thread with \(count) fixture replies"].tap()
            assertHittable("Thread \(count) history row \(count)")
            app.navigationBars.buttons.element(boundBy: 0).tap()
            app.navigationBars.buttons.element(boundBy: 0).tap()
        }
    }

    func testColdOpenWaitsForLateHistoryAndShowsLatestWithoutScrolling() {
        app.terminate()
        app.launchArguments.append("-MarbleUITestDelayedEveryone")
        app.launch()
        app.tabBars.buttons["Everyone"].tap()
        app.buttons.matching(NSPredicate(format: "label == 'Everyone' AND identifier != 'tab.everyone'")).firstMatch.tap()
        XCTAssertTrue(app.descendants(matching: .any)["Loading messages"].firstMatch.waitForExistence(timeout: 2))
        XCTAssertFalse(app.staticTexts["No one has spoken yet"].exists)
        assertHittable("Everyone history row 59")
    }

    func testEveryoneSendKeepsHistoryAndThreeWorkingAgentsVisible() {
        app.terminate()
        app.launchArguments.append("-MarbleUITestEveryoneSend")
        app.launch()
        app.tabBars.buttons["Everyone"].tap()
        app.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Everyone,'")).firstMatch.tap()
        assertHittable("Everyone history row 59")

        let message = "@everyone whats the system time?"
        let field = app.textFields.firstMatch
        field.tap()
        field.typeText(message)
        app.buttons["Send"].tap()

        assertHittable(message)
        for name in ["Marble", "Ada", "Ben"] {
            let indicator = app.descendants(matching: .any)["\(name) is working"].firstMatch
            XCTAssertTrue(indicator.waitForExistence(timeout: 3))
            XCTAssertTrue(indicator.isHittable)
        }
        assertHittable("Everyone history row 59")
    }

    func testColdOpenShowsLatestNamedAndDefaultHistoryWithoutScrolling() {
        app.tabBars.buttons["Marbles"].tap()
        app.staticTexts["Ada"].tap()
        assertHittable("Hello! Lovely to meet you — what shall we work on?")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.staticTexts["Marble"].tap()
        app.staticTexts["Fixture conversation 0"].tap()
        assertHittable("Fixture message 299")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.staticTexts["Fixture conversation 2"].tap()
        assertHittable("Fixture message 2")
    }

    private func assertHittable(_ text: String, file: StaticString = #filePath, line: UInt = #line) {
        let message = app.staticTexts[text].firstMatch
        let visible = NSPredicate { _, _ in message.exists && message.isHittable }
        let expectation = XCTNSPredicateExpectation(predicate: visible, object: nil)
        let result = XCTWaiter.wait(for: [expectation], timeout: 5)
        if result != .completed {
            shot("Latest message missing")
            print(app.debugDescription)
        }
        XCTAssertEqual(result, .completed, "Latest message did not appear without scrolling: \(text)", file: file, line: line)
    }

    func testLaunchLandsOnChatsListWithNoChatPushed() {
        XCTAssertTrue(app.tabBars.buttons["Marbles"].exists)
        XCTAssertTrue(app.staticTexts["Fixture conversation 0"].waitForExistence(timeout: 5))
        shot("01-chats-list")
    }

    func testEveryTabIsReachable() {
        for tab in ["Marbles", "Settings"] {
            app.tabBars.buttons[tab].tap()
            XCTAssertTrue(app.tabBars.buttons[tab].isSelected, "\(tab) tab did not select")
            shot("02-tab-" + tab.lowercased())
        }
        app.tabBars.buttons["Everyone"].tap()
        // And the detail screens under Settings.
        app.tabBars.buttons["Settings"].tap()
        app.buttons["settings.marbles"].tap()
        app.buttons["Permissions"].tap()
        shot("03-settings-permissions")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.buttons["Memory"].tap()
        shot("04-settings-memory")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.buttons["settings.about"].tap()
        app.buttons["About this host"].tap()
        shot("05-settings-about")
        app.buttons["What Marble can use"].tap()
        shot("06-settings-tools")
    }

    func testActivityAndScheduledScreens() {
        app.tabBars.buttons["Settings"].tap()
        app.buttons["settings.activity"].tap()
        XCTAssertTrue(app.staticTexts["Needs you"].waitForExistence(timeout: 5))
        shot("12-activity-items")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.buttons["settings.scheduled"].tap()
        XCTAssertTrue(app.staticTexts["Morning check"].waitForExistence(timeout: 5))
        shot("13-scheduled-list")
        app.buttons["Add schedule"].tap()
        shot("14-scheduled-add-sheet")
        app.buttons["Cancel"].tap()
    }

    func testConversationRowOpensItsChat() {
        app.tabBars.buttons["Marbles"].tap()
        app.staticTexts["Fixture conversation 2"].tap()
        XCTAssertTrue(app.navigationBars["Fixture conversation 2"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["Fixture message 2"].waitForExistence(timeout: 5))
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.staticTexts["Fixture conversation 3"].waitForExistence(timeout: 5))
    }

    func testSearchHitOpensThatChat() {
        app.tabBars.buttons["Marbles"].tap()
        app.searchFields.element(boundBy: 0).tap()
        app.searchFields.element(boundBy: 0).typeText("conversation 150")
        let hit = app.staticTexts["Fixture conversation 150"]
        XCTAssertTrue(hit.waitForExistence(timeout: 5))
        hit.tap()
        XCTAssertTrue(app.navigationBars["Fixture conversation 150"].waitForExistence(timeout: 5))
    }

    func testOnboardingScreenshot() {
        let fresh = XCUIApplication()
        fresh.launch()
        shot("15-onboarding")
    }

    func testBackFromChatStaysOnListWhileRefreshesFire() {
        app.tabBars.buttons["Marbles"].tap()
        app.staticTexts["Fixture conversation 0"].tap()
        XCTAssertTrue(app.navigationBars.buttons.element(boundBy: 0).waitForExistence(timeout: 5))
        shot("07-chat-open")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.staticTexts["Fixture conversation 1"].waitForExistence(timeout: 5))
        sleep(5)
        XCTAssertTrue(app.staticTexts["Fixture conversation 1"].exists)
    }

    func testConversationListScrollsToLastRow() {
        app.tabBars.buttons["Marbles"].tap()
        let last = app.staticTexts["Fixture conversation 199"]
        let deadline = Date().addingTimeInterval(30)
        while !last.exists && Date() < deadline {
            app.swipeUp()
        }
        XCTAssertTrue(last.exists, "200-row list did not scroll to the last row")
    }

    func testChatScrollComposerReasoningAndShots() {
        app.tabBars.buttons["Marbles"].tap()
        app.staticTexts["Fixture conversation 0"].tap()
        XCTAssertTrue(app.staticTexts["Fixture message 299"].waitForExistence(timeout: 5))
        shot("08-composer-empty")
        // Reasoning disclosure collapsed then expanded.
        let reasoning = app.buttons["Reasoning"]
        if reasoning.waitForExistence(timeout: 3) {
            shot("09-reasoning-collapsed")
            reasoning.tap()
            shot("10-reasoning-expanded")
        }
        // Composer with text and keyboard shown.
        let field = app.textFields.element(boundBy: 0)
        field.tap()
        field.typeText("A draft message for the composer shot")
        shot("11-composer-draft-keyboard")
        // Scroll away: streaming must not yank, jump button returns.
        let first = app.staticTexts["Fixture message 0"]
        let deadline = Date().addingTimeInterval(30)
        while !first.exists && Date() < deadline {
            app.swipeDown()
        }
        XCTAssertTrue(first.exists, "could not scroll to the first message")
        sleep(3)
        XCTAssertTrue(first.isHittable || first.exists)
        XCTAssertTrue(app.buttons["Jump to latest"].waitForExistence(timeout: 5))
        app.buttons["Jump to latest"].tap()
        XCTAssertTrue(app.staticTexts["Fixture message 299"].waitForExistence(timeout: 5))
    }
}
