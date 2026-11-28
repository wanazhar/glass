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
        app.launchArguments = ["-MarbleUITestFixture"]
        app.launch()
    }

    func testColdOpenShowsLatestEveryoneAndThreadHistoryWithoutScrolling() {
        app.tabBars.buttons["Everyone"].tap()
        app.staticTexts["Earlier messages"].tap()
        assertHittable("Everyone history row 59")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        for count in [60, 2] {
            app.staticTexts["Thread with \(count) fixture replies"].tap()
            assertHittable("Thread \(count) history row \(count)")
            app.navigationBars.buttons.element(boundBy: 0).tap()
        }
    }

    func testColdOpenWaitsForLateHistoryAndShowsLatestWithoutScrolling() {
        app.terminate()
        app.launchArguments.append("-MarbleUITestDelayedEveryone")
        app.launch()
        app.tabBars.buttons["Everyone"].tap()
        app.staticTexts["Earlier messages"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["Loading messages"].firstMatch.waitForExistence(timeout: 2))
        XCTAssertFalse(app.staticTexts["No one has spoken yet"].exists)
        // Allow an accessibility poll after the fixture's five-second delay.
        assertHittable("Everyone history row 59", timeout: 10)
    }

    func testEveryoneSavedMessageCanBeCopied() {
        app.tabBars.buttons["Everyone"].tap()
        app.staticTexts["Earlier messages"].tap()
        let text = "Everyone history row 59"
        assertHittable(text)

        app.staticTexts[text].firstMatch.press(forDuration: 1)
        XCTAssertTrue(app.buttons["Reply in thread"].firstMatch.isHittable)
        let copy = app.buttons["Copy"].firstMatch
        XCTAssertTrue(copy.isHittable)
        copy.tap()

        let composer = app.textFields.firstMatch
        composer.tap()
        composer.press(forDuration: 1)
        app.menuItems["Paste"].tap()
        XCTAssertEqual(composer.value as? String, text)
    }

    func testEveryoneSendKeepsHistoryAndThreeWorkingAgentsVisible() {
        app.terminate()
        app.launchArguments.append("-MarbleUITestEveryoneSend")
        app.launch()
        app.tabBars.buttons["Everyone"].tap()
        app.staticTexts["Earlier messages"].tap()
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

    private func openDefaultChats() {
        app.tabBars.buttons["Marbles"].tap()
        app.staticTexts["Marble"].tap()
    }

    private func assertHittable(
        _ text: String, timeout: TimeInterval = 5, file: StaticString = #filePath, line: UInt = #line
    ) {
        let message = app.staticTexts[text].firstMatch
        let visible = NSPredicate { _, _ in message.exists && message.isHittable }
        let expectation = XCTNSPredicateExpectation(predicate: visible, object: nil)
        let result = XCTWaiter.wait(for: [expectation], timeout: timeout)
        if result != .completed {
            shot("Latest message missing")
            print(app.debugDescription)
        }
        XCTAssertEqual(result, .completed, "Latest message did not appear without scrolling: \(text)", file: file, line: line)
    }

    func testLaunchLeavesTheTabsVisibleWithNoChatPushed() {
        XCTAssertTrue(app.tabBars.buttons["Everyone"].exists)
        XCTAssertTrue(app.tabBars.buttons["Marbles"].exists)
        openDefaultChats()
        XCTAssertTrue(app.staticTexts["Fixture conversation 0"].waitForExistence(timeout: 5))
        shot("01-chats-list")
    }

    func testEveryTabIsReachable() {
        for tab in ["Marbles", "Everyone", "Activity", "Settings"] {
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
        openDefaultChats()
        app.staticTexts["Fixture conversation 2"].tap()
        assertHittable("Fixture message 2")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.staticTexts["Fixture conversation 3"].waitForExistence(timeout: 5))
    }

    func testComposeOpensASavedChatThatCanBeReopened() {
        openDefaultChats()
        app.buttons["chat.compose"].tap()
        XCTAssertTrue(app.textFields["composer.field"].waitForExistence(timeout: 5))
        assertHittable("No messages yet")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.buttons["chat.compose"].isHittable)
        app.staticTexts["New Chat"].tap()
        XCTAssertTrue(app.textFields["composer.field"].waitForExistence(timeout: 5))
        assertHittable("No messages yet")
    }

    func testPickerOpensNewMarbleAndEveryoneIsOneTapAway() {
        XCTAssertTrue(app.tabBars.buttons["Marbles"].isSelected)
        let newMarble = app.buttons["New Marble"]
        XCTAssertTrue(newMarble.isHittable)
        newMarble.tap()
        XCTAssertTrue(app.textFields["Name your Marble"].waitForExistence(timeout: 5))
        app.buttons["Cancel"].tap()
        app.tabBars.buttons["Everyone"].tap()
        XCTAssertTrue(app.buttons["New Everyone thread"].isHittable)
        app.staticTexts["Earlier messages"].tap()
        assertHittable("Everyone history row 59")
    }

    func testConversationListSearchOpensMatchingChat() {
        openDefaultChats()
        searchConversations("conversation 150")
        let hit = app.staticTexts["Fixture conversation 150"]
        XCTAssertTrue(hit.waitForExistence(timeout: 5))
        hit.tap()
        let title = app.navigationBars.descendants(matching: .any).matching(
            NSPredicate(format: "label BEGINSWITH %@", "Fixture conversation 150,")
        ).firstMatch
        XCTAssertTrue(title.waitForExistence(timeout: 5))
    }

    func testConversationListSearchFindsSavedEmptyChatByDisplayedTitle() {
        openDefaultChats()
        app.buttons["chat.compose"].tap()
        assertHittable("No messages yet")
        app.navigationBars.buttons.element(boundBy: 0).tap()

        let list = app.collectionViews.firstMatch
        let emptyChat = list.staticTexts["New Chat"]
        let otherChat = list.staticTexts["Fixture conversation 0"]
        XCTAssertTrue(emptyChat.waitForExistence(timeout: 5))
        XCTAssertTrue(otherChat.exists)

        searchConversations("New Chat")
        XCTAssertTrue(otherChat.waitForNonExistence(timeout: 5))
        XCTAssertTrue(emptyChat.isHittable)
        emptyChat.tap()
        assertHittable("No messages yet")
    }

    private func searchConversations(_ query: String) {
        let search = app.searchFields.firstMatch
        if !search.exists {
            let button = app.buttons["Search"].firstMatch
            if button.exists {
                button.tap()
            } else {
                app.collectionViews.firstMatch.swipeDown()
            }
        }
        XCTAssertTrue(search.waitForExistence(timeout: 5))
        search.tap()
        search.typeText(query)
    }

    func testOnboardingScreenshot() {
        let fresh = XCUIApplication()
        fresh.launch()
        shot("15-onboarding")
    }

    func testBackFromChatStaysOnListWhileRefreshesFire() {
        openDefaultChats()
        app.staticTexts["Fixture conversation 0"].tap()
        XCTAssertTrue(app.navigationBars.buttons.element(boundBy: 0).waitForExistence(timeout: 5))
        shot("07-chat-open")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        XCTAssertTrue(app.staticTexts["Fixture conversation 1"].waitForExistence(timeout: 5))
        sleep(5)
        XCTAssertTrue(app.staticTexts["Fixture conversation 1"].exists)
    }

    func testConversationListScrollsToLastRow() {
        openDefaultChats()
        let last = app.staticTexts["Fixture conversation 199"]
        let list = app.collectionViews.firstMatch
        XCTAssertTrue(list.exists)
        let deadline = Date().addingTimeInterval(120)
        while !last.isHittable && Date() < deadline {
            list.swipeUp(velocity: .fast)
        }
        XCTAssertTrue(last.isHittable, "200-row list did not scroll to the last row")
    }

    func testChatScrollComposerReasoningAndShots() {
        openDefaultChats()
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
        XCTAssertTrue(app.keyboards.firstMatch.exists)
        shot("11-composer-draft-keyboard")
        // Scroll away: streaming must not yank, jump button returns.
        let transcript = app.scrollViews.containing(.staticText, identifier: "Fixture message 299").firstMatch
        let latest = app.staticTexts["Fixture message 299"]
        let jump = app.buttons["Jump to latest"]
        XCTAssertTrue(transcript.exists)
        let dismissalStart = transcript.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.4))
        let dismissalEnd = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.92))
        dismissalStart.press(forDuration: 0.05, thenDragTo: dismissalEnd)
        XCTAssertTrue(app.keyboards.firstMatch.waitForNonExistence(timeout: 5))
        shot("p1-keyboard-dismissed")
        let start = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.35))
        let end = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.66))
        for _ in 0..<3 where !jump.exists || latest.isHittable {
            start.press(forDuration: 0.05, thenDragTo: end)
        }
        XCTAssertTrue(jump.waitForExistence(timeout: 5))
        sleep(3)
        XCTAssertTrue(jump.isHittable)
        XCTAssertFalse(latest.isHittable)
        shot("p1-jump-visible")
        for _ in 0..<10 where !latest.isHittable {
            end.press(forDuration: 0.05, thenDragTo: start)
        }
        assertHittable("Fixture message 299")
        XCTAssertTrue(jump.waitForNonExistence(timeout: 5))
        shot("p1-jump-hidden-at-bottom")
        for _ in 0..<3 where !jump.exists {
            start.press(forDuration: 0.05, thenDragTo: end)
        }
        XCTAssertTrue(jump.waitForExistence(timeout: 5))
        jump.tap()
        assertHittable("Fixture message 299")
        XCTAssertFalse(jump.exists)
    }

    func testTranscriptDayGapAndExactTime() {
        openDefaultChats()
        app.staticTexts["Fixture conversation 2"].tap()
        assertHittable("Fixture message 2")
        let day = Calendar.current.startOfDay(for: Date())
        let lastMessageTime = day.addingTimeInterval(40_440)
        assertHittable(day.formatted(date: .abbreviated, time: .omitted))
        assertHittable(lastMessageTime.formatted(date: .omitted, time: .shortened))
        shot("p1-day-separator-and-time-gap")
        app.staticTexts["Fixture message 2"].press(forDuration: 1)
        let exactTime = app.buttons[lastMessageTime.formatted(date: .abbreviated, time: .standard)]
        XCTAssertTrue(exactTime.waitForExistence(timeout: 5))
        shot("p1-long-press-exact-time")
    }
}
