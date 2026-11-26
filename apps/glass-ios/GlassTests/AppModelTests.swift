import Foundation
import GlassAuth
import GlassModels
import GlassTransport
import Testing
@testable import Glass

private final class ScriptedTransport: GlassHTTPTransport, @unchecked Sendable {
    let lock = NSLock()
    var handler: @Sendable (URLRequest) -> (Int, String)
    init(_ handler: @escaping @Sendable (URLRequest) -> (Int, String)) { self.handler = handler }
    func set(_ handler: @escaping @Sendable (URLRequest) -> (Int, String)) { lock.withLock { self.handler = handler } }
    func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        let (status, body) = lock.withLock { handler }(request)
        return (Data(body.utf8), HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: nil, headerFields: nil)!)
    }
}

private actor AttachmentBatchTransport: GlassHTTPTransport {
    let owner: String
    let pauseSecondUpload: Bool
    private(set) var commandIDs: [String] = []
    private(set) var secondUploadStarted = false

    init(owner: String, pauseSecondUpload: Bool) {
        self.owner = owner
        self.pauseSecondUpload = pauseSecondUpload
    }

    func send(_ request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        let url = try #require(request.url)
        let body: String
        switch url.path {
            case "/api/local": body = #"{"ownerCapability":"\#(owner)"}"#
            case "/api/upload":
                if url.query?.contains("name=second.txt") == true {
                    secondUploadStarted = true
                    if pauseSecondUpload { try await Task.sleep(for: .seconds(60)) }
                }
                body = #"{"name":"first.txt","bytes":5,"token":"upload-token","kind":"text"}"#
            case "/api/send":
                let message = try JSONDecoder().decode(JSONValue.self, from: #require(request.httpBody))["message"]
                #expect(message?["name"]?.stringValue == "attach")
                commandIDs.append(try #require(message?["request_id"]?.stringValue))
                body = "{}"
            case "/api/context":
                try Task.checkCancellation()
                body = #"{"attachments":[{"path":"/uploads/first.txt","source":"Uploaded first.txt · snapshot"}]}"#
            default: body = AppModelTests.status
        }
        return (Data(body.utf8), try #require(HTTPURLResponse(url: url, statusCode: 200, httpVersion: nil, headerFields: nil)))
    }
}

@MainActor @Suite struct AppModelTests {
    let defaults: UserDefaults = {
        let name = "glass.tests.\(UUID().uuidString)"
        return UserDefaults(suiteName: name)!
    }()
    let store = InMemoryCapabilityStore()
    let owner = String(repeating: "c", count: 64)
    nonisolated static let link = "https://mac.tail1.ts.net#owner=AbCdEfGhIjKlMnOpQr_-12"
    nonisolated static let status = #"{"workspace":"/Users/o/glass","remote":true,"protocolVersion":1,"capabilities":["events-resume"]}"#

    private func happyTransport() -> ScriptedTransport {
        let owner = owner
        return ScriptedTransport { request in
            request.url?.path == "/api/local" ? (200, #"{"ownerCapability":"\#(owner)"}"#) : (200, Self.status)
        }
    }

    @Test func pairingPersistsHostAndConnects() async throws {
        let model = AppModel(defaults: defaults, store: store, transport: happyTransport())
        await model.pair(with: Self.link)
        #expect(model.pairingError == nil)
        #expect(model.status?.workspaceName == "glass")
        #expect(model.endpoint?.origin == "https://mac.tail1.ts.net")
        #expect(try store.credential(for: model.endpoint!) == .ownerCapability(owner))

        let relaunched = AppModel(defaults: defaults, store: store, transport: happyTransport())
        #expect(relaunched.endpoint == model.endpoint)
        #expect(relaunched.phase == .connecting)
    }

    @Test func demoOpenFailureKeepsHistoryAndOffersReconnection() async throws {
        let model = AppModel.demo()
        await model.refresh()
        let threads = ThreadsModel(app: model)
        await threads.reload()
        #expect(threads.threads.count == 336)
        let summary = try #require(threads.thread("harbor-1"))
        let conversation = threads.conversation(for: summary)
        await conversation.start()
        #expect(conversation.loadedHistory)
        #expect(!conversation.timeline.isEmpty)
        #expect(!conversation.canWrite)
        #expect(!conversation.needsControl)
        #expect(conversation.connectionMessage == "The studio Mac is offline.")
    }

    @Test func demoDraftsDoNotCarryOverBetweenLaunches() {
        let thread = ThreadSummary(session: "harbor-0")
        let first = AppModel.demo()
        let composer = ComposerModel(app: first, thread: thread)
        composer.text = "A demo draft"
        #expect(ComposerModel(app: first, thread: thread).text == "A demo draft")
        #expect(ComposerModel(app: AppModel.demo(), thread: thread).text.isEmpty)
    }

    @Test func suggestionsPreferTheCurrentWorkspace() {
        #expect(NewChatView.directory("/Users/robin/Projects/Maps", hostDefault: "/Users/robin/Projects/Harbor")
                == "/Users/robin/Projects/Maps")
        #expect(NewChatView.directory("", hostDefault: "/Users/robin/Projects/Harbor")
                == "/Users/robin/Projects/Harbor")
    }

    @Test func leavingIdleConversationKeepsItsStreamAndTimeline() async throws {
        let model = AppModel.demo()
        await model.refresh()
        let threads = ThreadsModel(app: model)
        await threads.reload()
        let summary = try #require(threads.thread("harbor-0"))
        threads.selection = summary.id
        let conversation = threads.conversation(for: summary)
        await conversation.start()
        defer { conversation.stop() }
        for _ in 0..<100 where !conversation.canWrite { try await Task.sleep(for: .milliseconds(20)) }
        #expect(conversation.canWrite)
        threads.selection = "harbor-1"
        #expect(conversation.connection == .live)
        #expect(conversation.canWrite)
        #expect(!conversation.timeline.isEmpty)
    }

    @Test func continuingAfterFailureDoesNotReplaceTheDraft() async throws {
        let transport = happyTransport()
        let model = AppModel(defaults: defaults, store: store, transport: transport)
        await model.pair(with: Self.link)
        let drafts = Drafts(url: URL.temporaryDirectory.appending(path: "\(UUID()).json"))
        defer { drafts.removeAll() }
        let composer = ComposerModel(app: model, thread: ThreadSummary(session: "retry"), drafts: drafts)
        composer.text = "A different question"
        transport.set { request in
            let body = request.httpBody.flatMap { try? JSONDecoder().decode(JSONValue.self, from: $0) }
            #expect(request.url?.path == "/api/send")
            #expect(body?["message"]?["text"]?.stringValue == "Continue from the failed step.")
            return (200, "{}")
        }
        await composer.continueFromFailure(canWrite: true)
        #expect(composer.error == nil)
        #expect(composer.text == "A different question")
        #expect(drafts.read("retry") == "A different question")
    }

    @Test func demoModelPickerReceivesChoicesFromTheLiveStream() async throws {
        let model = AppModel.demo()
        await model.refresh()
        let threads = ThreadsModel(app: model)
        await threads.reload()
        let summary = try #require(threads.thread("harbor-0"))
        let conversation = threads.conversation(for: summary)
        await conversation.start()
        defer { conversation.stop() }
        for _ in 0..<100 where !conversation.canWrite { try await Task.sleep(for: .milliseconds(20)) }
        try #require(conversation.canWrite)
        let choices = try await conversation.composer.choices(for: .model)
        #expect(choices.map(\.label).contains("Claude Opus 4.7"))
    }

    @Test func revokedCapabilityReturnsToPairingWithReason() async throws {
        let transport = happyTransport()
        let model = AppModel(defaults: defaults, store: store, transport: transport)
        await model.pair(with: Self.link)
        transport.set { _ in (403, #"{"error":"Open the private Connect owner link","ownerAuthRequired":true}"#) }
        await model.refresh()
        #expect(model.phase == .unpaired)
        #expect(model.repairReason == "Open the private Connect owner link")
        #expect(try store.credential(for: model.endpoint!) == .none)
    }

    @Test(arguments: [true, false])
    func cancellingRemainderOfAttachmentBatchRefreshesCompletedAttachments(pauseSecondUpload: Bool) async throws {
        let transport = AttachmentBatchTransport(owner: owner, pauseSecondUpload: pauseSecondUpload)
        let model = AppModel(defaults: defaults, store: store, transport: transport)
        await model.pair(with: Self.link)
        try #require(model.api != nil)
        let composer = ComposerModel(app: model, thread: ThreadSummary(session: "cancelled-attachment-batch"))
        composer.attach([AttachmentInput(name: "first.txt", data: Data("first".utf8)),
                         AttachmentInput(name: "second.txt", data: Data("second".utf8)),
                         AttachmentInput(name: "third.txt", data: Data("third".utf8))], canWrite: true)

        for _ in 0..<100 where await transport.commandIDs.isEmpty { try await Task.sleep(for: .milliseconds(20)) }
        let firstCommandID = await transport.commandIDs.first
        let id = try #require(firstCommandID)
        composer.observe(.commandResult(requestID: id, result: .null, error: nil))
        for _ in 0..<100 where await !transport.secondUploadStarted { try await Task.sleep(for: .milliseconds(20)) }
        let secondUploadStarted = await transport.secondUploadStarted
        try #require(secondUploadStarted)
        var pendingCommandID: String?
        if !pauseSecondUpload {
            for _ in 0..<100 where await transport.commandIDs.count < 2 { try await Task.sleep(for: .milliseconds(20)) }
            let ids = await transport.commandIDs
            pendingCommandID = try #require(ids.dropFirst().first)
        }

        composer.cancelUploads()
        #expect(composer.uploads.isEmpty)
        for _ in 0..<100 where composer.context.isEmpty { try await Task.sleep(for: .milliseconds(20)) }
        #expect(composer.context.map(\.label) == ["first.txt"])
        if let pendingCommandID { composer.observe(.commandResult(requestID: pendingCommandID, result: nil, error: "Not attached")) }
    }

    @Test func desktopDisconnectReleasesHeldButtonsAndBlocksInput() async throws {
        let transport = happyTransport()
        let model = AppModel(defaults: defaults, store: store, transport: transport)
        await model.pair(with: Self.link)
        let sent = NSLock()
        nonisolated(unsafe) var inputs: [String] = []
        transport.set { request in
            guard request.url?.path == "/api/desktop/input" else { return (200, Self.status) }
            let body = request.httpBody.flatMap { String(data: $0, encoding: .utf8) } ?? ""
            sent.withLock { inputs.append(body) }
            return (200, "{}")
        }
        let desktop = DesktopModel()
        desktop.attach(model.api)
        desktop.send(.button(x: 0.2, y: 0.3, button: .left, pressed: true))
        for _ in 0..<50 where sent.withLock({ inputs.isEmpty }) { try await Task.sleep(for: .milliseconds(20)) }
        desktop.disconnect()
        desktop.send(.click(x: 0.5, y: 0.5, button: .left))
        desktop.type("ls")
        for _ in 0..<50 where sent.withLock({ inputs.count }) < 2 { try await Task.sleep(for: .milliseconds(20)) }
        try await Task.sleep(for: .milliseconds(100))
        let got = sent.withLock { inputs }
        #expect(got.count == 2)
        #expect(got.allSatisfy { $0.contains("pointer-button") })
        #expect(got.last?.contains("false") == true)

        desktop.reconnect()
        desktop.send(.click(x: 0.5, y: 0.5, button: .left))
        for _ in 0..<50 where sent.withLock({ inputs.count }) < 3 { try await Task.sleep(for: .milliseconds(20)) }
        #expect(sent.withLock { inputs.count } == 3)
    }

    @Test func desktopDisconnectKeepsAReleaseQueuedBehindASlowInput() async throws {
        let transport = happyTransport()
        let model = AppModel(defaults: defaults, store: store, transport: transport)
        await model.pair(with: Self.link)
        let sent = NSLock()
        nonisolated(unsafe) var inputs: [String] = []
        let gate = DispatchSemaphore(value: 0)
        transport.set { request in
            guard request.url?.path == "/api/desktop/input" else { return (200, Self.status) }
            let body = request.httpBody.flatMap { String(data: $0, encoding: .utf8) } ?? ""
            sent.withLock { inputs.append(body) }
            if body.contains("pointer-move") { gate.wait() }
            return (200, "{}")
        }
        let desktop = DesktopModel()
        desktop.attach(model.api)
        desktop.send(.button(x: 0.2, y: 0.3, button: .left, pressed: true))
        desktop.send(.move(x: 0.7, y: 0.8))
        desktop.send(.button(x: 0.7, y: 0.8, button: .left, pressed: false))
        for _ in 0..<100 where sent.withLock({ inputs.count }) < 2 { try await Task.sleep(for: .milliseconds(20)) }
        #expect(sent.withLock { inputs.count } == 2)
        desktop.disconnect()
        gate.signal()
        for _ in 0..<100 where sent.withLock({ inputs.count }) < 3 { try await Task.sleep(for: .milliseconds(20)) }
        try await Task.sleep(for: .milliseconds(100))
        let got = sent.withLock { inputs }
        #expect(got.count == 3)
        #expect(got.last?.contains("pointer-button") == true)
        #expect(got.last?.contains("false") == true)
        // Released where the pointer is, not back at the press.
        #expect(got.last?.contains("\"x\"") == false)
    }

    @Test func forgettingOrRevokingTheHostClosesHeldConnections() async throws {
        let transport = happyTransport()
        let model = AppModel(defaults: defaults, store: store, transport: transport)
        await model.pair(with: Self.link)
        var stopped: [String] = []
        KeepAlive.shared.hold("conversation:test") { stopped.append("conversation") }
        model.forget()
        #expect(stopped == ["conversation"])
        #expect(!KeepAlive.shared.isConnected)

        await model.pair(with: Self.link)
        KeepAlive.shared.hold("desktop") { stopped.append("desktop") }
        transport.set { _ in (403, #"{"error":"Open the private Connect owner link","ownerAuthRequired":true}"#) }
        await model.refresh()
        #expect(stopped == ["conversation", "desktop"])
        #expect(!KeepAlive.shared.isConnected)
    }

    @Test func unreachableHostKeepsThePairing() async throws {
        let transport = happyTransport()
        let model = AppModel(defaults: defaults, store: store, transport: transport)
        await model.pair(with: Self.link)
        transport.set { _ in (502, #"{"error":"bad gateway"}"#) }
        await model.refresh()
        #expect(model.phase == .unreachable("bad gateway"))
        #expect(try store.credential(for: model.endpoint!) != .none)
    }

    @Test func lanToggleGatesHTTPLinksAndPersists() async {
        let model = AppModel(defaults: defaults, store: store, transport: happyTransport())
        await model.pair(with: "http://192.168.1.10:8080#owner=AbCdEfGhIjKlMnOpQr_-12")
        #expect(model.pairingError?.contains("HTTP") == true)
        model.allowLANHTTP = true
        await model.pair(with: "http://192.168.1.10:8080#owner=AbCdEfGhIjKlMnOpQr_-12")
        #expect(model.pairingError == nil)
        #expect(model.endpoint?.isSecure == false)
        #expect(AppModel(defaults: defaults, store: store, transport: happyTransport()).allowLANHTTP)
    }

    @Test func forgetClearsKeychainAndHost() async throws {
        let model = AppModel(defaults: defaults, store: store, transport: happyTransport())
        await model.pair(with: Self.link)
        let endpoint = try #require(model.endpoint)
        model.forget()
        #expect(model.phase == .unpaired)
        #expect(try store.credential(for: endpoint) == .none)
        #expect(AppModel(defaults: defaults, store: store, transport: happyTransport()).endpoint == nil)
    }

    @Test func titleCasingForDiagnosticsKeys() {
        #expect("bridgeCount".titleCased == "Bridge Count")
        #expect("schema_version".titleCased == "Schema Version")
        #expect(JSONValue.number(12_222_717_952).displayString == "12222717952")
    }
}
