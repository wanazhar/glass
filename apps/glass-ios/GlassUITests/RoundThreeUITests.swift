import UIKit
import XCTest

final class RoundThreeUITests: XCTestCase {
    @MainActor
    func testDemoCanOpenAndRecoverConversationStates() {
        let app = launch("light")
        if UIDevice.current.userInterfaceIdiom == .phone { app.buttons["conversation.back"].tap() }
        let settings = app.buttons["tab.settings"]
        XCTAssertTrue(settings.waitForExistence(timeout: 8))
        app.descendants(matching: .any)["thread.harbor-1"].firstMatch.tap()
        XCTAssertTrue(app.buttons["Reconnect"].waitForExistence(timeout: 8))
        XCTAssertFalse(app.textViews["composer.field"].exists)
        if UIDevice.current.userInterfaceIdiom == .phone { app.buttons["conversation.back"].tap() }
        app.descendants(matching: .any)["thread.harbor-0"].firstMatch.tap()
        XCTAssertTrue(app.textViews["composer.field"].waitForExistence(timeout: 8))
        XCTAssertTrue(app.buttons["composer.model"].exists)
    }

    @MainActor
    func testReferencedMarkdownLoadsInThreadAndWaitsForImageConsent() {
        let app = launch("light")
        if UIDevice.current.userInterfaceIdiom == .phone { app.buttons["conversation.back"].tap() }
        let thread = app.descendants(matching: .any)["thread.harbor-2"].firstMatch
        XCTAssertTrue(thread.waitForExistence(timeout: 8))
        thread.tap()
        let chip = app.buttons["markdown.file"].firstMatch
        XCTAssertTrue(chip.waitForExistence(timeout: 8))
        chip.tap()
        XCTAssertTrue(app.staticTexts["Deployment notes"].waitForExistence(timeout: 8))
        let loadButtons = app.buttons.matching(identifier: "markdown.loadRemoteImage")
        XCTAssertTrue(loadButtons["Load image: Deployment diagram"].exists)
        XCTAssertTrue(loadButtons["Load image: Runbook diagram"].exists)
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
    func testLensSplashCanBeSkippedByTappingTheScene() {
        let app = launch("light", options: ["-GlassLaunchFrame", "ripple"])
        let skip = app.buttons["Skip"]
        XCTAssertTrue(skip.waitForExistence(timeout: 8))
        app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.46)).tap()
        XCTAssertTrue(skip.waitForNonExistence(timeout: 5))
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
            .containing(NSPredicate(format: "label CONTAINS %@", "1m 6s")).firstMatch
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
        continueAfterFailure = false
        let prompt = "Verify sync after a restart."
        for appearance in ["light", "dark"] {
            let app = launch(appearance)
            app.buttons["conversation.back"].tap()
            app.descendants(matching: .any)["thread.harbor-0"].firstMatch.tap()
            let field = app.textViews["composer.field"]
            XCTAssertTrue(field.waitForExistence(timeout: 8))
            field.tap()
            XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 5))
            capture(appearance, "keyboard")
            field.typeText(prompt)
            XCTAssertEqual(field.value as? String, prompt)
            capture(appearance, "composer-typing")
            app.buttons["composer.send"].tap()
            XCTAssertTrue(app.buttons["composer.stop"].waitForExistence(timeout: 5))
            capture(appearance, "composer-working")
            let stream = app.descendants(matching: .any)["timeline.streaming"].firstMatch
            XCTAssertTrue(stream.waitForExistence(timeout: 8))
            capture(appearance, "streaming")
            let sent = app.descendants(matching: .any).matching(identifier: "timeline.user")
                .matching(NSPredicate(format: "label CONTAINS %@", prompt))
            XCTAssertEqual(sent.allElementsBoundByIndex.map(\.label), [prompt])
            app.terminate()
        }
    }

    @MainActor
    func testJustSentMessageClearsDockedComposerAndKeyboardDismisses() throws {
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .phone, "Run on an iPhone simulator")
        continueAfterFailure = false
        let prompt = "Review the sync retry behavior."
        for appearance in ["light", "dark"] {
            let app = launch(appearance)
            app.buttons["conversation.new"].tap()
            let field = app.textViews["composer.field"]
            XCTAssertTrue(field.waitForExistence(timeout: 8))
            field.tap()
            let keyboard = app.keyboards.firstMatch
            XCTAssertTrue(keyboard.waitForExistence(timeout: 5))
            field.typeText(prompt)
            app.buttons["composer.send"].tap()
            let sent = app.descendants(matching: .any).matching(identifier: "timeline.user")
                .matching(NSPredicate(format: "label CONTAINS %@", prompt)).firstMatch
            let model = app.buttons["composer.model"]
            XCTAssertTrue(sent.waitForExistence(timeout: 5))
            XCTAssertTrue(app.buttons["composer.stop"].exists)
            XCTAssertTrue(keyboard.exists)
            XCTAssertTrue(sent.isHittable)
            XCTAssertGreaterThanOrEqual(model.frame.minY - sent.frame.maxY, 16)
            capture(appearance, "chat-keyboard")
            let start = app.coordinate(withNormalizedOffset: CGVector(dx: 0.08, dy: 0.23))
            let end = app.coordinate(withNormalizedOffset: CGVector(dx: 0.08, dy: 0.96))
            start.press(forDuration: 0.1, thenDragTo: end, withVelocity: .slow, thenHoldForDuration: 0)
            XCTAssertTrue(keyboard.waitForNonExistence(timeout: 5))
            app.terminate()
        }
    }

    @MainActor
    func testNativeMathAndPieScrubbing() throws {
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .phone, "Run on an iPhone simulator")
        continueAfterFailure = false
        for appearance in ["light", "dark"] {
            let app = launch(appearance)
            app.buttons["conversation.back"].tap()
            app.descendants(matching: .any)["thread.harbor-3"].firstMatch.tap()
            let equation = app.descendants(matching: .any)["timeline.rich.math"].firstMatch
            XCTAssertTrue(equation.waitForExistence(timeout: 8))
            let chart = app.descendants(matching: .any)["component.chart.plot"].firstMatch
            XCTAssertTrue(chart.waitForExistence(timeout: 8))
            chart.coordinate(withNormalizedOffset: CGVector(dx: 0.64, dy: 0.42))
                .press(forDuration: 0.1, thenDragTo: chart.coordinate(withNormalizedOffset: CGVector(dx: 0.55, dy: 0.15)))
            let selected = app.descendants(matching: .any)["component.chart.value"].firstMatch
            XCTAssertTrue(selected.waitForExistence(timeout: 5))
            XCTAssertTrue(["12.75", "7.25"].contains(selected.value as? String), selected.debugDescription)
            capture(appearance, "math-chart")
        }
    }

    @MainActor
    func testThousandThreadDrawerScrollPerformance() throws {
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .phone, "Run on an iPhone simulator")
        continueAfterFailure = false
        let app = launch("light", options: ["-GlassDrawerPerformance"])
        XCTAssertTrue(app.waitForThreadList(timeout: 10))
        let list = app.collectionViews.firstMatch
        measure(metrics: [XCTOSSignpostMetric.scrollingAndDecelerationMetric]) {
            list.swipeUp(velocity: .fast)
            list.swipeDown(velocity: .fast)
        }
        let search = app.searchFields.firstMatch
        search.tap()
        search.typeText("Optimize large workspace imports")
        let finalThread = app.descendants(matching: .any)["thread.harbor-999"].firstMatch
        XCTAssertTrue(finalThread.waitForExistence(timeout: 8))
    }

    @MainActor
    func testSecondaryScreensAtDynamicTypeXL() throws {
        try XCTSkipUnless(UIDevice.current.userInterfaceIdiom == .phone, "Run on an iPhone simulator")
        continueAfterFailure = false
        let options = ["-UIPreferredContentSizeCategoryName", "UICTContentSizeCategoryXL"]
        let pairing = XCUIApplication()
        pairing.launchArguments = ["-GlassResetState"] + options
        pairing.launch()
        XCTAssertTrue(pairing.staticTexts["pair.title"].waitForExistence(timeout: 8))
        XCTAssertTrue(pairing.buttons["pair.scan"].isHittable)
        capture("light", "pairing-xl")

        let app = launch("light", options: options)
        app.buttons["conversation.back"].tap()
        app.buttons["tab.desktop"].tap()
        XCTAssertTrue(app.navigationBars["Desktop"].waitForExistence(timeout: 8))
        XCTAssertTrue(app.buttons["Start desktop"].waitForExistence(timeout: 8))
        capture("light", "desktop-xl")
        app.buttons["Done"].tap()
        app.buttons["tab.settings"].tap()
        let settings = app.navigationBars["Settings"]
        XCTAssertTrue(settings.waitForExistence(timeout: 8))
        capture("light", "settings-xl")
        for (route, title) in [("host", "Host"), ("defaults", "Defaults"), ("diagnostics", "Diagnostics"),
                               ("appearance", "Appearance"), ("notifications", "Notifications"),
                               ("connection", "Background connection"), ("voice", "Voice"), ("about", "About")] {
            let link = app.descendants(matching: .any)["settings.\(route)"].firstMatch
            for _ in 0..<4 {
                if link.isHittable { break }
                app.collectionViews.firstMatch.swipeUp()
            }
            XCTAssertTrue(link.isHittable, title)
            link.tap()
            let screen = app.navigationBars[title]
            XCTAssertTrue(screen.waitForExistence(timeout: 8), title)
            capture("light", "\(route)-xl")
            screen.buttons.firstMatch.tap()
            XCTAssertTrue(settings.waitForExistence(timeout: 8))
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

        let heading = chat.staticTexts["Sync now resumes from the last committed page"].firstMatch
        XCTAssertTrue(revealInTranscript(heading, in: chat))
        capture(appearance, "long-markdown")
        let code = chat.staticTexts["swift"].firstMatch
        XCTAssertTrue(revealInTranscript(code, in: chat))
        capture(appearance, "code")
        let table = chat.descendants(matching: .any)["markdown.table"].firstMatch
        XCTAssertTrue(revealInTranscript(table, in: chat))
        capture(appearance, "table")
        let work = chat.buttons.matching(identifier: "timeline.work")
        let latest = work.containing(NSPredicate(format: "label CONTAINS %@", "2m 14s")).firstMatch
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
    private func revealInTranscript(_ target: XCUIElement, in app: XCUIApplication) -> Bool {
        let frame = app.frame
        for _ in 0..<7 {
            let targetFrame = target.exists ? target.frame : .zero
            if targetFrame.minY > frame.minY + 108,
               targetFrame.maxY < frame.maxY - 115 { return true }
            if !targetFrame.isEmpty, targetFrame.maxY > frame.maxY - 115 {
                app.coordinate(withNormalizedOffset: CGVector(dx: 0.06, dy: 0.68))
                    .press(forDuration: 0.08, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.06, dy: 0.45)))
            } else {
                app.coordinate(withNormalizedOffset: CGVector(dx: 0.06, dy: 0.28))
                    .press(forDuration: 0.08, thenDragTo: app.coordinate(withNormalizedOffset: CGVector(dx: 0.06, dy: 0.51)))
            }
        }
        return false
    }

    @MainActor
    private func capture(_ appearance: String, _ name: String) {
        let attachment = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        attachment.name = "\(appearance)-\(name)"
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
