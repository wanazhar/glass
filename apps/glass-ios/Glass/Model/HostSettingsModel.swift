import Foundation
import GlassAPI
import GlassModels
import GlassTransport
import Observation

/// The host's revisioned settings. A save carries the revision it was based on; when the host reports a
/// conflict, the edit stays in `drafts` so the owner can refresh and save again without retyping.
@MainActor @Observable
final class HostSettingsModel {
    private(set) var snapshot: SettingsSnapshot?
    private(set) var loading = false
    private(set) var saving = false
    private(set) var conflict = false
    private(set) var feedback: String?
    private(set) var failed = false
    /// Unsaved text edits keyed by field, kept across refreshes and conflicts.
    var drafts: [String: String] = [:]

    private let app: AppModel

    init(app: AppModel) { self.app = app }

    var busy: Bool { loading || saving }

    func load() async {
        guard !loading, let api = app.api else { return }
        loading = true
        defer { loading = false }
        do {
            snapshot = try await api.settings()
            conflict = false
            if failed { feedback = nil; failed = false }
        } catch {
            fail(error)
        }
    }

    /// Saves one change. Returns true when the host accepted it; clears the field's draft only then.
    @discardableResult
    func save(_ change: SettingsChange, draft key: String? = nil) async -> Bool {
        guard !busy else { feedback = "Wait for the current settings request to finish."; failed = true; return false }
        guard let api = app.api, let revision = snapshot?.revision else { return false }
        saving = true
        feedback = "Saving…"
        failed = false
        defer { saving = false }
        do {
            snapshot = try await api.updateSettings(change, revision: revision)
            if let key { drafts[key] = nil }
            conflict = false
            feedback = "Saved"
            return true
        } catch {
            fail(error)
            return false
        }
    }

    func draft(_ key: String, _ saved: String) -> String { drafts[key] ?? saved }

    private func fail(_ error: GlassError) {
        if error.isUnauthorized { app.handle(error); return }
        let message = error.localizedDescription
        conflict = SettingsChange.isConflict(message)
        feedback = conflict ? "Settings changed on the host. Refresh to load them; your unsaved edits are kept." : message
        failed = true
    }
}
