import UIKit
import XCTest

final class RoundThreeUITests: XCTestCase {
    @MainActor
    func testDemoCanOpenAndRecoverConversationStates() {
        let app = launch("light")
        app.buttons["conversation.back"].tap()
        let settings = app.buttons["tab.settings"]
        XCTAssertTrue(settings.waitForExistence(timeout: 8))
        app.descendants(matching: .any)["thread.harbor-1"].firstMatch.tap()
        XCTAssertTrue(app.buttons["Reconnect"].waitForExistence(timeout: 8))
        XCTAssertFalse(app.textViews["composer.field"].exists)
        app.buttons["conversation.back"].tap()
        app.descendants(matching: .any)["thread.harbor-0"].firstMatch.tap()
        XCTAssertTrue(app.textViews["composer.field"].waitForExistence(timeout: 8))
        XCTAssertTrue(app.buttons["composer.model"].exists)
    }

    @MainActor
    func testLightTour() throws { try tour("light") }

    @MainActor
    func testDarkTour() throws { try tour("dark") }

    @MainActor
    func testSidebarSearchAndDisconnectedState() throws {
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .phone, "Run on an iPhone simulator")
        for appearance in ["light", "dark"] {
            let app = launch(appearance)
            app.buttons["conversation.back"].tap()
            let search = app.searchFields.firstMatch
            XCTAssertTrue(search.waitForExistence(timeout: 8))
            let staging = app.descendants(matching: .any)["thread.harbor-1"].firstMatch
            let unrelated = app.descendants(matching: .any)["thread.harbor-0"].firstMatch
            XCTAssertTrue(unrelated.waitForExistence(timeout: 5))
            search.tap()
            search.typeText("staging")
            XCTAssertTrue(unrelated.waitForNonExistence(timeout: 5))
            XCTAssertTrue(staging.isHittable)
            capture(appearance, "sidebar-search")
            staging.tap()
            XCTAssertTrue(app.buttons["Reconnect"].waitForExistence(timeout: 8))
            capture(appearance, "disconnected")
        }
    }

    @MainActor
    func testSplashAndOutputTours() throws {
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .phone, "Run on an iPhone simulator")
        for appearance in ["light", "dark"] {
            for frame in ["start", "ripple", "handoff"] {
                let app = launch(appearance, options: ["-GlassLaunchFrame", frame])
                XCTAssertTrue(app.buttons["Skip"].waitForExistence(timeout: 8))
                capture(appearance, "splash-" + frame)
            }
            outputTour(appearance)
        }
    }

    @MainActor
    func testTranscriptCanScrollAndFollowTheKeyboard() throws {
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .phone, "Run on an iPhone simulator")
        let app = launch("light")
        app.buttons["conversation.back"].tap()
        app.descendants(matching: .any)["thread.harbor-0"].firstMatch.tap()
        let field = app.textViews["composer.field"]
        XCTAssertTrue(field.waitForExistence(timeout: 8))
        let older = app.buttons.matching(identifier: "timeline.work")
            .containing(NSPredicate(format: "label CONTAINS %@", "1 min")).firstMatch
        XCTAssertTrue(reveal(older, in: app))
        let latest = app.buttons["conversation.latest"]
        XCTAssertTrue(latest.waitForExistence(timeout: 5))
        latest.tap()

        let restingFrame = field.frame
        let keyboard = app.keyboards.firstMatch
        let tail = app.buttons["Coordinator.swift"]
        for _ in 0..<2 {
            field.tap()
            XCTAssertTrue(keyboard.waitForExistence(timeout: 5))
            XCTAssertLessThan(field.frame.maxY, restingFrame.minY)
            XCTAssertTrue(tail.isHittable)
            XCTAssertLessThan(tail.frame.maxY, field.frame.minY)
            let start = app.coordinate(withNormalizedOffset: CGVector(dx: 0.08, dy: 0.23))
            let end = app.coordinate(withNormalizedOffset: CGVector(dx: 0.08, dy: 0.95))
            start.press(forDuration: 0.05, thenDragTo: end)
            XCTAssertTrue(keyboard.waitForNonExistence(timeout: 5))
            if latest.isHittable { latest.tap() }
        }
    }

    @MainActor
    func testStreamingAndComposerStates() throws {
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .phone, "Run on an iPhone simulator")
        for appearance in ["light", "dark"] {
            let app = launch(appearance)
            app.buttons["conversation.back"].tap()
            app.descendants(matching: .any)["thread.harbor-0"].firstMatch.tap()
            let field = app.textViews["composer.field"]
            XCTAssertTrue(field.waitForExistence(timeout: 8))
            field.tap()
            XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5))
            capture(appearance, "keyboard")
            field.typeText("Verify sync after a restart.")
            capture(appearance, "composer-typing")
            app.buttons["composer.send"].tap()
            XCTAssertTrue(app.buttons["composer.stop"].exists)
            capture(appearance, "composer-working")
            let stream = app.staticTexts.containing(NSPredicate(format: "label BEGINSWITH %@", "The sync loop now")).firstMatch
            XCTAssertTrue(stream.waitForExistence(timeout: 5))
            XCTAssertTrue(stream.isHittable)
            XCTAssertTrue(app.buttons["composer.stop"].exists)
            capture(appearance, "streaming")
        }
    }

    @MainActor
    func testTabletLayout() throws {
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .pad, "Run on an iPad simulator")
        for appearance in ["light", "dark"] {
            let app = launch(appearance)
            XCTAssertTrue(app.buttons["tab.settings"].waitForExistence(timeout: 8))
            app.descendants(matching: .any)["thread.harbor-0"].firstMatch.tap()
            XCTAssertTrue(app.textViews["composer.field"].waitForExistence(timeout: 8))
            capture(appearance, "ipad-sidebar")
        }
    }

    @MainActor
    private func tour(_ appearance: String) throws {
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .phone, "Run on an iPhone simulator")
        let app = launch(appearance)
        capture(appearance, "new-chat")
        app.buttons["conversation.back"].tap()
        let settings = app.buttons["tab.settings"]
        XCTAssertTrue(settings.waitForExistence(timeout: 8))
        capture(appearance, "sidebar-top")
        let finalThread = app.descendants(matching: .any)["thread.harbor-335"].firstMatch
        let list = app.collectionViews.firstMatch
        for _ in 0..<30 {
            if finalThread.isHittable { break }
            list.swipeUp(velocity: .fast)
        }
        XCTAssertTrue(finalThread.isHittable)
        XCTAssertTrue(settings.isHittable)
        capture(appearance, "sidebar-bottom")
        settings.tap()
        XCTAssertTrue(app.navigationBars["Settings"].waitForExistence(timeout: 8))
        capture(appearance, "settings")
    }

    @MainActor
    private func outputTour(_ appearance: String) {
        let chat = launch(appearance)
        chat.buttons["conversation.back"].tap()
        let repair = chat.descendants(matching: .any)["thread.harbor-0"].firstMatch
        XCTAssertTrue(repair.waitForExistence(timeout: 8))
        repair.tap()
        let field = chat.textViews["composer.field"]
        XCTAssertTrue(field.waitForExistence(timeout: 8))
        capture(appearance, "composer-idle")
        chat.buttons["composer.model"].tap()
        XCTAssertTrue(chat.buttons["Claude Opus 4.7"].waitForExistence(timeout: 8))
        capture(appearance, "model-picker")
        chat.buttons["Cancel"].tap()
        chat.buttons["conversation.connection"].tap()
        XCTAssertTrue(chat.staticTexts["Machine"].waitForExistence(timeout: 8))
        capture(appearance, "thread-info")
        chat.buttons["Done"].tap()

        capture(appearance, "long-markdown")
        let table = chat.descendants(matching: .any)["markdown.table"].firstMatch
        if reveal(table, in: chat) { capture(appearance, "table") }
        let code = chat.descendants(matching: .any)["timeline.code"].firstMatch
        if reveal(code, in: chat) { capture(appearance, "code") }
        let work = chat.buttons.matching(identifier: "timeline.work")
        let latest = work.containing(NSPredicate(format: "label CONTAINS %@", "2 min")).firstMatch
        if reveal(latest, in: chat) {
            capture(appearance, "work-collapsed")
            latest.tap()
            capture(appearance, "work-expanded")
            let diff = chat.buttons.matching(identifier: "timeline.diff").firstMatch
            if reveal(diff, in: chat) {
                diff.tap()
                capture(appearance, "diff")
                chat.buttons["Done"].tap()
            }
            let command = chat.buttons.matching(identifier: "timeline.tool").firstMatch
            if reveal(command, in: chat) {
                command.tap()
                capture(appearance, "command-output")
                chat.buttons["Done"].tap()
            }
        }
        let failed = chat.descendants(matching: .any)["timeline.failure"].firstMatch
        if reveal(failed, in: chat) { capture(appearance, "failed-run") }
        let large = launch(appearance, options: ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryXL"])
        large.buttons["conversation.back"].tap()
        large.descendants(matching: .any)["thread.harbor-0"].firstMatch.tap()
        XCTAssertTrue(large.textViews["composer.field"].waitForExistence(timeout: 8))
        capture(appearance, "dynamic-type-xl")
    }

    @MainActor
    private func launch(_ appearance: String, options: [String] = []) -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-GlassDemo", "-glass.theme", appearance] + options
        if !options.contains("-GlassLaunchFrame") { app.launchArguments.append("-GlassSkipSplash") }
        app.launch()
        return app
    }

    @MainActor
    private func reveal(_ target: XCUIElement, in app: XCUIApplication) -> Bool {
        let start = app.coordinate(withNormalizedOffset: CGVector(dx: 0.08, dy: 0.28))
        let end = app.coordinate(withNormalizedOffset: CGVector(dx: 0.08, dy: 0.68))
        for _ in 0..<5 {
            if target.isHittable { return true }
            start.press(forDuration: 0.04, thenDragTo: end)
        }
        return target.isHittable
    }

    @MainActor
    private func capture(_ appearance: String, _ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = "\(appearance)-\(name)"
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
