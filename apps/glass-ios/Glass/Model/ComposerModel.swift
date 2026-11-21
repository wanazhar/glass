import Foundation
import GlassAPI
import GlassModels
import GlassTransport
import Observation

/// The composer for one conversation: drafts, sending, control, interactions, uploads and commands.
@MainActor @Observable
final class ComposerModel {
    enum SendMode: String, CaseIterable, Identifiable {
        /// Applies the message to the running turn.
        case steer
        /// Runs the message after the current turn finishes.
        case queue
        var id: String { rawValue }
        var title: String { self == .steer ? "Steer" : "Queue" }
    }

    struct Upload: Identifiable, Equatable {
        let id = UUID()
        var name: String
        var fraction: Double = 0
        var failure: String?
    }

    struct Choice: Identifiable, Hashable {
        var id: String
        var label: String
    }

    enum Picker: String, Identifiable {
        case model, profiles
        var id: String { rawValue }
        var title: String { self == .model ? "Model" : "Execution profile" }
    }

    enum CommandError: LocalizedError {
        case failed(String)
        case timedOut
        var errorDescription: String? {
            switch self {
            case .failed(let message): message
            case .timedOut: "The host didn’t answer the command. Check the conversation before trying again."
            }
        }
    }

    var text: String {
        didSet { if text != oldValue { drafts.write(thread.id, text) } }
    }
    var mode: SendMode = .steer
    private(set) var sending = false
    private(set) var uploads: [Upload] = []
    private(set) var context: [ContextAttachment] = []
    private(set) var checklist: [ChecklistTask] = []
    private(set) var model: String?
    private(set) var submittingSecret = false
    /// Commands waiting for the running turn to finish.
    private(set) var queuedCommands: Set<String> = []
    var showingEditor = false
    var showingCommands = false
    /// A transient confirmation, such as "Attached notes.txt to future requests."
    var confirmation: String?
    var error: String?

    let thread: ThreadSummary
    private let app: AppModel
    private let drafts: Drafts
    private var pendingCommands: [String: CheckedContinuation<JSONValue, any Error>] = [:]
    private var uploadTask: Task<Void, Never>?
    private var stateToolRunning = false

    init(app: AppModel, thread: ThreadSummary, drafts: Drafts = .shared) {
        self.app = app
        self.thread = thread
        self.drafts = drafts
        model = thread.model
        text = drafts.read(thread.id)
    }

    var trimmed: String { text.trimmingCharacters(in: .whitespacesAndNewlines) }

    // MARK: Sending

    /// Sends the draft: a prompt when idle, otherwise a steer or a queued prompt per `mode`.
    func send(active: Bool, canWrite: Bool) async {
        let message = trimmed
        guard !message.isEmpty, !sending else { return }
        guard canWrite else { error = "Take control before sending a message."; return }
        guard let api = app.api else { return }
        let id = SendMessage.newRequestID()
        let saved = text
        text = ""
        sending = true
        defer { sending = false }
        do {
            try await api.send(active && mode == .steer ? .steer(text: message, requestID: id) : .prompt(text: message, requestID: id),
                               thread: thread.id, sourceRef: thread.sourceRef)
        } catch {
            if text.isEmpty { text = saved }
            report(error)
        }
    }

    func stop() async { await post(.cancel) }

    func claimControl() async { await post(.claimControl) }

    func releaseControl() async { await post(.releaseControl) }

    func answer(_ interaction: Interaction, _ value: JSONValue) async {
        await post(.answer(interactionID: interaction.id, answer: value))
    }

    /// Declines whatever the turn is waiting on.
    func decline(_ interaction: Interaction) async {
        switch interaction.kind {
        case .approval: await answer(interaction, Interaction.approval(allow: false))
        case .questions: await answer(interaction, Interaction.cancelled)
        case .secret, .other: await post(.cancel)
        }
    }

    /// Sends the secret once over HTTPS, then clears both the field and the byte buffer.
    func submitSecret(_ value: inout String, for interaction: Interaction) async {
        var bytes = Array(value.utf8)
        value = ""
        guard let api = app.api else { return }
        submittingSecret = true
        defer { submittingSecret = false }
        do {
            try await api.submitSecret(&bytes, thread: thread.id, sourceRef: thread.sourceRef, interactionID: interaction.id)
        } catch {
            report(error)
        }
    }

    private func post(_ message: SendMessage) async {
        guard let api = app.api else { return }
        do { try await api.send(message, thread: thread.id, sourceRef: thread.sourceRef) } catch { report(error) }
    }

    // MARK: Commands

    /// Runs a control-channel command and waits for its `command_result`.
    func command(_ name: String, _ values: [String: JSONValue] = [:], requestID id: String = SendMessage.newRequestID(),
                 timeout: Duration = .seconds(60)) async throws -> JSONValue {
        guard let api = app.api else { throw CommandError.failed("Not connected to a host.") }
        let thread = thread
        return try await withCheckedThrowingContinuation { continuation in
            pendingCommands[id] = continuation
            Task {
                do {
                    try await api.send(.command(name: name, values: .object(values), requestID: id), thread: thread.id, sourceRef: thread.sourceRef)
                } catch {
                    self.resolve(id, .failure(error))
                }
            }
            Task {
                try? await Task.sleep(for: timeout)
                self.resolve(id, .failure(CommandError.timedOut))
            }
        }
    }

