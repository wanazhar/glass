import GlassModels
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

/// Message entry, control handoff, attachments and stop, pinned under the conversation.
struct ComposerView: View {
    @Bindable var conversation: ConversationModel
    @State private var dictation = Dictation()
    @State private var photos: [PhotosPickerItem] = []
    @State private var choosingPhotos = false
    @State private var importingFiles = false

    private var composer: ComposerModel { conversation.composer }
    private var canWrite: Bool { conversation.canWrite }
    private var active: Bool { conversation.timeline.isActive }

    var body: some View {
        VStack(spacing: 8) {
            if conversation.connection == .live, !canWrite {
                controlPrompt
            }
            if !composer.uploads.isEmpty { uploadList }
            if !composer.context.isEmpty { contextChips }
            if active, canWrite {
                Picker("While working", selection: Bindable(composer).mode) {
                    ForEach(ComposerModel.SendMode.allCases) { Text($0.title).tag($0) }
                }
                .pickerStyle(.segmented)
                .accessibilityIdentifier("composer.mode")
            }
            HStack(alignment: .bottom, spacing: 8) {
                attachMenu
                field
                trailingButton
            }
            if case .unavailable(let message) = dictation.state {
                Text(message).font(.footnote).foregroundStyle(.secondary).frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityIdentifier("composer.dictation.message")
            }
        }
        .photosPicker(isPresented: $choosingPhotos, selection: $photos, maxSelectionCount: UploadPolicy.filesPerBatch, matching: .images)
        .onChange(of: photos) { _, items in
            guard !items.isEmpty else { return }
            photos = []
            Task { await attachPhotos(items) }
        }
        .fileImporter(isPresented: $importingFiles, allowedContentTypes: [.image, .plainText, .utf8PlainText, .sourceCode, .json, .text, .data],
                      allowsMultipleSelection: true) { result in
            if case .success(let urls) = result { attachFiles(urls) }
        }
        .sheet(isPresented: Bindable(composer).showingEditor) {
            DraftEditor(text: Bindable(composer).text) {
                composer.showingEditor = false
                Task { await composer.send(active: active, canWrite: canWrite) }
            }
        }
        .onDisappear { dictation.stop(); composer.flushDraft() }
    }

    private var controlPrompt: some View {
        HStack(spacing: 10) {
            Text("Another client may be typing. Take control to send messages from this iPhone.")
                .font(.footnote)
                .foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, alignment: .leading)
            Button("Take Control") { Task { await composer.claimControl() } }
                .glassButtonStyle(prominent: true)
                .accessibilityIdentifier("composer.takeControl")
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("composer.readOnly")
    }

    private var field: some View {
        ComposerTextField(placeholder: canWrite ? (active ? (composer.mode == .steer ? "Steer this turn" : "Queue a message") : "Message") : "Read-only",
                          text: Bindable(composer).text, isEnabled: canWrite,
                          dismissFor: conversation.timeline.interaction?.id,
                          onCommands: { composer.showingCommands = true }) {
            Task { await composer.send(active: active, canWrite: canWrite) }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
        .background(Color(.secondarySystemBackground), in: .rect(cornerRadius: 20, style: .continuous))
    }

    @ViewBuilder
    private var trailingButton: some View {
        GlassGroup(spacing: 8) {
            HStack(spacing: 8) {
                if composer.trimmed.isEmpty {
                    Button(dictation.isListening ? "Stop dictation" : "Dictate", systemImage: dictation.isListening ? "waveform" : "mic") {
                        let base = composer.text.isEmpty ? "" : composer.text + " "
                        Task { await dictation.toggle { composer.text = base + $0 } }
                    }
                    .labelStyle(.iconOnly)
                    .glassButtonStyle()
                    .disabled(!canWrite)
                    .accessibilityIdentifier("composer.dictate")
                }
                if active, composer.trimmed.isEmpty {
                    Button("Stop", systemImage: "stop.fill") { Task { await composer.stop() } }
                        .labelStyle(.iconOnly)
                        .glassButtonStyle(prominent: true)
                        .disabled(!canWrite)
                        .accessibilityIdentifier("composer.stop")
                } else {
                    Button("Send", systemImage: "arrow.up") {
                        dictation.stop()
                        Task { await composer.send(active: active, canWrite: canWrite) }
                    }
                    .labelStyle(.iconOnly)
                    .glassButtonStyle(prominent: true)
                    .disabled(!canWrite || composer.trimmed.isEmpty || composer.sending)
                    .accessibilityIdentifier("composer.send")
                }
            }
        }
    }

    private var attachMenu: some View {
        Menu {
            Button("Photo Library", systemImage: "photo.on.rectangle") { choosingPhotos = true }
                .accessibilityIdentifier("composer.attach.photos")
            Button("Files", systemImage: "folder") { importingFiles = true }
                .accessibilityIdentifier("composer.attach.files")
            Divider()
            Button("Expand Editor", systemImage: "arrow.up.left.and.arrow.down.right") { composer.showingEditor = true }
                .accessibilityIdentifier("composer.expand")
        } label: {
            Label("Attach", systemImage: "plus")
                .labelStyle(.iconOnly)
                .frame(minWidth: 22, minHeight: 22)
        }
        .glassButtonStyle()
        .disabled(!canWrite)
        .accessibilityIdentifier("composer.attach")
    }

    private var uploadList: some View {
        VStack(spacing: 6) {
            ForEach(composer.uploads) { upload in
                HStack(spacing: 8) {
                    Text(upload.name).font(.footnote).lineLimit(1)
                    if let failure = upload.failure {
                        Text(failure).font(.caption).foregroundStyle(.red).lineLimit(2)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        Button("Dismiss", systemImage: "xmark") { composer.dismissUpload(upload) }.labelStyle(.iconOnly).font(.footnote)
                    } else {
                        ProgressView(value: upload.fraction).frame(maxWidth: .infinity)
                    }
                }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("composer.upload")
            }
            if composer.uploads.contains(where: { $0.failure == nil && $0.fraction < 1 }) {
                Button("Cancel uploads", role: .cancel) { composer.cancelUploads() }.font(.footnote)
                    .frame(maxWidth: .infinity, alignment: .trailing)
            }
        }
    }

    private var contextChips: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(composer.context) { attachment in
                    HStack(spacing: 4) {
                        Image(systemName: attachment.isImage ? "photo" : "doc.text").accessibilityHidden(true)
                        Text(attachment.label).lineLimit(1)
                        Button("Remove \(attachment.label)", systemImage: "xmark") { Task { await composer.remove(attachment) } }
                            .labelStyle(.iconOnly)
                            .disabled(!canWrite)
                    }
                    .font(.caption)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .background(Color(.tertiarySystemFill), in: .capsule)
                    .accessibilityElement(children: .combine)
                    .accessibilityIdentifier("composer.context")
                }
            }
        }
        .accessibilityIdentifier("composer.contextChips")
    }

    private func attachPhotos(_ items: [PhotosPickerItem]) async {
        var files: [(name: String, data: Data)] = []
        for (index, item) in items.enumerated() {
            guard let data = try? await item.loadTransferable(type: Data.self) else { continue }
            let ext = item.supportedContentTypes.first?.preferredFilenameExtension ?? "jpg"
            files.append(("Photo \(index + 1).\(ext)", data))
        }
        composer.attach(files, canWrite: canWrite)
    }

    private func attachFiles(_ urls: [URL]) {
        let files: [(name: String, data: Data)] = urls.prefix(UploadPolicy.filesPerBatch + 1).compactMap { url in
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            return (try? Data(contentsOf: url, options: .mappedIfSafe)).map { (url.lastPathComponent, $0) }
        }
        composer.attach(files, canWrite: canWrite)
    }
}

