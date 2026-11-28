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

    private func checkCapsule(_ title: String) {
        let control = button(title)
        check(control)
        XCTAssertEqual(control.frame.height, 44, accuracy: 1)
    }

    private func start(address: String = "http://127.0.0.1:4783", expectedAgent: String = "Ada") throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["MARBLE_EVIDENCE"] == "1",
                          "Requires a seeded local Marble host")
        continueAfterFailure = false
        app.launch()
        let connect = button("Connect to host")
        let picker = app.staticTexts[expectedAgent].firstMatch
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
        button("New agent").tap()
        check(app.textFields["Name your agent"])
        shot("new-marble")
        button("Cancel").tap()
        app.staticTexts["Ada"].firstMatch.tap()
        check(app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "packing list")).firstMatch)
        shot("chat")
        button("About this agent").tap()
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
        app.staticTexts["Earlier messages"].tap()
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

    func testHostedKnowledge() throws {
        guard let address = ProcessInfo.processInfo.environment["MARBLE_KNOWLEDGE_EVIDENCE_URL"] else {
            throw XCTSkip("Requires the local knowledge fixture")
        }
        try start(address: address, expectedAgent: "Marble")
        shot("p3-agents")
        let ada = app.staticTexts["Ada"].firstMatch
        for _ in 0..<4 where !ada.isHittable { app.swipeUp() }
        XCTAssertTrue(ada.isHittable)
        shot("p3-peach-avatar")
        ada.tap()
        let composer = app.descendants(matching: .any)["composer.field"].firstMatch
        func send(_ prompt: String) {
            check(composer)
            composer.tap()
            composer.typeText(prompt)
            button("Send").tap()
            let tutorial = button("Continue")
            if tutorial.exists { tutorial.tap() }
        }
        let remembered = app.buttons.containing(NSPredicate(format: "label CONTAINS %@", "Remembered")).firstMatch
        let skillSaved = app.buttons.containing(NSPredicate(format: "label CONTAINS %@", "Skill saved")).firstMatch
        send("Record my lasting preference")
        check(remembered, timeout: 15)
        shot("p3-remembered")
        remembered.tap()
        check(app.navigationBars["Remembered"])
        shot("p3-memory-source")
        let memory = app.textFields["knowledge.memory.text"].firstMatch
        check(memory)
        memory.tap()
        memory.tap(withNumberOfTaps: 3, numberOfTouches: 1)
        memory.typeText(XCUIKeyboardKey.delete.rawValue)
        XCTAssertFalse(button("Save").isEnabled)
        shot("p3-blank-memory-save-disabled")
        memory.typeText("New wording")
        XCTAssertTrue(button("Save").isEnabled)
        button("Cancel").tap()
        send("Save our repeatable checklist")
        check(skillSaved, timeout: 15)
        shot("p3-skill-saved")
        send("Ada, teach Dietrich how you did that")
        check(app.staticTexts["I can help with that."].firstMatch, timeout: 15)
        send("I'm done with the cobalt project")
        check(app.staticTexts["Suggested cleanup"], timeout: 15)
        let keepSkill = button("Cobalt project checklist — Plan a release")
        check(keepSkill)
        XCTAssertTrue(keepSkill.isSelected)
        keepSkill.tap()
        XCTAssertFalse(keepSkill.isSelected)
        checkCapsule("Delete selected")
        checkCapsule("Keep all")
        shot("p3-pruning")
        button("Delete selected").tap()
        XCTAssertTrue(app.staticTexts["Suggested cleanup"].waitForNonExistence(timeout: 10))
        button("About this agent").tap()
        button("Memory").tap()
        let deleted = button("Recently Deleted")
        for _ in 0..<3 where !deleted.isHittable { app.swipeUp() }
        deleted.tap()
        check(button("Restore"))
        shot("p3-recently-deleted")
        button("Restore").tap()
        check(app.staticTexts["Nothing recently deleted."])
    }

    func testHostedLearnedSkillAndSharing() throws {
        guard let address = ProcessInfo.processInfo.environment["MARBLE_LEARNED_SKILL_EVIDENCE_URL"] else {
            throw XCTSkip("Requires a local host seeded with Dietrich's learned skill")
        }
        try start(address: address, expectedAgent: "Marble")
        let everyoneLink = app.buttons["Everyone"].firstMatch
        check(everyoneLink)
        everyoneLink.tap()
        check(app.navigationBars["Everyone"])
        app.tabBars.buttons["Agents"].tap()
        let dietrich = app.staticTexts["Dietrich"].firstMatch
        for _ in 0..<4 where !dietrich.isHittable { app.swipeUp() }
        XCTAssertTrue(dietrich.isHittable)
        dietrich.tap()
        button("About this agent").tap()
        button("Memory").tap()
        let placeholder = app.textFields["What should Dietrich remember?"]
        for _ in 0..<4 where !placeholder.isHittable { app.swipeUp() }
        XCTAssertTrue(placeholder.isHittable)
        shot("p4-dietrich-memory")
        for _ in 0..<4 where !app.staticTexts["Cobalt project checklist"].firstMatch.isHittable { app.swipeDown() }
        let attribution = app.staticTexts.containing(NSPredicate(format: "label BEGINSWITH %@", "Learned from Ada, ")).firstMatch
        check(attribution)
        shot("p3-learned-skill")
        app.staticTexts["Cobalt project checklist"].firstMatch.tap()
        let name = app.textFields["Name"]
        check(name)
        name.tap()
        name.typeText(" for launch")
        button("Share with agents").tap()
        let engineer = app.switches["Engineer"]
        check(engineer)
        engineer.coordinate(withNormalizedOffset: CGVector(dx: 0.95, dy: 0.5)).tap()
        XCTAssertEqual(engineer.value as? String, "1")
        shot("p3-share-skill")
        back()
        check(button("Share with 1 agent"))
        shot("p4-one-agent")
        button("Save").tap()
        check(app.staticTexts["Cobalt project checklist for launch"])
    }

    func testHostedPrivateMemoryConsent() throws {
        guard let address = ProcessInfo.processInfo.environment["MARBLE_MEMORY_CONSENT_EVIDENCE_URL"] else {
            throw XCTSkip("Requires a local group with a pending private memory request")
        }
        try start(address: address, expectedAgent: "Marble")
        app.staticTexts["Marble"].firstMatch.tap()
        let thread = app.staticTexts["Cobalt group"]
        check(thread)
        thread.tap()
        let choose = app.buttons.matching(NSPredicate(format: "label == %@ AND enabled == YES", "Choose memory")).firstMatch
        check(choose)
        checkCapsule("Choose memory")
        checkCapsule("Not now")
        XCTAssertFalse(app.staticTexts["I plan trips using cobalt maps"].exists)
        shot("p3-memory-consent")
        choose.tap()
        check(app.navigationBars["Allow in this chat"])
        let fact = app.buttons.matching(NSPredicate(format: "label CONTAINS %@", "I plan trips using cobalt maps")).firstMatch
        check(fact)
        shot("p3-choose-memory")
        fact.tap()
        let field = app.descendants(matching: .any)["composer.field"].firstMatch
        let readyToType = XCTNSPredicateExpectation(predicate: NSPredicate(format: "hittable == YES"), object: field)
        XCTAssertEqual(XCTWaiter.wait(for: [readyToType], timeout: 5), .completed)
        field.tap()
        field.typeText("Which facts are in my saved memories?")
        button("Send").tap()
        check(app.staticTexts["Memory available."], timeout: 15)
        shot("p3-memory-consent-granted")
    }

    func testHostedAgentCards() throws {
        try start(address: "http://127.0.0.1:4785", expectedAgent: "Engineer")
        let engineer = app.staticTexts["Engineer"].firstMatch
        check(engineer)
        shot("p2-agent-roster")
        engineer.tap()
        button("About this agent").tap()
        check(app.textFields["First name"])
        shot("p2-agent-card")
        let native = app.switches["Native tools (all as a group)"]
        for _ in 0..<4 where !native.isHittable { app.swipeUp() }
        XCTAssertTrue(native.isHittable)
        shot("p2-agent-allowed-tools")
        let duplicate = button("Duplicate agent")
        for _ in 0..<4 where !duplicate.isHittable { app.swipeUp() }
        duplicate.tap()
        check(app.textFields["Name your agent"])
        XCTAssertEqual(app.textFields["Name your agent"].value as? String, "Engineer copy")
        shot("p2-duplicate-agent")
        button("Cancel").tap()
        back()
        back()
        let create = button("New agent")
        for _ in 0..<4 where !create.isHittable { app.swipeUp() }
        create.tap()
        check(app.textFields["Name your agent"])
        shot("p2-new-agent")
        button("Cancel").tap()
    }

    func testHostedAgentImport() throws {
        try start(address: "http://127.0.0.1:4785", expectedAgent: "Engineer")
        let action = button("Import agent")
        for _ in 0..<5 where !action.isHittable { app.swipeUp() }
        action.tap()
        let editor = app.textViews.firstMatch
        check(editor)
        editor.tap()
        editor.typeText(#"{"name":"Sage","role":"Release librarian","persona":"Summarize release notes","glyph":"circle","hue":45,"workspace":""}"#)
        let load = button("Load card")
        if !load.isHittable { app.swipeUp() }
        load.tap()
        let name = app.textFields["Name your agent"]
        check(name)
        XCTAssertEqual(name.value as? String, "Sage")
        XCTAssertEqual(app.textFields["Role"].value as? String, "Release librarian")
        shot("p2-import-agent-preview")
        button("Create").tap()
        check(button("About this agent"), timeout: 35)
        back()
        let imported = app.staticTexts["Sage"].firstMatch
        for _ in 0..<5 where !imported.isHittable { app.swipeUp() }
        check(imported)
        shot("p2-import-agent-roster")
    }

    func testHostedGroupRound() throws {
        try start(address: "http://127.0.0.1:4785", expectedAgent: "Engineer")
        app.staticTexts["Marble"].firstMatch.tap()
        button("chat.compose").tap()
        button("Group chat").tap()
        check(app.staticTexts["Crew presets"])
        button("Launch").tap()
        shot("p2-group-crew-presets")
        let engineer = app.switches["Engineer"]
        if !engineer.isHittable { app.swipeUp() }
        check(engineer)
        XCTAssertEqual(engineer.value as? String, "1")
        shot("p2-group-member-picker")
        app.swipeDown()
        let title = app.textFields["Title"]
        title.tap()
        title.typeText("Launch checklist")
        let goal = app.descendants(matching: .any)["group.goal"].firstMatch
        for _ in 0..<5 where !goal.isHittable { app.swipeUp() }
        goal.tap()
        goal.typeText("Plan the launch")
        let doneWhen = app.descendants(matching: .any)["group.doneWhen"].firstMatch
        doneWhen.tap()
        doneWhen.typeText("Checklist is ready")
        button("Create").tap()
        let field = app.descendants(matching: .any)["composer.field"].firstMatch
        check(field)
        button("Thread info").tap()
        check(app.staticTexts["Lead · Required"])
        shot("p2-group-thread-info")
        let designer = app.switches["Designer"]
        check(designer)
        designer.coordinate(withNormalizedOffset: CGVector(dx: 0.91, dy: 0.5)).tap()
        XCTAssertEqual(designer.value as? String, "0")
        button("Done").tap()
        button("Thread info").tap()
        check(designer)
        XCTAssertEqual(designer.value as? String, "0")
        button("Cancel").tap()
        check(app.staticTexts["Done when, Checklist is ready"])
        shot("p2-group-task-header")
        field.tap()
        field.typeText("Verify the handoff")
        button("Send").tap()
        let tutorial = button("Continue")
        if tutorial.exists { tutorial.tap() }
        let handoff = app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "checklist from Lead")).firstMatch
        check(handoff, timeout: 15)
        check(app.staticTexts["I verified Engineer’s checklist."], timeout: 15)
        check(app.staticTexts["Lead suggests adding Researcher"])
        app.swipeDown()
        shot("p2-group-suggestion")
        button("Add").tap()
        XCTAssertTrue(app.staticTexts["Lead suggests adding Researcher"].waitForNonExistence(timeout: 12))
        button("Thread info").tap()
        let researcher = app.switches["Researcher"]
        check(researcher)
        XCTAssertEqual(researcher.value as? String, "1")
        button("Cancel").tap()
        XCTAssertFalse(app.staticTexts["[[PASS]]"].exists)
        shot("p2-group-ordered-round")
        field.tap()
        field.typeText("Close the task")
        button("Send").tap()
        check(app.staticTexts["Task completed"])
        check(app.staticTexts["The launch checklist is ready for review."])
        shot("p2-group-closing-summary")
        back()
        check(app.staticTexts["Launch checklist"])
        shot("p2-group-chats-list")
    }

    func testEveryoneThreads() throws {
        try start(address: "http://127.0.0.1:4785", expectedAgent: "Engineer")
        app.tabBars.buttons["Everyone"].tap()
        let newThread = button("New Everyone thread")
        check(newThread)
        shot("p0-everyone-threads")
        newThread.tap()
        let title = app.alerts.textFields["Thread title"]
        check(title)
        shot("p0-new-thread")
        title.tap()
        let name = "Friday plans \(Int(Date().timeIntervalSince1970))"
        title.typeText(name)
        button("Create").tap()
        let field = app.textFields["Reply in thread"]
        check(field)
        field.tap()
        field.typeText("Verify the handoff")
        button("Send").tap()
        let reviewer = app.staticTexts["I verified Engineer’s checklist."]
        check(reviewer, timeout: 15)
        XCTAssertFalse(app.staticTexts["Dietrich is working…"].exists)
        XCTAssertFalse(app.staticTexts.containing(NSPredicate(format: "label BEGINSWITH %@", "The user is asking")).firstMatch.exists)
        let engineer = app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "@reviewer, please verify.")).firstMatch
        let transcript = app.scrollViews.firstMatch
        for _ in 0..<4 where !engineer.isHittable || !reviewer.isHittable {
            transcript.swipeUp(velocity: .slow)
        }
        XCTAssertTrue(engineer.isHittable)
        XCTAssertTrue(reviewer.isHittable)
        shot("p4-everyone-handoff")
        button("Thread options").tap()
        button("Archive").tap()
        check(newThread)
        app.staticTexts["Archived"].tap()
        check(app.staticTexts[name])
        shot("p0-archived")
        button("Restore").tap()
        back()
        check(app.staticTexts[name])
        shot("p0-restored")
        app.staticTexts[name].tap()
        button("Thread options").tap()
        button("Delete").tap()
        button("Delete thread").tap()
        check(newThread)
        XCTAssertFalse(app.staticTexts[name].exists)
        shot("p0-deleted")
    }

    func testHostedPrivateChatLifecycle() throws {
        guard let address = ProcessInfo.processInfo.environment["MARBLE_PRIVATE_CHAT_EVIDENCE_URL"] else {
            throw XCTSkip("Requires a local private-chat fixture")
        }
        try start(address: address, expectedAgent: "Engineer")
        let prompt = "Private trip notes \(Int(Date().timeIntervalSince1970))"
        app.staticTexts["Engineer"].firstMatch.tap()
        let chats = app.navigationBars.buttons["Chats"]
        check(chats)
        chats.tap()
        button("New chat").tap()
        let composer = app.textFields["composer.field"]
        check(composer)
        composer.tap()
        composer.typeText(prompt)
        button("Send").tap()
        check(app.staticTexts[prompt])
        check(app.staticTexts["Memory not available."].firstMatch, timeout: 15)
        let more = app.navigationBars.buttons["Chat actions"]
        check(more)
        more.tap()
        button("Archive").tap()
        back()
        back()
        app.staticTexts["Marble"].firstMatch.tap()
        button("Archived").tap()
        check(button("Restore"))
        shot("p4-private-archived")
        let archived = app.staticTexts.containing(NSPredicate(format: "label CONTAINS[c] %@", prompt)).firstMatch
        check(archived)
        archived.tap()
        check(composer)
        more.tap()
        button("Delete").tap()
        app.buttons.matching(NSPredicate(format: "label == %@", "Delete")).firstMatch.tap()
        check(app.navigationBars["Archived"])
        back()
        button("Recently Deleted").tap()
        let deleted = app.staticTexts.containing(NSPredicate(format: "label CONTAINS[c] %@", prompt)).firstMatch
        check(deleted)
        shot("p4-chat-recently-deleted")
        app.buttons.matching(NSPredicate(format: "label BEGINSWITH %@ AND label CONTAINS %@", "Restore ", prompt))
            .firstMatch.tap()
        XCTAssertTrue(deleted.waitForNonExistence(timeout: 15))
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
        check(app.navigationBars["Pair your host"])
        check(app.staticTexts["Host"])
        check(app.staticTexts["Access token"])
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
        check(app.descendants(matching: .any)["pairing.tokenError"].firstMatch)
        XCTAssertFalse(app.descendants(matching: .any)["pairing.hostError"].firstMatch.exists)
        shot("error")
    }

    func testHostedPickerBottom() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["MARBLE_PICKER_INSET_EVIDENCE"] == "1",
                          "Requires a seeded roster that fills the picker")
        try start()
        let newMarble = button("New agent")
        let tab = button("Agents")
        check(tab)
        for _ in 0..<5 {
            app.swipeUp()
            if newMarble.isHittable, newMarble.frame.maxY < tab.frame.minY { break }
        }
        XCTAssertTrue(newMarble.isHittable)
        XCTAssertLessThan(newMarble.frame.maxY, tab.frame.minY)
        shot("picker-bottom")
    }

    func testHostedTablet() throws {
        try start()
        check(app.staticTexts["Choose an agent to start a conversation."])
        shot("ipad-home")
        app.staticTexts["Ada"].firstMatch.tap()
        check(app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "packing list")).firstMatch)
        XCTAssertTrue(app.staticTexts["Cleo"].firstMatch.isHittable)
        shot("ipad-chat")
    }

    func testHostedAgentCopy() throws {
        try start(expectedAgent: "Marble")
        try XCTSkipUnless(app.frame.width >= 600, "Requires the split-view layout")
        check(app.staticTexts["Start a chat"])
        check(app.staticTexts["Choose an agent to start a conversation."])
        shot("p4-agent-empty")
        let newAgent = button("New agent")
        for _ in 0..<4 where !newAgent.isHittable {
            app.collectionViews.firstMatch.swipeUp()
        }
        check(newAgent)
        shot("p4-new-agent-button")
        newAgent.tap()
        check(app.textFields["Name your agent"])
        shot("p4-new-agent")
    }

    func testHostedXL() throws {
        try start()
        app.swipeUp()
        let create = button("New agent")
        check(create)
        XCTAssertTrue(create.isHittable)
        create.tap()
        check(app.textFields["Name your agent"])
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

    func testHostedTurnPresentation() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["MARBLE_TURN_EVIDENCE"] == "1",
                          "Requires the local turn presentation fixture")
        let address = try XCTUnwrap(ProcessInfo.processInfo.environment["MARBLE_TURN_EVIDENCE_URL"])
        try start(address: address, expectedAgent: "Marble")
        shot("p4-agents-tab")
        app.staticTexts["Marble"].firstMatch.tap()
        check(button("chat.compose"))
        shot("p4-new-chat-button")
        button("chat.compose").tap()
        check(app.staticTexts["New chat"])
        shot("p4-new-chat-picker")
        button("Private chat").tap()
        let field = app.textFields["composer.field"]
        check(field)
        shot("p4-new-chat-empty")
        field.tap()
        field.typeText("Please check the report.")
        button("Send").tap()
        check(app.staticTexts["Checking the report…"])
        shot("p1-working-line")
        let collapsed = button("Used 2 tools")
        check(collapsed, timeout: 15)
        collapsed.tap()
        let listing = app.buttons.containing(NSPredicate(format: "label CONTAINS %@", "Couldn’t run ls")).firstMatch
        let search = app.buttons.containing(NSPredicate(format: "label CONTAINS %@", "Searched the web")).firstMatch
        check(listing)
        check(search)
        check(app.staticTexts["Running"].firstMatch)
        shot("p4-tools-running")
        let reply = app.staticTexts["The report is ready to review."]
        check(reply, timeout: 20)
        XCTAssertFalse(app.tabBars.firstMatch.exists)
        check(listing)
        check(search)
        shot("p4-tools-settled")
        collapsed.tap()
        shot("p1-tools-collapsed")
        collapsed.tap()
        let failed = listing
        check(failed)
        XCTAssertTrue(app.buttons.containing(NSPredicate(format: "label CONTAINS %@", "Done")).firstMatch.exists)
        shot("p1-tools-expanded")
        failed.tap()
        check(app.staticTexts["ls -l appendix.pdf"])
        check(app.staticTexts["No such file: appendix.pdf"])
        shot("p1-tool-detail")
        button("Done").tap()
        field.tap()
        field.typeText("Check the report failure.")
        button("Send").tap()
        XCTAssertFalse(app.staticTexts["Marble couldn’t finish this reply."].exists)
        check(app.staticTexts["Marble couldn’t finish this reply."], timeout: 20)
        XCTAssertEqual(app.staticTexts.matching(identifier: "Marble couldn’t finish this reply.").count, 1)
        checkCapsule("Retry")
        let continueButton = button("Continue")
        if continueButton.exists { continueButton.tap() }
        shot("p1-named-failure")
    }

    func testHostedInlineApproval() throws {
        guard let address = ProcessInfo.processInfo.environment["MARBLE_TURN_EVIDENCE_URL"] else {
            throw XCTSkip("Requires the local turn presentation fixture")
        }
        try start(address: address)
        app.staticTexts["Marble"].firstMatch.tap()
        button("chat.compose").tap()
        button("Private chat").tap()
        let field = app.textFields["composer.field"]
        check(field)
        field.tap()
        field.typeText("Approve the report draft")
        button("Send").tap()
        check(app.staticTexts["Write to workspace"])
        let approve = button("Allow")
        let decline = button("Deny")
        let transcript = app.scrollViews.firstMatch
        for _ in 0..<3 where !approve.isHittable { transcript.swipeUp(velocity: .slow) }
        XCTAssertTrue(approve.isHittable)
        XCTAssertGreaterThanOrEqual(decline.frame.minX, app.frame.minX + 16)
        XCTAssertLessThanOrEqual(approve.frame.maxX, app.frame.maxX - 16)
        shot("p4-inline-approval")
        decline.tap()
    }

    func testHostedP5CodeAndComposer() throws {
        guard let address = ProcessInfo.processInfo.environment["MARBLE_TURN_EVIDENCE_URL"] else {
            throw XCTSkip("Requires the local turn presentation fixture")
        }
        try start(address: address, expectedAgent: "Marble")
        app.staticTexts["Marble"].firstMatch.tap()
        button("chat.compose").tap()
        button("Private chat").tap()
        let field = app.textFields["composer.field"]
        check(field)
        field.tap()
        field.typeText("Show a wide code sample.")
        button("Send").tap()
        let code = app.scrollViews["chat.code"].firstMatch
        let copy = button("Copy code")
        check(code)
        check(copy)
        XCTAssertEqual(copy.frame.height, 44, accuracy: 1)
        XCTAssertTrue(copy.isHittable)
        shot("p5-code-start")
        code.swipeLeft(velocity: .slow)
        XCTAssertTrue(copy.isHittable)
        shot("p5-code-scrolled")
        copy.tap()
        check(button("Copied"))
        field.tap()
        check(app.keyboards.firstMatch)
        let latest = app.staticTexts["End of code example."].firstMatch
        check(latest)
        XCTAssertTrue(latest.isHittable)
        XCTAssertLessThanOrEqual(latest.frame.maxY, field.frame.minY)
        shot("p5-code-keyboard")
    }

    func testHostedP5PauseAcrossTabs() throws {
        guard let address = ProcessInfo.processInfo.environment["MARBLE_TURN_EVIDENCE_URL"] else {
            throw XCTSkip("Requires the local turn presentation fixture")
        }
        try start(address: address, expectedAgent: "Marble")
        app.tabBars.buttons["Settings"].tap()
        let stop = button("settings.stopEverything")
        for _ in 0..<5 where !stop.isHittable { app.swipeUp() }
        XCTAssertTrue(stop.isHittable)
        stop.tap()
        check(app.staticTexts["This cancels every running turn and pauses the host. New messages wait until you resume."])
        shot("p5-stop-confirmation")
        app.buttons["Stop everything"].lastMatch.tap()
        let resume = button("host.resume")
        check(resume)
        shot("p5-settings-paused")
        app.tabBars.buttons["Everyone"].tap()
        check(resume)
        XCTAssertFalse(app.alerts["Couldn’t update threads"].exists)
        shot("p5-everyone-paused")
        resume.tap()
        XCTAssertTrue(resume.waitForNonExistence(timeout: 9))
    }

    func testHostedP5BuildMismatch() throws {
        guard let address = ProcessInfo.processInfo.environment["MARBLE_MISMATCH_EVIDENCE_URL"] else {
            throw XCTSkip("Requires a local host built at a different git commit")
        }
        try start(address: address, expectedAgent: "Marble")
        check(app.descendants(matching: .any)["host.versionMismatch"].firstMatch)
        shot("p5-host-version-mismatch")
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
        check(app.textFields.containing(NSPredicate(format: "placeholderValue == %@", "Correct Ada’s reply")).firstMatch)
        if button("Hide keyboard").exists { button("Hide keyboard").tap() }
        shot("named-streaming-stop")
        stop.tap()
        XCTAssertTrue(stop.waitForNonExistence(timeout: 15))
        shot("named-stopped")
    }

    func testHostedNamedSteer() throws {
        try XCTSkipUnless(ProcessInfo.processInfo.environment["MARBLE_STEER_EVIDENCE"] == "1",
                          "Requires the local streaming and steering fixture")
        try start()
        let marble = app.buttons.containing(.staticText, identifier: "Ada").firstMatch
        check(marble)
        marble.tap()
        let composer = app.descendants(matching: .any)["composer.field"].firstMatch
        check(composer)
        composer.tap()
        composer.typeText("Plan a walk for this afternoon.")
        button("Send").tap()
        let stop = button("Stop")
        let partial = app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@", "trail by the river")).firstMatch
        check(stop)
        check(partial)
        shot("named-reply-streaming")
        composer.typeText("Choose a shaded trail instead.")
        let steer = button("Steer")
        check(steer)
        XCTAssertTrue(steer.isEnabled)
        XCTAssertTrue(stop.exists)
        shot("named-steer-and-stop")
        steer.tap()
        check(app.staticTexts.containing(NSPredicate(format: "label CONTAINS %@",
                                                    "With your correction in mind: Choose a shaded trail instead.")).firstMatch)
        XCTAssertTrue(stop.waitForNonExistence(timeout: 15))
        shot("named-reply-corrected")
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
