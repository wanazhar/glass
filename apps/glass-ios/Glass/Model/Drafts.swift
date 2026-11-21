import Foundation
import GlassModels

/// Device-local drafts in Application Support, written 300 ms after the last keystroke.
@MainActor
final class Drafts {
    static let shared = Drafts()

    private var store: DraftStore
    private let url: URL
    private var pending: Task<Void, Never>?

    init(url: URL = Drafts.defaultURL) {
        self.url = url
        store = DraftStore.load(from: url)
    }

    static var defaultURL: URL {
        URL.applicationSupportDirectory.appending(path: "Glass/drafts.v1.json")
    }

    func read(_ thread: String) -> String { store.read(thread) }

    func write(_ thread: String, _ text: String) {
        store.write(thread, text)
        pending?.cancel()
        pending = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(300))
            guard !Task.isCancelled else { return }
            self?.flush()
        }
    }

    func flush() {
        pending?.cancel()
        pending = nil
        try? store.save(to: url)
    }

    func removeAll() {
        store = DraftStore()
        try? FileManager.default.removeItem(at: url)
    }
}