/// A full-screen editor for long drafts.
private struct DraftEditor: View {
    @Binding var text: String
    var send: () -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            TextEditor(text: $text)
                .padding(.horizontal, 12)
                .accessibilityIdentifier("composer.editor")
                .navigationTitle("Draft")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } }
                    ToolbarItem(placement: .confirmationAction) {
                        Button("Send", action: send).disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    }
                }
        }
    }
}

/// Model and execution-profile choices, from the host's `model` and `profiles` commands.
struct ChoicePicker: View {
    var picker: ComposerModel.Picker
    var composer: ComposerModel
    @Environment(\.dismiss) private var dismiss
    @State private var choices: [ComposerModel.Choice]?
    @State private var filter = ""
    @State private var custom = ""
    @State private var error: String?

    var body: some View {
        NavigationStack {
            List {
                if let error { Section { Text(error).foregroundStyle(.red) } }
                if let choices {
                    Section {
                        ForEach(choices.filter { filter.isEmpty || $0.label.localizedCaseInsensitiveContains(filter) || $0.id.localizedCaseInsensitiveContains(filter) }.prefix(100)) { choice in
                            Button { choose(choice.id) } label: {
                                HStack {
                                    VStack(alignment: .leading) {
                                        Text(choice.label)
                                        if choice.label != choice.id { Text(choice.id).font(.caption).foregroundStyle(.secondary) }
                                    }
                                    Spacer()
                                    if picker == .model, composer.model == choice.id { Image(systemName: "checkmark").foregroundStyle(.tint) }
                                }
                            }
                            .tint(.primary)
                        }
                    }
                    if choices.isEmpty { Text("The host returned no choices.").foregroundStyle(.secondary) }
                } else if error == nil {
                    ProgressView().frame(maxWidth: .infinity)
                }
                if picker == .model {
                    Section("Custom model ID") {
                        TextField("Model ID", text: $custom).autocorrectionDisabled().textInputAutocapitalization(.never)
                        Button("Use Model") { choose(custom.trimmingCharacters(in: .whitespaces)) }
                            .disabled(custom.trimmingCharacters(in: .whitespaces).isEmpty)
                    }
                }
            }
            .searchable(text: $filter, prompt: "Filter")
            .navigationTitle(picker.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
            .task {
                do { choices = try await composer.choices(for: picker) } catch { self.error = error.localizedDescription }
            }
        }
    }

    private func choose(_ id: String) {
        Task {
            do {
                try await composer.choose(id, for: picker)
                dismiss()
            } catch {
                self.error = error.localizedDescription
            }
        }
    }
}

/// The agent's task board from the `todo` command.
struct ChecklistSheet: View {
    var tasks: [ChecklistTask]
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List(tasks) { task in
                Label {
                    Text(task.title).strikethrough(task.isAbandoned).foregroundStyle(task.isDone || task.isAbandoned ? .secondary : .primary)
                } icon: {
                    Image(systemName: task.isDone ? "checkmark.circle.fill" : task.status == "in_progress" ? "circle.dotted" : task.isAbandoned ? "xmark.circle" : "circle")
                        .foregroundStyle(task.isDone ? Color.green : .secondary)
                }
                .accessibilityValue(task.status.replacingOccurrences(of: "_", with: " "))
            }
            .navigationTitle("Checklist")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
        }
    }
}
