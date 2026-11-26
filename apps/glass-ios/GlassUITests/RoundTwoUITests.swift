import XCTest

final class RoundTwoUITests: XCTestCase {
    @MainActor
    func testSeededDrawerAndPagedWorkTimeline() throws {
        let fixture = ProcessInfo.processInfo.environment["GLASS_ROUND_TWO_FIXTURE"]
        try XCTSkipIf(fixture?.isEmpty != false, "Run with the seeded local Connect gateway")
        let link = try XCTUnwrap(fixture)
        continueAfterFailure = false
        let app = XCUIApplication()
        app.launchArguments = ["-GlassResetState"]
        app.launch()

        let field = app.descendants(matching: .any)["pair.link"].firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 10))
        field.tap()
        field.typeText(link)
        app.buttons["pair.connect"].tap()

        let drawer = app.buttons["conversation.back"]
        let paired = drawer.waitForExistence(timeout: 10)
        let pairingError = app.staticTexts["pair.error"]
        XCTAssertTrue(paired, pairingError.exists ? pairingError.label : "Conversations button missing")
        drawer.tap()
        let search = app.searchFields.firstMatch
        XCTAssertTrue(search.waitForExistence(timeout: 10))
        search.tap()
        search.typeText("Round Two History")
        let thread = app.buttons.containing(NSPredicate(format: "label CONTAINS %@", "Round Two History")).firstMatch
        XCTAssertTrue(thread.waitForExistence(timeout: 10))
        thread.tap()

        let work = app.buttons.matching(identifier: "timeline.work")
        XCTAssertTrue(work.firstMatch.waitForExistence(timeout: 10))
        let center = app.frame.midY
        let visibleRows = work.allElementsBoundByIndex.filter {
            $0.isHittable && abs($0.frame.midY - center) < app.frame.height / 3
        }
        let collapsed = try XCTUnwrap(visibleRows.min { abs($0.frame.midY - center) < abs($1.frame.midY - center) })
        XCTAssertTrue(collapsed.label.contains("command"))
        XCTAssertFalse(app.buttons["timeline.tool"].exists)
        collapsed.tap()
        let command = app.buttons.matching(identifier: "timeline.tool").firstMatch
        XCTAssertTrue(command.waitForExistence(timeout: 5))
        for _ in 0..<3 where !command.isHittable { app.swipeUp(velocity: .slow) }
        XCTAssertTrue(command.isHittable)
        command.tap()
        XCTAssertTrue(app.staticTexts["saved.text"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["Copy"].isHittable)
        app.buttons["Done"].tap()

        let earlier = app.buttons["conversation.older"]
        let headerBottom = drawer.frame.maxY
        for _ in 0..<14 where !earlier.isHittable || earlier.frame.midY < headerBottom + 8 {
            app.swipeDown(velocity: .fast)
        }
        XCTAssertTrue(earlier.isHittable)
        XCTAssertGreaterThan(earlier.frame.midY, headerBottom)
        let anchor = try XCTUnwrap(app.descendants(matching: .any)
            .matching(identifier: "timeline.user").allElementsBoundByIndex
            .first(where: { $0.isHittable && $0.frame.minY >= earlier.frame.maxY }))
        let label = anchor.label
        let originalY = anchor.frame.minY
        let conversation = app.descendants(matching: .any)["conversation"].firstMatch
        let previous = conversation.value as? String ?? ""
        earlier.tap()
        let changed = XCTNSPredicateExpectation(predicate: NSPredicate(format: "value != %@", previous), object: conversation)
        XCTAssertEqual(XCTWaiter.wait(for: [changed], timeout: 5), .completed)
        let preserved = app.descendants(matching: .any)
            .matching(identifier: "timeline.user").matching(NSPredicate(format: "label == %@", label)).firstMatch
        XCTAssertTrue(preserved.isHittable)
        XCTAssertEqual(preserved.frame.minY, originalY, accuracy: 24)
    }
}
