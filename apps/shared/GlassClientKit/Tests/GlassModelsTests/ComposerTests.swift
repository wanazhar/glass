import Foundation
import GlassModels
import Testing

@Suite struct ComposerTests {
    func encoded(_ message: SendMessage) throws -> [String: JSONValue] {
        let data = try JSONEncoder().encode(message.json)
        return try JSONDecoder().decode([String: JSONValue].self, from: data)
    }

    @Test func everySendMessageEncodesLikeTheWebUI() throws {
        #expect(try encoded(.prompt(text: "hi", requestID: "r1")) == ["t": "prompt", "text": "hi", "request_id": "r1"])
        #expect(try encoded(.steer(text: "go left", requestID: "r2")) == ["t": "steer", "text": "go left", "request_id": "r2"])
        #expect(try encoded(.cancel) == ["t": "cancel"])
        #expect(try encoded(.cancelCommand(requestID: "c1")) == ["t": "cancel_command", "request_id": "c1"])
        #expect(try encoded(.claimControl) == ["t": "control", "action": "claim"])
        #expect(try encoded(.releaseControl) == ["t": "control", "action": "release"])
        #expect(try encoded(.answer(interactionID: "i1", answer: Interaction.approval(allow: true)))
            == ["t": "answer", "id": "i1", "answer": .object(["allow": true])])
        #expect(try encoded(.command(name: "attach", values: .object(["upload": "tok"]), requestID: "c2"))
            == ["t": "command", "name": "attach", "values": .object(["upload": "tok"]), "request_id": "c2"])
    }

    @Test func requestIDsAreUniqueHex() {
        let a = SendMessage.newRequestID(), b = SendMessage.newRequestID()
        #expect(a != b)
        #expect(a.count == 32 && a.allSatisfy(\.isHexDigit))
    }

    @Test func questionAnswersAndCancelEncodeAsTheGatewayExpects() throws {
        let answer = Interaction.answers([["Yes"], ["A", "B"]])
        #expect(answer == .object(["status": "answered", "answers": .array([
            .object(["answers": .array(["Yes"])]), .object(["answers": .array(["A", "B"])]),
        ])]))
        #expect(Interaction.cancelled == .object(["status": "cancelled"]))
    }

    func value(_ json: String) throws -> JSONValue { try JSONDecoder().decode(JSONValue.self, from: Data(json.utf8)) }

    @Test func parsesApprovalQuestionAndSecretInteractions() throws {
        let approval = try #require(Interaction(value(#"{"id":"a","request":{"kind":"approval","reason":"Writes a file","operation":{"path":"x"}}}"#)))
        #expect(approval.kind == .approval(reason: "Writes a file", operation: .object(["path": "x"])))
        let question = try #require(Interaction(value(#"{"id":"q","request":{"kind":"question","request":{"questions":[{"question":"Which?","multiSelect":true,"options":[{"label":"A","description":"first"},{"label":"B"}]}]}}}"#)))
        #expect(question.kind == .questions([.init(text: "Which?", multiSelect: true, options: [.init(label: "A", detail: "first"), .init(label: "B", detail: nil)])]))
        let secret = try #require(Interaction(value(#"{"id":"s","request":{"kind":"secret","request":{"purpose":"Deploy","destination":"fly.io","secureSubmission":true}}}"#)))
        #expect(secret.kind == .secret(purpose: "Deploy", destination: "fly.io", secureSubmission: true))
        #expect(try Interaction(value(#"{"id":"x"}"#)) == nil)
    }

    @Test func timelineTracksThePendingInteraction() throws {
        var timeline = Timeline()
        let event = try JSONDecoder().decode(GlassEvent.self, from: Data(#"{"t":"interaction","id":"a","request":{"kind":"approval","reason":"r"}}"#.utf8))
        timeline.apply(event)
        #expect(timeline.interaction?.id == "a")
        timeline.apply(.interactionDone)
        #expect(timeline.interaction == nil)
        timeline.apply(event)
        timeline.apply(.error(message: "Stopped", settled: true, provider: false, requestID: nil))
        #expect(timeline.interaction == nil)
    }

    @Test func commandResultsDecodeWithRequestID() throws {
        let event = try JSONDecoder().decode(GlassEvent.self, from: Data(#"{"t":"command_result","request_id":"c1","result":{"body":{"tasks":[]}}}"#.utf8))
        #expect(event == .commandResult(requestID: "c1", result: .object(["body": .object(["tasks": .array([])])]), error: nil))
        let rejected = try JSONDecoder().decode(GlassEvent.self, from: Data(#"{"t":"error","request_id":"p1","message":"Read-only observer: claim control before sending input"}"#.utf8))
        #expect(rejected == .error(message: "Read-only observer: claim control before sending input", settled: false, provider: false, requestID: "p1"))
    }

    @Test func draftsAreBoundedByCountAgeAndLength() {
        var store = DraftStore()
        let start = Date(timeIntervalSince1970: 1_000_000)
        for i in 0..<30 { store.write("t\(i)", "draft \(i)", now: start.addingTimeInterval(Double(i))) }
        #expect(store.drafts.count == 24)
        #expect(store.read("t0") == "")
        #expect(store.read("t29") == "draft 29")
        store.write("long", String(repeating: "x", count: 70_000), now: start.addingTimeInterval(40))
        #expect(store.read("long").count == 60_000)
        store.prune(now: start.addingTimeInterval(31 * 86_400))
        #expect(store.drafts.isEmpty)
        store.write("t", "x", now: start)
        store.write("t", "", now: start)
        #expect(store.drafts.isEmpty)
    }

    @Test func draftsSurviveASaveAndLoad() throws {
        let url = FileManager.default.temporaryDirectory.appending(path: "drafts-\(UUID()).json")
        defer { try? FileManager.default.removeItem(at: url) }
        var store = DraftStore()
        store.write("a", "keep me")
        try store.save(to: url)
        #expect(DraftStore.load(from: url).read("a") == "keep me")
        try Data("not json".utf8).write(to: url)
        #expect(DraftStore.load(from: url).drafts.isEmpty)
    }

    @Test func uploadPolicyMatchesTheHostLimits() throws {
        let png = Data([0x89, 0x50, 0x4E, 0x47]) + Data(count: 100)
        #expect(try UploadPolicy.check(png, name: "a.png") == .image)
        #expect(throws: UploadPolicy.Rejection.imageTooLarge("big.png")) {
            try UploadPolicy.check(Data([0x89, 0x50, 0x4E, 0x47]) + Data(count: 8 * 1024 * 1024), name: "big.png")
        }
        #expect(try UploadPolicy.check(Data("notes".utf8), name: "a.txt") == .text)
        #expect(throws: UploadPolicy.Rejection.notText("big.txt")) {
            try UploadPolicy.check(Data(repeating: 0x61, count: 32 * 1024 + 1), name: "big.txt")
        }
        #expect(throws: UploadPolicy.Rejection.notText("bin")) { try UploadPolicy.check(Data([0x61, 0x00]), name: "bin") }
        #expect(throws: UploadPolicy.Rejection.empty("e")) { try UploadPolicy.check(Data(), name: "e") }
        let webp = Data("RIFF".utf8) + Data([0, 0, 0, 0]) + Data("WEBP".utf8)
        #expect(UploadPolicy.imageType(webp) == "image/webp")
    }

    @Test func contextChipLabelsUseTheUploadedName() {
        let chip = ContextAttachment(path: "/u/abc", source: "Uploaded notes.txt · 12 bytes", bytes: 12, mediaType: "text/plain")
        #expect(chip.label == "notes.txt")
        #expect(ContextAttachment(path: "/w/src/main.rs", source: nil, bytes: nil, mediaType: nil).label == "main.rs")
    }

    @Test func checklistBoardReadsTasksAndHiddenState() throws {
        let result = try value(#"{"body":{"tasks":[{"id":"1","title":"Plan","status":"completed"},{"id":"2","title":"Build","status":"in_progress"}]}}"#)
        let tasks = try #require(ChecklistTask.board(from: result))
        #expect(tasks.map(\.title) == ["Plan", "Build"])
        #expect(tasks[0].isDone)
        #expect(ChecklistTask.board(from: try value(#"{"body":{"tasks":[{"title":"x"}],"hidden":true}}"#)) == [])
        #expect(ChecklistTask.board(from: try value(#"{"body":{}}"#)) == nil)
    }
}
