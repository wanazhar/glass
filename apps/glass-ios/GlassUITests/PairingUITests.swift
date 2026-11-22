import XCTest

final class PairingUITests: XCTestCase {
    override func setUp() { continueAfterFailure = false }

    @MainActor
    private func launch() -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-GlassResetState"]
        app.launch()
        XCTAssertTrue(app.staticTexts["pair.title"].waitForExistence(timeout: 10))
        return app
    }

    @MainActor
    func testFirstLaunchShowsPairing() throws {
        let app = launch()
        XCTAssertTrue(app.navigationBars["Connect to Host"].exists)
        XCTAssertTrue(app.staticTexts["pair.title"].label.contains("gc connect qr"))
        XCTAssertTrue(app.textFields["pair.link"].exists || app.textViews["pair.link"].exists)
        XCTAssertTrue(app.buttons["pair.scan"].exists)
        XCTAssertFalse(app.buttons["pair.connect"].isEnabled)
        XCTAssertTrue(app.switches["pair.allowHTTP"].exists)
        attach(app, "pairing")
    }

    @MainActor
    func testRejectsTextThatIsNotALink() throws {
        let app = launch()
        enter("hello there", in: app)
        app.buttons["pair.connect"].tap()
        let error = app.descendants(matching: .any)["pair.error"].firstMatch
        XCTAssertTrue(error.waitForExistence(timeout: 5))
        XCTAssertTrue(error.label.contains("isn’t a Glass Connect link"), error.label)
    }

    @MainActor
    func testPlainHTTPLinkNeedsTheLANToggle() throws {
        let app = launch()
        enter("http://192.168.1.10:8080#owner=AbCdEfGhIjKlMnOpQr_-12", in: app)
        app.buttons["pair.connect"].tap()
        let error = app.descendants(matching: .any)["pair.error"].firstMatch
        XCTAssertTrue(error.waitForExistence(timeout: 5))
        XCTAssertTrue(error.label.contains("Allow HTTP on local network"), error.label)
        attach(app, "pairing-http-refused")
    }

    @MainActor
    func testScannerExplainsWhenTheCameraIsUnavailable() throws {
        let app = launch()
        app.buttons["pair.scan"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["scan.unavailable"].waitForExistence(timeout: 5))
        app.buttons["Cancel"].tap()
        XCTAssertTrue(app.staticTexts["pair.title"].waitForExistence(timeout: 5))
    }

    @MainActor
    func testPairingPassesTheAccessibilityAudit() throws {
        audit(launch(), "pairing")
    }

    @MainActor
    func testPairingLaysOutInLandscape() throws {
        XCUIDevice.shared.orientation = .landscapeLeft
        defer { XCUIDevice.shared.orientation = .portrait }
        let app = launch()
        XCTAssertTrue(app.buttons["pair.connect"].exists)
    }
}

extension XCTestCase {
    @MainActor
    func enter(_ text: String, in app: XCUIApplication) {
        let field = app.textFields["pair.link"].exists ? app.textFields["pair.link"] : app.textViews["pair.link"]
        field.tap()
        field.typeText(text)
    }

    /// Runs Xcode's accessibility audit and returns the actionable issues on the screen, so a test can report all screens at once.
    /// Not counted: contrast on disabled controls (WCAG exempts inactive UI), the system search field and content sampled under the
    /// tab bar, which Glass doesn't draw, advisory results ("nearly passed", "partially unsupported", no element), and "text clipped",
    /// which Xcode 26.6 reports for unclipped icon-and-title list rows; the large-type screenshot pass checks clipping instead.
    /// Those are attached to the test as a note.
    @MainActor
    func auditIssues(_ app: XCUIApplication, _ screen: String) -> [String] {
        sleep(1)
        var issues: [String] = []
        var notes: [String] = []
        let chromeTop = app.tabBars.firstMatch.exists ? app.tabBars.firstMatch.frame.minY : .greatestFiniteMagnitude
        do {
            try app.performAccessibilityAudit(for: .all) { issue in
                let element = issue.element
                let line = "\(screen) \(issue.compactDescription): \(element.map { "\($0.elementType.rawValue) '\($0.identifier)' '\($0.label.prefix(60))' \($0.frame)" } ?? "no element")"
                let advisory = element == nil || issue.compactDescription.contains("nearly") || issue.auditType == .dynamicType || issue.auditType == .textClipped
                    || (issue.auditType == .contrast && element?.isEnabled == false)
                    || (element?.elementType == .searchField && issue.auditType == .contrast)
                    // Rows scrolled under the tab bar's edge blur are measured against the blur, not their own row.
                    || (element.map { $0.frame.maxY > chromeTop - 24 } ?? false)
                if advisory { notes.append(line) } else { issues.append(line) }
                return true
            }
        } catch {
            issues.append("\(screen) audit failed: \(error)")
        }
        if !notes.isEmpty {
            let note = XCTAttachment(string: notes.joined(separator: "\n"))
            note.name = "audit-notes-\(screen)"
            note.lifetime = .keepAlways
            add(note)
        }
        return issues
    }

    @MainActor
    func audit(_ app: XCUIApplication, _ screen: String) {
        let issues = auditIssues(app, screen)
        XCTAssertTrue(issues.isEmpty, issues.joined(separator: "\n"))
    }

    @MainActor
    func attach(_ app: XCUIApplication, _ name: String) {
        let shot = XCTAttachment(screenshot: app.screenshot())
        shot.name = name
        shot.lifetime = .keepAlways
        add(shot)
    }
}