    /// Asks the host to stop a queued or running command; its `command_result` settles the wait.
    func cancelCommand(_ id: String) async { await post(.cancelCommand(requestID: id)) }

    private func resolve(_ id: String, _ result: Result<JSONValue, any Error>) {
        queuedCommands.remove(id)
        pendingCommands.removeValue(forKey: id)?.resume(with: result)
    }

    /// Live events the composer cares about: command replies and checklist changes.
    func observe(_ event: GlassEvent) {
        switch event {
        case .commandQueued(let id?):
            if pendingCommands[id] != nil { queuedCommands.insert(id) }
        case .commandResult(let id?, let result, let failure):
            if let failure { resolve(id, .failure(CommandError.failed(failure))) } else { resolve(id, .success(result ?? .null)) }
        case .tool(_, let name, _):
            stateToolRunning = name == "state"
        case .toolEnd:
            if stateToolRunning { stateToolRunning = false; Task { await refreshChecklist() } }
        case .done:
            Task { await refreshContext() }
        default:
            break
        }
    }

    // MARK: Model and profile

    func choices(for picker: Picker) async throws -> [Choice] {
        let result = try await command(picker.rawValue, ["action": "list"])
        let body = result["body"]
        let rows = body?["models"] ?? body?["profiles"] ?? body ?? .null
        switch rows {
        case .array(let items):
            return items.compactMap { item in
                if let key = item.stringValue { return Choice(id: key, label: key) }
                guard let key = item["id"]?.stringValue ?? item["name"]?.stringValue else { return nil }
                return Choice(id: key, label: item["label"]?.stringValue ?? item["name"]?.stringValue ?? key)
            }
        case .object(let object):
            return object.keys.sorted().map { key in
                let value = object[key]
                return Choice(id: key, label: value?.stringValue ?? value?["label"]?.stringValue ?? value?["name"]?.stringValue ?? key)
            }
        default:
            return []
        }
    }

    func choose(_ choice: String, for picker: Picker) async throws {
        _ = try await command(picker.rawValue, ["action": picker == .model ? "set" : "use", "args": .string(choice), "confirmed": true])
        if picker == .model { model = choice }
        confirmation = picker == .model ? "Model set to \(choice)." : "Using profile \(choice)."
    }

    // MARK: Attachments

    /// Uploads up to 8 files in order, attaching each to future requests.
    func attach(_ files: [(name: String, data: Data)], canWrite: Bool) {
        guard canWrite else { error = "Take control before attaching files."; return }
        guard uploadTask == nil else { return }
        let batch = Array(files.prefix(UploadPolicy.filesPerBatch))
        if files.count > batch.count { error = "Attach up to \(UploadPolicy.filesPerBatch) files at a time." }
        uploads = batch.map { Upload(name: $0.name) }
        uploadTask = Task {
            for (index, file) in batch.enumerated() {
                guard !Task.isCancelled else { break }
                await upload(file, at: index)
            }
            uploadTask = nil
            uploads.removeAll { $0.failure == nil }
            await refreshContext()
        }
    }

    func cancelUploads() {
        uploadTask?.cancel()
        uploadTask = nil
        uploads.removeAll()
    }

    func dismissUpload(_ upload: Upload) { uploads.removeAll { $0.id == upload.id } }

    private func upload(_ file: (name: String, data: Data), at index: Int) async {
        guard let api = app.api else { return }
        do {
            _ = try UploadPolicy.check(file.data, name: file.name)
            let receipt = try await api.upload(file.data, name: file.name, thread: thread.id, sourceRef: thread.sourceRef) { sent, total in
                Task { @MainActor [weak self] in
                    guard let self, self.uploads.indices.contains(index), total > 0 else { return }
                    self.uploads[index].fraction = min(Double(sent) / Double(total), 1)
                }
            }
            if uploads.indices.contains(index) { uploads[index].fraction = 1 }
            _ = try await command("attach", ["upload": .string(receipt.token), "upload_name": .string(file.name), "args": ""])
            confirmation = "Attached \(file.name) to future requests."
        } catch {
            if uploads.indices.contains(index) { uploads[index].failure = error.localizedDescription }
        }
    }

    func refreshContext() async {
        guard let api = app.api else { return }
        if let attachments = try? await api.context(thread.id, sourceRef: thread.sourceRef) { context = attachments }
    }

    func remove(_ attachment: ContextAttachment) async {
        do {
            _ = try await command("context", ["action": "remove", "args": .string(attachment.path)])
            context.removeAll { $0.id == attachment.id }
        } catch {
            report(error)
        }
        await refreshContext()
    }

    func refreshChecklist() async {
        guard let api = app.api, let result = try? await api.inspect("todo", thread: thread.id, sourceRef: thread.sourceRef) else { return }
        checklist = ChecklistTask.board(from: result) ?? []
    }

    func attachment(_ id: String) async -> Data? {
        try? await app.api?.attachment(id, thread: thread.id, sourceRef: thread.sourceRef)
    }

    func flushDraft() { drafts.flush() }

    private func report(_ error: any Error) {
        if let error = error as? GlassError, error.isUnauthorized { app.handle(error); return }
        self.error = error.localizedDescription
    }
}
