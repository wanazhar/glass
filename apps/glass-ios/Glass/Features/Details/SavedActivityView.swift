import GlassModels
import SwiftUI

/// A large saved activity record, read from the host one page at a time.
struct SavedActivityView: View {
    let conversation: ConversationModel
    let record: Int
    @Environment(AppModel.self) private var app
    @Environment(\.dismiss) private var dismiss
    @State private var page: ActivityTextPage?
    @State private var previous: [Int] = []
    @State private var loading = false
    @State private var error: String?

    var body: some View {
        NavigationStack {
            ScrollView([.vertical, .horizontal]) {
                if let page {
                    Text(page.text)
                        .font(.system(.footnote, design: .monospaced))
                        .textSelection(.enabled)
                        .padding()
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .accessibilityIdentifier("saved.text")
                } else if let error {
                    ContentUnavailableView("Can’t read this record", systemImage: "doc.text.magnifyingglass", description: Text(error))
                }
            }
            .overlay { if loading && page == nil { ProgressView() } }
            .navigationTitle("Saved activity")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
                ToolbarItemGroup(placement: .bottomBar) {
                    Button("Previous", systemImage: "chevron.left") {
                        if let offset = previous.popLast() { Task { await load(offset, back: true) } }
                    }
                    .disabled(previous.isEmpty || loading)
                    Spacer()
                    if let page {
                        Text(progress(page)).font(.caption).foregroundStyle(.secondary).monospacedDigit()
                    }
                    Spacer()
                    Button("Next", systemImage: "chevron.right") {
                        if let page { Task { await load(page.nextOffset, back: false) } }
                    }
                    .disabled(page?.eof != false || loading)
                    .accessibilityIdentifier("saved.next")
                }
            }
            .task { await load(0, back: true) }
        }
    }

    private func progress(_ page: ActivityTextPage) -> String {
        let shown = ByteCountFormatter.string(fromByteCount: Int64(page.nextOffset), countStyle: .file)
        let total = ByteCountFormatter.string(fromByteCount: Int64(page.totalBytes), countStyle: .file)
        return "\(shown) of \(total)"
    }

    private func load(_ offset: Int, back: Bool) async {
        guard let api = app.api else { return }
        loading = true
        defer { loading = false }
        do {
            let next = try await api.activityText(record: record, offset: offset, expected: page?.revision,
                                                  thread: conversation.thread.id, sourceRef: conversation.thread.sourceRef)
            if !back, let current = page { previous.append(current.offset) }
            page = next
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}
