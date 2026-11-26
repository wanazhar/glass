import Testing
@testable import Glass

@Test func copiedCommandPreviewsAreLabeled() {
    var detail = ActivityDetail(id: "tool", title: "Command output", text: "limited", monospaced: true)
    #expect(detail.copyTitle(hasSavedOutput: true) == "Copy preview")
    #expect(detail.copyTitle(hasSavedOutput: false) == "Copy")
    detail.command = CommandActivity(command: "swift test",
                                     output: "Output\nPreview capped · full output remains on the host.",
                                     duration: nil, failed: false)
    #expect(detail.copyTitle(hasSavedOutput: false) == "Copy preview")
}

@Test func commandDurationUsesADashOnlyWhenUnavailable() {
    let unavailable = CommandActivity(command: "swift test", output: "", duration: nil, failed: false)
    let measured = CommandActivity(command: "swift test", output: "", duration: .milliseconds(480), failed: false)
    #expect(unavailable.durationLabel == "—")
    #expect(measured.durationLabel != "—")
}
