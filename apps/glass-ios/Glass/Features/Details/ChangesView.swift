import GlassModels
import SwiftUI

/// Git changes in the conversation's folder, with per-file diffs.
struct ChangesView: View {
    let conversation: ConversationModel
    @Environment(AppModel.self) private var app
    @State private var changes: WorkspaceChanges?
    @State private var error: String?

    var body: some View {
        List {
            if let changes {
                if changes.files.isEmpty {
                    ContentUnavailableView("No changes", systemImage: "checkmark.circle",
                                           description: Text("The folder matches the last commit."))
                } else {
                    Section {
                        ForEach(changes.files) { file in
                            NavigationLink {
                                DiffView(conversation: conversation, file: file, hunks: changes.hunks(for: file.path),
                                         expected: changes.expected, reload: load)
                            } label: {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(file.path).font(.body.monospaced()).lineLimit(2).truncationMode(.head)
                                    Text(file.isStaged ? "\(file.summary), staged" : file.summary)
                                        .font(.caption).foregroundStyle(.secondary)
                                }
                            }
                            .accessibilityIdentifier("changes.file")
                        }
                    } header: {
                        Text(changes.branch.map { "On \($0)" } ?? "Changed files")
                    }
                }
            } else if let error {
                ContentUnavailableView("Can’t read changes", systemImage: "exclamationmark.triangle", description: Text(error))
            }
        }
        .overlay { if changes == nil && error == nil { ProgressView() } }
        .navigationTitle("Changes")
        .task { await load() }
        .refreshable { await load() }
    }

    private func load() async {
        guard let api = app.api else { return }
        do {
            changes = try await api.changes(thread: conversation.thread.id, sourceRef: conversation.thread.sourceRef)
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}

struct DiffView: View {
    let conversation: ConversationModel
    let file: WorkspaceChanges.File
    let hunks: [DiffHunk]
    let expected: String?
    let reload: () async -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var busy = false
    @State private var error: String?
    @State private var discarding: DiffHunk?

    private var canAct: Bool { conversation.canWrite && expected != nil && !busy }

    var body: some View {
        ScrollView([.vertical, .horizontal]) {
            VStack(alignment: .leading, spacing: 0) {
                if let error {
                    Text(error).font(.footnote).foregroundStyle(.red).padding(12)
                        .accessibilityIdentifier("changes.error")
                }
                if !conversation.canWrite {
                    Text("Take control of the conversation to stage or discard.")
                        .font(.footnote).foregroundStyle(.secondary).padding(12)
                }
                if hunks.isEmpty {
                    Text(file.isStaged ? "This file is staged." : "No line changes to show. The file may be new, binary or only renamed.")
                        .foregroundStyle(.secondary)
                        .padding()
                }
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(hunks) { hunk in
                        ForEach(Array(hunk.lines.enumerated()), id: \.offset) { _, line in
                            Text(line.isEmpty ? " " : line)
                                .font(.system(.caption, design: .monospaced))
                                .foregroundStyle(line.hasPrefix("@@") ? .secondary : .primary)
                                .padding(.horizontal, 12)
                                .padding(.vertical, 1)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .background(Self.tint(line))
                        }
                        HStack(spacing: 16) {
                            Button("Stage these lines") { Task { await git("stage-patch", patch: hunk.patch) } }
                                .accessibilityIdentifier("changes.stageHunk")
                            Button("Discard", role: .destructive) { discarding = hunk }
                                .accessibilityIdentifier("changes.discardHunk")
                        }
                        .font(.footnote)
                        .disabled(!canAct)
                        .padding(.horizontal, 12)
                        .padding(.vertical, 6)
                        Divider().padding(.bottom, 4)
                    }
                }
                .textSelection(.enabled)
            }
        }
        .navigationTitle((file.path as NSString).lastPathComponent)
        .navigationBarTitleDisplayMode(.inline)
        .accessibilityIdentifier("changes.diff")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button(file.isStaged ? "Unstage" : "Stage") {
                    Task { await git(file.isStaged ? "unstage" : "stage", args: file.path) }
                }
                .disabled(!canAct)
                .accessibilityIdentifier("changes.stageFile")
            }
        }
        .confirmationDialog("Discard these lines?", isPresented: Binding(get: { discarding != nil }, set: { if !$0 { discarding = nil } }),
                            titleVisibility: .visible) {
            Button("Discard", role: .destructive) {
                if let hunk = discarding { Task { await git("revert-patch", patch: hunk.patch) } }
            }
        } message: {
            Text("The lines go back to the last saved version in the folder. This can’t be undone from Glass.")
        }
    }

    private func git(_ action: String, args: String? = nil, patch: String? = nil) async {
        guard let expected else { return }
        busy = true
        defer { busy = false }
        var values: [String: JSONValue] = ["action": .string(action), "expected": .string(expected), "confirmed": .bool(true)]
        if let args { values["args"] = .string(args) }
        if let patch { values["patch"] = .string(patch) }
        do {
            _ = try await conversation.composer.command("git", values)
            error = nil
            await reload()
            dismiss()
        } catch {
            self.error = error.localizedDescription
        }
    }

    private static func tint(_ line: String) -> Color {
        if line.hasPrefix("+") { return .green.opacity(0.15) }
        if line.hasPrefix("-") { return .red.opacity(0.15) }
        return .clear
    }
}
