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

    /// Pairing lands on Conversations; host status lives under Settings › Host.
    @MainActor
    private func waitForHost(_ app: XCUIApplication) -> Bool {
        guard app.tabBars.buttons["Settings"].waitForExistence(timeout: 20) else { return false }
        app.openSettings("host")
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
        XCTAssertTrue(app.descendants(matching: .any)["host.protocol"].label.contains("Up to date"))
        attach(app, "e2e-connected")

        app.openSettings("diagnostics")
        XCTAssertTrue(app.staticTexts["diag.core"].firstMatch.waitForExistence(timeout: 20), "host diagnostics report should load")
        let hostSection = app.staticTexts["Host machine"]
        for _ in 0..<6 where !hostSection.exists { app.swipeUp() }
        XCTAssertTrue(hostSection.exists)
        attach(app, "e2e-diagnostics")

        _ = try call("revoke", method: "POST")
        app.openSettings("host")
        app.buttons["host.refresh"].tap()
        XCTAssertTrue(app.descendants(matching: .any)["pair.repair"].waitForExistence(timeout: 20))
        attach(app, "e2e-revoked")

        pair(app, link: try call("link/secret"))
        XCTAssertTrue(waitForHost(app))
        XCTAssertTrue(text(app, containing: "Unlocked").exists, "secret link should unlock secret entry")

        app.openSettings("host")
        let forget = app.buttons["settings.forget"].firstMatch
        for _ in 0..<6 where !(forget.exists && forget.isHittable) { app.swipeUp() }
        forget.tap()
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
    private func pairedApp(link: String = "link/owner") throws -> XCUIApplication {
        let app = XCUIApplication()
        app.launchArguments = ["-GlassResetState"]
        app.launch()
        XCTAssertTrue(app.staticTexts["pair.title"].waitForExistence(timeout: 10))
        pair(app, link: try call(link))
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
    private func composerField(_ app: XCUIApplication) -> XCUIElement {
        let field = app.textViews["composer.field"].exists ? app.textViews["composer.field"] : app.textFields["composer.field"]
        return field.firstMatch
    }

    @MainActor
    private func waitUntil(_ timeout: TimeInterval = 30, _ condition: () -> Bool) -> Bool {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if condition() { return true }
            usleep(250_000)
        }
        return false
    }

    @MainActor
    private func typeAndSend(_ app: XCUIApplication, _ message: String) {
        let field = composerField(app)
        XCTAssertTrue(field.waitForExistence(timeout: 10))
        XCTAssertTrue(waitUntil(10) { field.isEnabled }, "the composer accepts input")
        field.tap()
        field.typeText(message)
        app.buttons["composer.send"].tap()
    }

    @MainActor
    func testComposerSendsStopsQueuesAndKeepsDrafts() throws {
        let thread = try call("thread/new?title=Composer", method: "POST")
        var app = try pairedApp()
        open(app, thread: thread)
        XCTAssertTrue(waitForConnection(app, "Live"))
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["composer.send"].isEnabled, "send is disabled while the draft is empty")

        typeAndSend(app, "hello from the phone")
        XCTAssertTrue(text(app, containing: "Echo from the host: hello from the phone").waitForExistence(timeout: 30), "native prompt runs a turn")
        attach(app, "e2e-composer-sent")

        // Hardware keyboard: Shift-Return breaks the line, Return sends.
        let field = composerField(app)
        field.tap()
        // The first hardware key event switches the simulator to its hardware keyboard and is not delivered.
        field.typeKey(XCUIKeyboardKey.shift, modifierFlags: [])
        field.typeText("line one")
        field.typeKey("\n", modifierFlags: .shift)
        field.typeText("line two")
        XCTAssertTrue((field.value as? String)?.contains("line one\nline two") == true, "Shift-Return inserts a line break")
        field.typeKey("\n", modifierFlags: [])
        XCTAssertTrue(text(app, containing: "Echo from the host: line one line two").waitForExistence(timeout: 30), "Return sends")

        typeAndSend(app, "slow 240")
        let stop = app.buttons["composer.stop"]
        XCTAssertTrue(stop.waitForExistence(timeout: 20), "stop appears while a turn runs")
        let mode = app.segmentedControls["composer.mode"]
        XCTAssertTrue(mode.exists, "steer or queue is offered while working")
        let queue = mode.buttons["Queue"]
        XCTAssertTrue(waitUntil(10) {
            if !queue.isSelected { queue.tap() }
            return queue.isSelected
        }, "queue mode is selected")
        let field2 = composerField(app)
        field2.tap()
        field2.typeText("queued after stop")
        app.buttons["composer.send"].tap()
        attach(app, "e2e-composer-queue")
        XCTAssertTrue(stop.waitForExistence(timeout: 10))
        stop.tap()
        XCTAssertTrue(text(app, containing: "Echo from the host: queued after stop").waitForExistence(timeout: 60), "queued prompt runs after the stopped turn")

        let draft = composerField(app)
        draft.tap()
        draft.typeText("unsent draft")
        // Drafts are written 300 ms after the last keystroke; leave room for a busy main thread.
        sleep(2)
        app.terminate()
        app = XCUIApplication()
        app.launch()
        XCTAssertTrue(app.waitForThreadList(timeout: 20))
        open(app, thread: thread)
        XCTAssertTrue(waitUntil(10) { (composerField(app).value as? String) == "unsent draft" }, "draft survives app termination")

        // Dictation asks for Speech Recognition; declining explains how to enable it.
        XCTAssertTrue(waitForConnection(app, "Live"))
        let clear = composerField(app)
        clear.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.5)).tap()
        let delete = app.keys["delete"]
        XCTAssertTrue(delete.waitForExistence(timeout: 5))
        for _ in 0..<"unsent draft".count { delete.tap() }
        XCTAssertTrue(app.buttons["composer.dictate"].waitForExistence(timeout: 5), "the draft clears")
        app.buttons["composer.dictate"].tap()
        let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
        let alert = springboard.alerts.firstMatch
        if alert.waitForExistence(timeout: 10) {
            attach(app, "e2e-dictation-permission")
            let deny = alert.buttons.matching(NSPredicate(format: "label BEGINSWITH 'Don'")).firstMatch
            (deny.exists ? deny : alert.buttons.element(boundBy: 0)).tap()
        }
        XCTAssertTrue(app.descendants(matching: .any)["composer.dictation.message"].waitForExistence(timeout: 10))
    }

    @MainActor
    func testControlHandoffMakesTheAppReadOnly() throws {
        let thread = try call("thread/new?title=Handoff", method: "POST")
        let app = try pairedApp()
        open(app, thread: thread)
        XCTAssertTrue(waitForConnection(app, "Live"))
        XCTAssertTrue(app.buttons["composer.send"].waitForExistence(timeout: 10))
        XCTAssertTrue(waitUntil(15) { !app.descendants(matching: .any)["composer.readOnly"].exists }, "the opening client holds control")

        app.buttons["conversation.menu"].tap()
        app.buttons["conversation.releaseControl"].tap()
        let readOnly = app.descendants(matching: .any)["composer.readOnly"]
        XCTAssertTrue(readOnly.waitForExistence(timeout: 15), "releasing control makes the composer read-only")
        XCTAssertTrue(try call("control/claim?id=\(thread)", method: "POST").contains("true"), "another client takes control")

        app.buttons["composer.takeControl"].tap()
        sleep(2)
        XCTAssertTrue(readOnly.exists, "control stays with the other client")
        XCTAssertFalse(composerField(app).isEnabled)
        attach(app, "e2e-read-only")

        _ = try call("control/release?id=\(thread)", method: "POST")
        app.buttons["composer.takeControl"].tap()
        XCTAssertTrue(waitUntil(15) { !readOnly.exists }, "taking control after release restores the composer")
        typeAndSend(app, "back in control")
        XCTAssertTrue(text(app, containing: "Echo from the host: back in control").waitForExistence(timeout: 30))
    }

    @MainActor
    func testOpeningAShortChatShowsNoControlPromptOrEarlierMessages() throws {
        let thread = try call("thread/new?title=Short%20chat", method: "POST")
        _ = try call("thread/seed?id=\(thread)&pairs=1&size=40", method: "POST")
        let app = try pairedApp()
        let readOnly = app.descendants(matching: .any)["composer.readOnly"]
        let older = app.buttons["conversation.older"]
        for attempt in 0..<3 {
            open(app, thread: thread)
            var flashed = false
            var earlier = false
            let deadline = Date().addingTimeInterval(4)
            while Date() < deadline {
                if readOnly.exists { flashed = true }
                if older.exists { earlier = true }
                usleep(100_000)
            }
            XCTAssertFalse(flashed, "opening (attempt \(attempt)) never shows Take Control while this iPhone gets control")
            XCTAssertFalse(earlier, "a two-message chat has no earlier messages")
            XCTAssertTrue(waitForConnection(app, "Live"))
            XCTAssertTrue(text(app, containing: "Seeded answer 1").exists)
            if attempt == 0 { attach(app, "e2e-short-chat") }
            app.navigationBars.buttons.element(boundBy: 0).tap()
            XCTAssertTrue(app.waitForThreadList(timeout: 10))
        }
    }

    @MainActor
    func testQuestionAndSecretInteractionsOverHTTPS() throws {
        let thread = try call("thread/new?title=Interactions", method: "POST")
        let app = try pairedApp(link: "link/secret")
        open(app, thread: thread)
        XCTAssertTrue(waitForConnection(app, "Live"))

        typeAndSend(app, "ask question")
        let interaction = app.descendants(matching: .any)["interaction"]
        XCTAssertTrue(interaction.waitForExistence(timeout: 30), "question interaction appears")
        app.buttons.matching(identifier: "interaction.option").matching(NSPredicate(format: "label CONTAINS 'Blue'")).firstMatch.tap()
        attach(app, "e2e-question")
        app.buttons["interaction.submit"].tap()
        XCTAssertTrue(text(app, containing: "Tool said").waitForExistence(timeout: 30))
        XCTAssertTrue(text(app, containing: "Blue").exists, "the chosen answer reaches the agent")
        XCTAssertTrue(waitUntil(10) { !interaction.exists })

        typeAndSend(app, "need secret")
        let field = app.secureTextFields["interaction.secret.field"]
        XCTAssertTrue(field.waitForExistence(timeout: 30), "secret prompt appears over HTTPS with secret entry unlocked")
        field.tap()
        field.typeText("hunter2-e2e")
        attach(app, "e2e-secret")
        app.buttons["interaction.submit"].tap()
        XCTAssertTrue(text(app, containing: "provided").waitForExistence(timeout: 30), "the agent receives a secret handle")
        XCTAssertFalse(text(app, containing: "hunter2-e2e").exists, "the secret value never appears in the transcript")
    }

    @MainActor
    func testAttachPhotoUploadsAndRunsATurn() throws {
        let thread = try call("thread/new?title=Attachments", method: "POST")
        let app = try pairedApp()
        open(app, thread: thread)
        XCTAssertTrue(waitForConnection(app, "Live"))
        XCTAssertTrue(app.buttons["composer.attach"].waitForExistence(timeout: 10))
        app.buttons["composer.attach"].tap()
        app.buttons["composer.attach.photos"].tap()
        let photo = app.images.matching(NSPredicate(format: "label CONTAINS[c] 'photo' OR identifier BEGINSWITH 'PXGGridLayout'")).firstMatch
        XCTAssertTrue(photo.waitForExistence(timeout: 20), "photo picker shows the library")
        photo.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        let add = app.buttons.matching(NSPredicate(format: "label == 'Add' OR label == 'Done'")).firstMatch
        XCTAssertTrue(waitUntil(10) { add.exists && add.isEnabled }, "picking a photo enables Add")
        add.tap()
        XCTAssertTrue(app.descendants(matching: .any)["composer.context"].firstMatch.waitForExistence(timeout: 30), "uploaded photo appears as a context chip")
        XCTAssertTrue(try call("thread/context?id=\(thread)").contains("Uploaded Photo 1"), "the host lists the attachment")
        attach(app, "e2e-attachment")
        typeAndSend(app, "what is in the photo")
        XCTAssertTrue(text(app, containing: "Echo from the host: what is in the photo").waitForExistence(timeout: 30))
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

    // MARK: Commands and settings (M4)

    @MainActor
    private func search(_ app: XCUIApplication, _ query: String) {
        let field = app.searchFields.firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 10))
        field.tap()
        if let clear = field.buttons.firstMatch as XCUIElement?, clear.exists { clear.tap() }
        field.typeText(query)
    }

    @MainActor
    private func runFromPalette(_ app: XCUIApplication, _ query: String, _ name: String) -> XCUIElement {
        search(app, query)
        let row = app.buttons["palette.\(name)"]
        XCTAssertTrue(row.waitForExistence(timeout: 15), "/\(name) is in the catalogue")
        row.tap()
        XCTAssertTrue(app.descendants(matching: .any)["command.\(name)"].firstMatch.waitForExistence(timeout: 10))
        return app.descendants(matching: .any)["command.result"].firstMatch
    }

    @MainActor
    private func back(_ app: XCUIApplication) {
        app.navigationBars.buttons["Commands"].firstMatch.tap()
    }

    @MainActor
    func testCommandPaletteRunsStatusUsageTodoAndName() throws {
        let thread = try call("thread/new?title=Commands", method: "POST")
        let app = try pairedApp()
        open(app, thread: thread)
        XCTAssertTrue(waitForConnection(app, "Live"))
        XCTAssertTrue(waitUntil(15) { !app.descendants(matching: .any)["composer.readOnly"].exists })

        app.buttons["conversation.commands"].tap()
        let palette = app.buttons["palette.done"].firstMatch
        XCTAssertTrue(palette.waitForExistence(timeout: 10), "the toolbar opens the command palette")
        XCTAssertTrue(app.buttons["palette.conversations"].firstMatch.waitForExistence(timeout: 15), "the catalogue loads")
        attach(app, "e2e-palette")
        app.buttons["palette.done"].tap()
        XCTAssertTrue(waitUntil(10) { !palette.exists }, "Done closes the palette")

        app.buttons["conversation.commands"].tap()
        XCTAssertTrue(runFromPalette(app, "usage", "usage").waitForExistence(timeout: 20), "read-only /usage returns a result")
        attach(app, "e2e-command-usage")
        back(app)

        XCTAssertTrue(runFromPalette(app, "/status", "status").waitForExistence(timeout: 20), "/status inspects on open")
        attach(app, "e2e-command-status")
        back(app)

        _ = runFromPalette(app, "todo", "todo")
        let args = app.textFields["command.field.args"].firstMatch
        XCTAssertTrue(args.waitForExistence(timeout: 10), "the todo form shows the add field")
        args.tap()
        args.typeText("Ship the M4 build")
        app.buttons["command.run"].tap()
        XCTAssertTrue(text(app, containing: "Ship the M4 build").waitForExistence(timeout: 30), "/todo add runs over the control channel")
        attach(app, "e2e-command-todo")
        back(app)

        _ = runFromPalette(app, "name", "name")
        let title = app.textFields["command.field.args"].firstMatch
        XCTAssertTrue(title.waitForExistence(timeout: 10))
        title.tap()
        title.typeText("Renamed from Glass")
        app.buttons["command.run"].tap()
        XCTAssertTrue(text(app, containing: "Renamed from Glass").waitForExistence(timeout: 30), "/name returns the new title")
        attach(app, "e2e-command-name")
    }

    @MainActor
    func testSettingsSaveKeepsEditsThroughAConflict() throws {
        let app = try pairedApp()
        app.openSettings("defaults")
        let general = app.buttons["settings.section.general"].firstMatch
        XCTAssertTrue(general.waitForExistence(timeout: 10))
        attach(app, "e2e-settings")
        general.tap()

        let imageInput = app.switches["settings.image_input"].firstMatch
        XCTAssertTrue(imageInput.waitForExistence(timeout: 20), "host settings load")
        let before = try call("settings/get?key=image_input", method: "POST")
        imageInput.switches.firstMatch.exists ? imageInput.switches.firstMatch.tap() : imageInput.tap()
        XCTAssertTrue(waitUntil(15) { (try? self.call("settings/get?key=image_input", method: "POST")) != before }, "the toggle saves on the host")

        let field = app.textFields["settings.field.max_tokens"].firstMatch
        XCTAssertTrue(field.waitForExistence(timeout: 10))
        XCTAssertTrue(waitUntil(15) { field.isEnabled }, "the toggle save finishes")
        field.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.8)).tap()
        let current = field.value as? String ?? ""
        field.typeText(String(repeating: XCUIKeyboardKey.delete.rawValue, count: current.count) + "4321")
        _ = try call("settings/set?key=cost_limit&value=7", method: "POST")
        app.buttons["settings.save.max_tokens"].tap()
        let feedback = app.descendants(matching: .any)["settings.feedback"].firstMatch
        XCTAssertTrue(feedback.waitForExistence(timeout: 15), "a stale revision reports a conflict")
        XCTAssertEqual(field.value as? String, "4321", "the unsaved edit survives the conflict")
        attach(app, "e2e-settings-conflict")

        app.buttons["settings.refresh"].tap()
        XCTAssertTrue(waitUntil(15) { !feedback.exists }, "refresh clears the conflict")
        XCTAssertEqual(field.value as? String, "4321", "the edit survives the refresh")
        app.buttons["settings.save.max_tokens"].tap()
        XCTAssertTrue(waitUntil(15) { (try? self.call("settings/get?key=max_tokens", method: "POST")) == "4321" }, "the retried save lands")

        app.openSettings("appearance")
        let theme = app.buttons["settings.theme"].firstMatch
        XCTAssertTrue(theme.waitForExistence(timeout: 10))
        theme.tap()
        app.buttons["Dark"].firstMatch.tap()
        XCTAssertTrue(waitUntil(5) { (theme.value as? String)?.contains("Dark") == true || theme.label.contains("Dark") })
        attach(app, "e2e-settings-dark")
    }

    @MainActor
    private func openDetails(_ app: XCUIApplication) {
        app.buttons["conversation.menu"].firstMatch.tap()
        app.buttons["conversation.details"].firstMatch.tap()
        XCTAssertTrue(app.buttons["details.done"].firstMatch.waitForExistence(timeout: 10), "Details opens")
    }

    @MainActor
    func testDetailsShowChangesFilesExportAndRichReplies() throws {
        let thread = try call("thread/new?title=Details", method: "POST")
        _ = try call("thread/rich?id=\(thread)", method: "POST")
        _ = try call("workspace/change", method: "POST")
        let app = try pairedApp()
        open(app, thread: thread)
        XCTAssertTrue(waitForConnection(app, "Live"))
        XCTAssertTrue(waitUntil(15) { !app.descendants(matching: .any)["composer.readOnly"].exists })

        let math = app.descendants(matching: .any)["timeline.rich.math"].firstMatch
        XCTAssertTrue(math.waitForExistence(timeout: 20), "the formula renders offline")
        XCTAssertTrue(app.descendants(matching: .any)["timeline.rich.mermaid"].firstMatch.waitForExistence(timeout: 20),
                      "the diagram renders offline")
        attach(app, "e2e-rich-reply")

        openDetails(app)
        attach(app, "e2e-details")
        app.buttons["details.changes"].firstMatch.tap()
        let file = app.buttons["changes.file"].firstMatch
        XCTAssertTrue(file.waitForExistence(timeout: 20), "Changes lists the edited file")
        file.tap()
        XCTAssertTrue(text(app, containing: "added from the harness").waitForExistence(timeout: 10), "the diff shows the new line")
        attach(app, "e2e-changes-diff")
        app.buttons["changes.stageFile"].firstMatch.tap()
        XCTAssertTrue(waitUntil(20) { (try? self.call("workspace/status", method: "POST"))?.hasPrefix("M ") == true },
                      "Stage runs git on the host")
        app.navigationBars.buttons.element(boundBy: 0).tap()

        app.buttons["details.files"].firstMatch.tap()
        let notes = app.buttons.containing(NSPredicate(format: "label CONTAINS 'notes.md'")).firstMatch
        XCTAssertTrue(notes.waitForExistence(timeout: 20), "Files lists the workspace")
        notes.tap()
        XCTAssertTrue(text(app, containing: "first line").waitForExistence(timeout: 15), "the file opens")
        attach(app, "e2e-file")
        app.navigationBars.buttons.element(boundBy: 0).tap()
        app.navigationBars.buttons.element(boundBy: 0).tap()

        app.buttons["details.export"].firstMatch.tap()
        XCTAssertTrue(app.descendants(matching: .any)["details.share"].firstMatch.waitForExistence(timeout: 30), "export downloads Markdown")
        app.buttons["details.done"].firstMatch.tap()

        let reply = app.descendants(matching: .any)["timeline.assistant"].firstMatch
        XCTAssertTrue(reply.waitForExistence(timeout: 10))
        reply.press(forDuration: 1.2)
        let quote = app.buttons["Quote"].firstMatch
        XCTAssertTrue(quote.waitForExistence(timeout: 10), "saved replies offer Quote")
        quote.tap()
        XCTAssertTrue(text(app, containing: "Quoted").waitForExistence(timeout: 20), "the host attaches the quote")
        attach(app, "e2e-quote")
    }

    @MainActor
    func testDesktopTabShowsTheHostsDesktopChoices() throws {
        let app = try pairedApp()
        app.tabBars.buttons["Desktop"].tap()
        let reusable = app.buttons["desktop.start.reusable"].firstMatch
        let quick = app.buttons["desktop.start.disposable"].firstMatch
        XCTAssertTrue(reusable.waitForExistence(timeout: 20), "the host lists its desktops")
        XCTAssertTrue(quick.exists)
        // The macOS test host has no Linux desktop, so each choice explains why it can't start.
        XCTAssertFalse(quick.isEnabled, "desktops the host can't run are disabled")
        attach(app, "e2e-desktop")
    }

    @MainActor
    func testEveryScreenPassesTheAccessibilityAudit() throws {
        let thread = try call("thread/new?title=Audit", method: "POST")
        _ = try call("thread/rich?id=\(thread)", method: "POST")
        let app = try pairedApp()
        var issues = auditIssues(app, "conversations")
        open(app, thread: thread)
        XCTAssertTrue(waitForConnection(app, "Live"))
        issues += auditIssues(app, "conversation")
        openDetails(app)
        sleep(2)
        attach(app, "audit-details")
        issues += auditIssues(app, "details")
        app.buttons["details.done"].firstMatch.tap()
        app.tabBars.buttons["Desktop"].tap()
        XCTAssertTrue(app.navigationBars.firstMatch.waitForExistence(timeout: 10))
        sleep(2)
        issues += auditIssues(app, "Desktop")
        app.openSettings(nil)
        sleep(1)
        issues += auditIssues(app, "Settings")
        for route in ["host", "diagnostics", "appearance", "notifications", "voice", "about"] {
            app.openSettings(route)
            sleep(2)
            issues += auditIssues(app, route)
        }
        XCTAssertTrue(issues.isEmpty, issues.joined(separator: "\n"))
    }
}
