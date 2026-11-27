import GlassModels
import SwiftUI

/// The conversation's work: teammates, Git changes, files, attached files, export and restart.
struct DetailsView: View {
    let conversation: ConversationModel
    let openAgent: (Teammate) -> Void
    @Environment(AppModel.self) private var app
    @Environment(\.dismiss) private var dismiss
    @State private var exported: URL?
    @State private var exporting = false
    @State private var confirmingRestart = false
    @State private var restarting = false
    @State private var message: String?

    var body: some View {
        NavigationStack {
            List {
                Section {
                    NavigationLink { TeammatesView(conversation: conversation, open: openAgent) } label: {
                        Label("Teammates", systemImage: "person.2")
                    }
                    .accessibilityIdentifier("details.teammates")
                    NavigationLink { ChangesView(conversation: conversation) } label: {
                        Label("Changes", systemImage: "plusminus")
                    }
                    .accessibilityIdentifier("details.changes")
                    if let workspace = conversation.thread.workspace, !workspace.isEmpty {
                        NavigationLink { FolderView(conversation: conversation, path: workspace) } label: {
                            Label("Files", systemImage: "folder")
                        }
                        .accessibilityIdentifier("details.files")
                    }
                    NavigationLink { AttachedFilesView(composer: conversation.composer) } label: {
                        Label("Attached files", systemImage: "paperclip")
                    }
                    .accessibilityIdentifier("details.attached")
                } header: {
                    Text("Work")
                } footer: {
                    if let workspace = conversation.thread.workspace, !workspace.isEmpty {
                        Text(workspace).textSelection(.enabled)
                            .accessibilityLabel("Folder \((workspace as NSString).lastPathComponent)")
                    }
                }

                Section {
                    if let exported {
                        ShareLink(item: exported) {
                            Label("Share Markdown file", systemImage: "square.and.arrow.up")
                        }
                        .accessibilityIdentifier("details.share")
                    } else {
                        Button {
                            Task { await export() }
                        } label: {
                            HStack {
                                Label("Export conversation", systemImage: "arrow.down.doc")
                                if exporting { Spacer(); ProgressView() }
                            }
                        }
                        .disabled(exporting)
                        .accessibilityIdentifier("details.export")
                    }
                    Button("Restart session", systemImage: "arrow.clockwise", role: .destructive) {
                        confirmingRestart = true
                    }
                    .disabled(restarting)
                    .accessibilityIdentifier("details.restart")
                } header: {
                    Text("Conversation")
                } footer: {
                    Text("Restarting stops the host’s work for this conversation and starts it again. The history is kept.")
                }

                if let message {
                    Section { Text(message).accessibilityIdentifier("details.message") }
                }
            }
            .navigationTitle("Details")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }.accessibilityIdentifier("details.done")
                }
            }
            .confirmationDialog("Restart this session?", isPresented: $confirmingRestart, titleVisibility: .visible) {
                Button("Restart", role: .destructive) { Task { await restart() } }
            } message: {
                Text("Running work stops. The history is kept.")
            }
        }
    }

    private func export() async {
        guard let api = app.api else { return }
        exporting = true
        defer { exporting = false }
        do {
            let data = try await api.export(thread: conversation.thread.id, sourceRef: conversation.thread.sourceRef)
            let name = conversation.title.replacingOccurrences(of: "/", with: "-").prefix(80)
            let url = FileManager.default.temporaryDirectory.appending(path: "\(name.isEmpty ? "Conversation" : String(name)).md")
            try data.write(to: url, options: .atomic)
            exported = url
            message = nil
        } catch {
            message = "Couldn’t export: \(error.localizedDescription)"
        }
    }

    private func restart() async {
        guard let api = app.api else { return }
        restarting = true
        defer { restarting = false }
        do {
            _ = try await api.restartRuntime(thread: conversation.thread.id, sourceRef: conversation.thread.sourceRef)
            message = "Session restarted."
        } catch {
            message = "Couldn’t restart: \(error.localizedDescription)"
        }
    }
}

/// Files attached to future requests in this conversation.
struct AttachedFilesView: View {
    let composer: ComposerModel

    var body: some View {
        List {
            if composer.context.isEmpty {
                ContentUnavailableView("No attached files", systemImage: "paperclip",
                                       description: Text("Files you attach are sent with every message until you remove them."))
            } else {
                Section {
                    ForEach(composer.context) { attachment in
                        VStack(alignment: .leading, spacing: 2) {
                            Text((attachment.path as NSString).lastPathComponent)
                            if let bytes = attachment.bytes {
                                Text(ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file))
                                    .font(.caption).foregroundStyle(.secondary)
                            }
                        }
                        .swipeActions {
                            Button("Remove", role: .destructive) { Task { await composer.remove(attachment) } }
                        }
                    }
                } footer: {
                    Text("Sent with every message until removed.")
                }
            }
        }
        .navigationTitle("Attached files")
        .task { await composer.refreshContext() }
        .refreshable { await composer.refreshContext() }
    }
}
