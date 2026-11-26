import GlassModels
import PDFKit
import SwiftUI

/// One folder of the host's workspace, paged 256 entries at a time.
struct FolderView: View {
    let conversation: ConversationModel
    let path: String
    @Environment(AppModel.self) private var app
    @State private var entries: [BrowsePage.Entry] = []
    @State private var resolved: String?
    @State private var next: Int?
    @State private var loaded = false
    @State private var error: String?
    @State private var showHidden = false
    @State private var usage: String?

    var body: some View {
        List {
            if let usage {
                Text(usage).font(.footnote).foregroundStyle(.secondary).accessibilityIdentifier("files.usage")
            }
            ForEach(entries.filter { showHidden || !$0.isHidden }) { entry in
                let child = BrowsePage(path: resolved ?? path, entries: []).child(entry)
                if entry.isDirectory {
                    NavigationLink { FolderView(conversation: conversation, path: child) } label: { row(entry) }
                } else if entry.isOpenable {
                    NavigationLink {
                        FileView(conversation: conversation, folder: resolved ?? path, name: entry.name)
                    } label: { row(entry) }
                } else {
                    row(entry).foregroundStyle(.secondary)
                }
            }
            if let next {
                Button("Show more") { Task { await load(offset: next) } }.accessibilityIdentifier("files.more")
            }
            if loaded && entries.isEmpty && error == nil {
                Text("This folder is empty.").foregroundStyle(.secondary)
            }
            if let error {
                ContentUnavailableView("Can’t open this folder", systemImage: "folder.badge.questionmark", description: Text(error))
            }
        }
        .overlay { if !loaded && error == nil { ProgressView() } }
        .navigationTitle((path as NSString).lastPathComponent.isEmpty ? "Files" : (path as NSString).lastPathComponent)
        .navigationBarTitleDisplayMode(.inline)
        .accessibilityIdentifier("files")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Menu {
                    Toggle("Show hidden files", isOn: $showHidden)
                    Button("Folder size", systemImage: "internaldrive") { Task { await measure() } }
                } label: {
                    Label("Options", systemImage: "ellipsis")
                }
                .accessibilityIdentifier("files.options")
            }
        }
        .task { if !loaded { await load(offset: 0) } }
        .refreshable { await load(offset: 0) }
    }

    private func row(_ entry: BrowsePage.Entry) -> some View {
        HStack {
            Label(entry.name, systemImage: entry.isDirectory ? "folder" : "doc")
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer()
            if let bytes = entry.bytes ?? entry.size, !entry.isDirectory {
                Text(ByteCountFormatter.string(fromByteCount: Int64(bytes), countStyle: .file))
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
        .accessibilityIdentifier("files.entry")
    }

    private func load(offset: Int) async {
        guard let api = app.api else { return }
        do {
            let page = try await api.browse(path, offset: offset)
            resolved = page.path
            entries = offset == 0 ? page.entries : entries + page.entries
            next = page.next
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
        loaded = true
    }

    private func measure() async {
        guard let api = app.api else { return }
        usage = "Measuring…"
        do {
            let result = try await api.diskUsage(resolved ?? path)
            let size = ByteCountFormatter.string(fromByteCount: Int64(result.bytes), countStyle: .file)
            usage = "\(size) in \(result.entries.formatted()) items\(result.partial ? " (stopped early)" : "")"
        } catch {
            usage = "Couldn’t measure: \(error.localizedDescription)"
        }
    }
}

/// A file preview: text, image, PDF, or isolated HTML.
struct FileView: View {
    let conversation: ConversationModel
    let folder: String
    let name: String
    @Environment(AppModel.self) private var app
    @State private var content: FileContent?
    @State private var error: String?
    @State private var editing = false
    @State private var previewingHTML = false

    private var fullPath: String { BrowsePage(path: folder, entries: []).child(.init(name: name, kind: "file")) }
    private var inWorkspace: Bool {
        guard let workspace = conversation.thread.workspace, !workspace.isEmpty else { return false }
        return fullPath.hasPrefix(workspace.hasSuffix("/") ? workspace : workspace + "/")
    }

    var body: some View {
        Group {
            if let content {
                if content.isPDF {
                    PDFPreview(data: content.data)
                } else if content.isImage, let image = UIImage(data: content.data) {
                    ScrollView([.horizontal, .vertical]) {
                        Image(uiImage: image).resizable().scaledToFit().frame(maxWidth: .infinity)
                    }
                    .accessibilityLabel(name)
                } else if let text = content.text {
                    ScrollView {
                        if ["md", "markdown"].contains((name as NSString).pathExtension.lowercased()) {
                            MarkdownView(text: text).padding(Design.padding)
                        } else {
                            CodeBlock(code: text, language: (name as NSString).pathExtension).padding(Design.padding)
                        }
                    }
                } else {
                    ContentUnavailableView("No preview", systemImage: "doc",
                                           description: Text("Glass can’t show this kind of file."))
                }
            } else if let error {
                ContentUnavailableView("Can’t open this file", systemImage: "doc.badge.exclamationmark", description: Text(error))
            } else {
                ProgressView()
            }
        }
        .navigationTitle((name as NSString).lastPathComponent)
        .navigationBarTitleDisplayMode(.inline)
        .accessibilityIdentifier("file")
        .toolbar {
            if content?.isHTML == true {
                ToolbarItem(placement: .primaryAction) {
                    Button("Preview", systemImage: "play.rectangle") { previewingHTML = true }
                        .accessibilityIdentifier("file.preview")
                }
            }
            if inWorkspace, content?.text != nil {
                ToolbarItem(placement: .primaryAction) {
                    Button("Edit", systemImage: "pencil") { editing = true }
                        .accessibilityIdentifier("file.edit")
                }
            }
        }
        .sheet(isPresented: $editing, onDismiss: { Task { await load() } }) {
            FileEditorView(conversation: conversation, path: fullPath)
        }
        .sheet(isPresented: $previewingHTML) {
            if let content { ArtifactView(conversation: conversation, html: content.data, title: name) }
        }
        .task { await load() }
    }

    private func load() async {
        guard let api = app.api else { return }
        do {
            content = try await api.file(name, workspace: folder)
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}

struct PDFPreview: UIViewRepresentable {
    var data: Data

    func makeUIView(context: Context) -> PDFView {
        let view = PDFView()
        view.autoScales = true
        view.document = PDFDocument(data: data)
        return view
    }

    func updateUIView(_ view: PDFView, context: Context) {}
}

/// Edits a text file through the host's `file` command, guarded by the file's revision.
struct FileEditorView: View {
    let conversation: ConversationModel
    let path: String
    @Environment(AppModel.self) private var app
    @Environment(\.dismiss) private var dismiss
    @State private var text = ""
    @State private var saved = ""
    @State private var revision: String?
    @State private var writePath: String?
    @FocusState private var focused: Bool
    @State private var saving = false
    @State private var error: String?

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                if let error {
                    Text(error).font(.footnote).foregroundStyle(.red).padding(.horizontal).padding(.vertical, 6)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .accessibilityIdentifier("editor.error")
                }
                if !conversation.canWrite {
                    Text("Take control of the conversation to save.").font(.footnote).foregroundStyle(.secondary)
                        .padding(.horizontal).padding(.vertical, 6)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                TextEditor(text: $text)
                    .font(.system(.footnote, design: .monospaced))
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .disabled(revision == nil)
                    .focused($focused)
                    .accessibilityIdentifier("editor.text")
            }
            .navigationTitle((path as NSString).lastPathComponent)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { Task { await save() } }
                        .disabled(saving || text == saved || revision == nil || !conversation.canWrite)
                        .accessibilityIdentifier("editor.save")
                }
            }
            .interactiveDismissDisabled(text != saved)
            .task { await load() }
        }
    }

    private func load() async {
        guard let api = app.api else { return }
        do {
            let file = try await api.editableFile(path, thread: conversation.thread.id, sourceRef: conversation.thread.sourceRef)
            text = file.text
            saved = file.text
            revision = file.revision
            // The host answers with the workspace-relative path that `file write` accepts.
            writePath = file.path
            focused = conversation.canWrite
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func save() async {
        guard let revision else { return }
        // The host writes this snapshot; anything typed while it saves stays in the editor, unsaved.
        let submitted = text
        saving = true
        defer { saving = false }
        do {
            let result = try await conversation.composer.command("file", [
                "action": .string("write"), "path": .string(writePath ?? path), "expected": .string(revision),
                "text": .string(submitted), "confirmed": .bool(true),
            ], timeout: .seconds(60))
            saved = submitted
            if let next = result["body"]?["revision"]?.stringValue ?? result["revision"]?.stringValue {
                self.revision = next
            }
            if text == submitted { dismiss() }
        } catch {
            self.error = "Couldn’t save: \(error.localizedDescription)"
        }
    }
}
