import Foundation
import GlassAPI
import GlassModels
import GlassTransport
import Observation

/// One open command form: its inputs, the run in flight and the latest result.
@MainActor @Observable
final class CommandRun {
    let command: CommandDescriptor
    var inputs: [String: String] = [:]
    var confirmed = false
    private(set) var running = false
    private(set) var inspecting = false
    private(set) var output: CommandOutput?
    private(set) var error: String?
    private(set) var requestID: String?
    private(set) var cancelling = false

    private let composer: ComposerModel
    private let app: AppModel

    init(command: CommandDescriptor, composer: ComposerModel, app: AppModel, args: String = "") {
        self.command = command
        self.composer = composer
        self.app = app
        if let action = command.form.fields.first(where: { $0.key == "action" })?.choices?.first { inputs["action"] = action }
        if !args.isEmpty { inputs["args"] = args }
    }

    var action: String? { inputs["action"] }
    var fields: [CommandField] { command.visibleFields(for: action) }
    var queued: Bool { requestID.map(composer.queuedCommands.contains) ?? false }

    var status: String {
        if cancelling { return "Stopping…" }
        if queued { return "Waiting for the current turn to finish. This command will run once." }
        if inspecting { return "Loading…" }
        if running { return command.form.modelRequest ? "Working. This uses the model." : "Working…" }
        return command.form.modelRequest ? "This uses the model when it runs." : "Runs on your host. Nothing is sent to the model."
    }

    var canCancel: Bool { running && !inspecting && command.cancellable && requestID != nil }

    /// Loads current state without changing anything, as the web UI does when a command opens.
    func open(canWrite: Bool) async {
        guard command.inspectsOnOpen, output == nil else { return }
        await run(inspect: true, canWrite: canWrite)
    }

    func run(inspect: Bool = false, canWrite: Bool) async {
        guard !running else { return }
        if !inspect, !command.readonly, !canWrite {
            error = "Take control before changing this session."
            return
        }
        running = true
        inspecting = inspect
        error = nil
        defer { running = false; inspecting = false; requestID = nil; cancelling = false }
        let values: JSONValue = inspect ? .object([:]) : command.values(inputs, confirmed: confirmed)
        do {
            let result: JSONValue
            if command.readonly {
                guard let api = app.api else { throw ComposerModel.CommandError.failed("Not connected to a host.") }
                let thread = composer.thread
                result = try await api.inspect(command.name, values: values, thread: thread.id, sourceRef: thread.sourceRef)
            } else {
                let id = SendMessage.newRequestID()
                requestID = id
                guard case .object(let object) = values else { return }
                result = try await composer.command(command.name, object, requestID: id, timeout: .seconds(300))
            }
            output = CommandOutput(result)
            if !inspect { confirmed = false; inputs["secret"] = nil }
        } catch let failure as ComposerModel.CommandError {
            if case .timedOut = failure {
                error = "No completion receipt received. The command may still be running; inspect its state before retrying."
            } else {
                error = failure.localizedDescription
            }
        } catch {
            if let error = error as? GlassError, error.isUnauthorized { app.handle(error) }
            self.error = error.localizedDescription
        }
    }

    func cancel() async {
        guard let requestID else { return }
        cancelling = true
        await composer.cancelCommand(requestID)
    }
}
