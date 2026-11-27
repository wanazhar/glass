import GlassModels
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

/// Message entry, control handoff, attachments and stop, pinned under the conversation.
struct ComposerView: View {
    @Bindable var conversation: ConversationModel
    var onFieldFocus: () -> Void
    var chooseModel: () -> Void
    @State private var dictation = Dictation()
    @State private var photos: [PhotosPickerItem] = []
    @State private var choosingPhotos = false
    @State private var importingFiles = false
    @State private var takingPhoto = false
    @State private var showsControlPrompt = false
    @State private var fieldHeight: CGFloat = 0
    @State private var writingSince = Date.now
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    private var composer: ComposerModel { conversation.composer }
    private var canWrite: Bool { conversation.canWrite }
    private var active: Bool { conversation.timeline.isActive }

    var body: some View {
        Group {
            if canWrite {
                GlassGroup(spacing: 12) {
                    VStack(spacing: Design.small) {
                        if !composer.uploads.isEmpty { uploadList }
                        if !composer.context.isEmpty { contextChips }
                        if dictation.isListening { RecordingBar(dictation: dictation) }
                        if case .unavailable(let message) = dictation.state {
                            Text(message).font(.footnote).foregroundStyle(Design.muted)
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .accessibilityIdentifier("composer.dictation.message")
                        }
                        rail
                        HStack(alignment: .bottom, spacing: Design.gap) {
                            attachMenu
                            field
                            trailingButton
                        }
                    }
                    .padding(Design.gap)
                    .background(reduceTransparency ? Design.surface : .clear, in: .rect(cornerRadius: 28))
                    .glassEffect(reduceTransparency ? .identity : .regular.tint(Design.surface.opacity(0.72)),
                                 in: .rect(cornerRadius: 28))
                }
            } else if conversation.connectionMessage != nil {
                ComposerBanner(title: "Disconnected", icon: "wifi.slash",
                               detail: "Showing the last synced history. Reconnect to send.",
                               actionTitle: "Reconnect", action: { Task { await conversation.reconnect() } })
                .accessibilityIdentifier("composer.disconnected")
            } else if showsControlPrompt, conversation.needsControl {
                ComposerBanner(title: "Read-only on this device", icon: "lock",
                               detail: "Move typing here to reply. The conversation stays open on the other device.",
                               actionTitle: "Take over here", actionID: "composer.takeControl",
                               action: { Task { await composer.claimControl() } })
                .accessibilityIdentifier("composer.readOnly")
            } else {
                HStack {
                    ProgressView()
                    Text("Connecting to your host…").font(.footnote).foregroundStyle(Design.muted)
                    Spacer()
                }
                .padding(Design.padding)
            }
        }
        .onChange(of: active, initial: true) { _, active in
            if active { writingSince = .now }
        }
        .onChange(of: canWrite, initial: true) { _, writable in
            guard writable, let action = composer.initialAction else { return }
            composer.initialAction = nil
            switch action {
            case .send: Task { await composer.send(active: active, canWrite: canWrite) }
            case .model: chooseModel()
            case .photos: choosingPhotos = true
            case .files: importingFiles = true
            case .camera: takingPhoto = true
            case .dictate: beginDictation()
            case .editor: composer.showingEditor = true
            }
        }
        .photosPicker(isPresented: $choosingPhotos, selection: $photos, maxSelectionCount: UploadPolicy.filesPerBatch,
                      matching: .images, preferredItemEncoding: .current)
        .onChange(of: photos) { _, items in
            guard !items.isEmpty else { return }
            photos = []
            Task { await attachPhotos(items) }
        }
        .fileImporter(isPresented: $importingFiles, allowedContentTypes: AttachmentInput.fileTypes,
                      allowsMultipleSelection: true) { result in
            switch result {
                case .success(let urls): attachFiles(urls)
                case .failure(let error): composer.error = error.localizedDescription
            }
        }
        .sheet(isPresented: Bindable(composer).showingEditor) {
            DraftEditor(text: Bindable(composer).text) {
                composer.showingEditor = false
                Task { await composer.send(active: active, canWrite: canWrite) }
            }
        }
        .fullScreenCover(isPresented: $takingPhoto) {
            ComposerCamera { image in
                takingPhoto = false
                if let image {
                    composer.attach([AttachmentInput(name: "Camera.jpg", data: image.jpegData(compressionQuality: 0.9), isPhoto: true)], canWrite: canWrite)
                }
            }
            .ignoresSafeArea()
        }
        .sensoryFeedback(trigger: dictation.isListening) { _, listening in listening ? .start : .stop }
        .onChange(of: dictation.isListening) { _, listening in
            AccessibilityNotification.Announcement(listening ? "Recording. Speak now." : "Recording stopped.").post()
        }
        .onDisappear { dictation.stop(); composer.flushDraft() }
        .task(id: conversation.needsControl) {
            // Control often settles a moment after the stream opens; only ask if it stays with another client.
            showsControlPrompt = false
            guard conversation.needsControl else { return }
            try? await Task.sleep(for: .milliseconds(900))
            if !Task.isCancelled { showsControlPrompt = conversation.needsControl }
        }
    }

    private var rail: some View {
        HStack(spacing: 6) {
            Button(action: chooseModel) {
                HStack(spacing: 5) {
                    Text(composer.model ?? conversation.thread.model ?? "Model")
                        .font(.subheadline.weight(.semibold)).lineLimit(1)
                    Image(systemName: "chevron.down").font(.caption2.weight(.semibold))
                }
                .frame(minHeight: Design.control)
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("composer.model")
            Spacer(minLength: 4)
            if active {
                ActivityIndicator().tint(Design.accent).accessibilityHidden(true)
                TimelineView(.periodic(from: writingSince, by: 1)) { frame in
                    let elapsed = Duration.seconds(max(0, frame.date.timeIntervalSince(writingSince).rounded(.down)))
                    Text("Writing · \(workElapsed(elapsed))")
                        .font(.caption.weight(.medium))
                        .monospacedDigit()
                        .fixedSize(horizontal: true, vertical: false)
                }
                if !composer.trimmed.isEmpty { sendModeMenu }
            }
        }
        .foregroundStyle(Design.muted)
        .padding(.horizontal, 12)
    }

    /// While Glass works, a message either steers the current reply or waits for the next turn.
    /// One tap switches between the two; the line always says what Send will do.
    private var sendModeMenu: some View {
        Button {
            composer.mode = composer.mode == .steer ? .queue : .steer
        } label: {
            Image(systemName: "arrow.left.arrow.right")
                .font(.footnote.weight(.medium))
                .frame(width: Design.control, height: Design.control)
                .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Send during this reply")
        .accessibilityValue(composer.mode.title + ". " + composer.mode.detail)
        .accessibilityHint(composer.mode == .steer ? "Switches to Queue: send after this reply" : "Switches to Steer: change what Glass is doing now")
        .accessibilityIdentifier("composer.mode")
    }

    private var field: some View {
        ComposerTextField(placeholder: active ? "Steer this reply" : "Message Glass",
                          text: Bindable(composer).text, isEnabled: canWrite,
                          height: fieldHeight, onHeight: { fieldHeight = $0 },
                          onFocus: onFieldFocus,
                          dismissFor: conversation.timeline.interaction?.id,
                          onCommands: { composer.showingCommands = true }) {
            Task { await composer.send(active: active, canWrite: canWrite) }
        }
        .padding(.vertical, 11)
    }

    private var trailingButton: some View {
        HStack(spacing: 8) {
            if composer.trimmed.isEmpty && !active {
                Button {
                    beginDictation()
                } label: {
                    ComposerCircle(icon: dictation.isListening ? "mic.slash" : "mic")
                }
                .buttonStyle(.plain)
                .accessibilityLabel(dictation.isListening ? "Stop recording" : "Dictate")
                .accessibilityIdentifier("composer.dictate")
            }
            if !active || !composer.trimmed.isEmpty {
                Button {
                    dictation.stop()
                    Task { await composer.send(active: active, canWrite: canWrite) }
                } label: {
                    ComposerCircle(icon: active ? "arrow.turn.up.right" : "arrow.up", primary: !active)
                        .opacity(!active && composer.trimmed.isEmpty ? 0.65 : 1)
                }
                .buttonStyle(.plain)
                .disabled(composer.trimmed.isEmpty || composer.sending)
                .accessibilityLabel(active ? composer.mode.title : "Send")
                .accessibilityIdentifier("composer.send")
            }
            if active {
                Button {
                    UIImpactFeedbackGenerator(style: .light).impactOccurred()
                    Task { await composer.stop() }
                } label: {
                    ComposerCircle(icon: "stop.fill", primary: true)
                }
                .buttonStyle(.plain)
                .keyCommand(.stop)
                .accessibilityLabel("Stop")
                .accessibilityIdentifier("composer.stop")
            }
        }
    }

    private var attachMenu: some View {
        Menu {
            Button("Photo Library", systemImage: "photo.on.rectangle") { choosingPhotos = true }
                .accessibilityIdentifier("composer.attach.photos")
            Button("Files", systemImage: "folder") { importingFiles = true }
                .accessibilityIdentifier("composer.attach.files")
            Button("Camera", systemImage: "camera") { takingPhoto = true }
                .disabled(!UIImagePickerController.isSourceTypeAvailable(.camera))
            Divider()
            Button("Expand Editor", systemImage: "arrow.up.left.and.arrow.down.right") { composer.showingEditor = true }
                .accessibilityIdentifier("composer.expand")
        } label: {
            ComposerCircle(icon: "plus")
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Attach")
        .disabled(!canWrite)
        .accessibilityIdentifier("composer.attach")
    }

    private func beginDictation() {
        let base = composer.text.isEmpty ? "" : composer.text + " "
        Task { await dictation.toggle { composer.text = base + $0 } }
    }

    private var uploadList: some View {
        VStack(spacing: 6) {
            ForEach(composer.uploads) { upload in
                VStack(alignment: .leading, spacing: 4) {
                    HStack(spacing: 8) {
                        Text(upload.name).font(.footnote).lineLimit(1)
                        if upload.failure != nil {
                            Spacer()
                            Button { composer.dismissUpload(upload) } label: {
                                Image(systemName: "xmark")
                                    .font(Design.footnote)
                                    .frame(width: Design.control, height: Design.control)
                                    .contentShape(Rectangle())
                            }
                            .accessibilityLabel("Dismiss upload")
                        } else {
                            ProgressView(value: upload.fraction).frame(maxWidth: .infinity)
                        }
                    }
                    if let failure = upload.failure {
                        Text(verbatim: failure)
                            .font(.footnote)
                            .foregroundStyle(.red)
                            .fixedSize(horizontal: false, vertical: true)
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
        .padding(12)
        .background(Design.inset, in: .rect(cornerRadius: 12))
    }

    private var contextChips: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            GlassGroup(spacing: 6) {
            HStack(spacing: 6) {
                ForEach(composer.context) { attachment in
                    HStack(spacing: 4) {
                        Image(systemName: attachment.isImage ? "photo" : "doc.text").accessibilityHidden(true)
                        Text(attachment.label).lineLimit(1)
                        Button { Task { await composer.remove(attachment) } } label: {
                            Image(systemName: "xmark")
                                .frame(width: Design.control, height: Design.control)
                                .contentShape(Rectangle())
                        }
                        .accessibilityLabel("Remove \(attachment.label)")
                        .disabled(!canWrite)
                    }
                    .font(.caption)
                    .padding(.horizontal, 10)
                    .padding(.vertical, Design.small)
                    .background(Design.inset, in: .capsule)
                    .accessibilityElement(children: .combine)
                    .accessibilityIdentifier("composer.context")
                }
            }
            }
            .padding(.vertical, 2)
        }
        .scrollClipDisabled()
        .accessibilityIdentifier("composer.contextChips")
    }

    private func attachPhotos(_ items: [PhotosPickerItem]) async {
        var files: [AttachmentInput] = []
        for (index, item) in items.enumerated() {
            let data = try? await item.loadTransferable(type: Data.self)
            let ext = item.supportedContentTypes.first?.preferredFilenameExtension ?? "jpg"
            files.append(AttachmentInput(name: "Photo \(index + 1).\(ext)", data: data, isPhoto: true))
        }
        composer.attach(files, canWrite: canWrite)
    }

    private func attachFiles(_ urls: [URL]) {
        let files = urls.prefix(UploadPolicy.filesPerBatch + 1).map { url in
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            return AttachmentInput(name: url.lastPathComponent, data: try? Data(contentsOf: url, options: .mappedIfSafe))
        }
        composer.attach(files, canWrite: canWrite)
    }
}

struct ComposerCircle: View {
    let icon: String
    var primary = false

    var body: some View {
        Image(systemName: icon)
            .font(.system(size: 17, weight: .semibold))
            .foregroundStyle(primary ? Design.onAccent : Design.ink)
            .frame(width: Design.control, height: Design.control)
            .background(primary ? Design.accent : Design.inset, in: .circle)
            .contentShape(.circle)
    }
}

struct ComposerBanner: View {
    let title: String
    let icon: String
    let detail: String
    let actionTitle: String
    var actionID = ""
    let action: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: Design.gap) {
            Label(title, systemImage: icon)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(Design.warning)
            Text(detail).font(.footnote).foregroundStyle(Design.muted)
                .fixedSize(horizontal: false, vertical: true)
            AccentAction(title: actionTitle, action: action)
                .accessibilityIdentifier(actionID)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .background(Design.surface, in: .rect(cornerRadius: 16))
        .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(Design.line))
        .accessibilityElement(children: .contain)
    }
}

private struct ComposerCamera: UIViewControllerRepresentable {
    let complete: (UIImage?) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(complete: complete) }

    func makeUIViewController(context: Context) -> UIImagePickerController {
        let camera = UIImagePickerController()
        camera.sourceType = .camera
        camera.delegate = context.coordinator
        return camera
    }

    func updateUIViewController(_ controller: UIImagePickerController, context: Context) {}

    final class Coordinator: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
        let complete: (UIImage?) -> Void

        init(complete: @escaping (UIImage?) -> Void) { self.complete = complete }

        func imagePickerController(_ picker: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
            complete(info[.originalImage] as? UIImage)
        }

        func imagePickerControllerDidCancel(_ picker: UIImagePickerController) { complete(nil) }
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

    private var filtered: [ComposerModel.Choice] {
        Array((choices ?? []).filter {
            filter.isEmpty || $0.label.localizedCaseInsensitiveContains(filter)
                || $0.id.localizedCaseInsensitiveContains(filter)
                || $0.provider.localizedCaseInsensitiveContains(filter)
        }.prefix(100))
    }

    var body: some View {
        NavigationStack {
            List {
                if let error { Section { Text(error).foregroundStyle(Design.failure) } }
                if let choices {
                    if picker == .model {
                        let groups = Dictionary(grouping: filtered, by: \.provider)
                        ForEach(groups.keys.sorted(), id: \.self) { provider in
                            Section {
                                ForEach(groups[provider] ?? []) { choiceRow($0) }
                            } header: {
                                Text(provider).font(.subheadline.weight(.semibold)).textCase(nil)
                            }
                        }
                    } else {
                        Section { ForEach(filtered) { choiceRow($0) } }
                    }
                    if choices.isEmpty { Text("No choices are available.").foregroundStyle(Design.muted) }
                    else if filtered.isEmpty { Text("No matches.").foregroundStyle(Design.muted) }
                } else if error == nil {
                    ProgressView().frame(maxWidth: .infinity)
                }
                if picker == .model, choices != nil {
                    Section {
                        DisclosureGroup("Custom model ID") {
                            TextField("provider/model", text: $custom)
                                .font(.subheadline.monospaced())
                                .autocorrectionDisabled().textInputAutocapitalization(.never)
                            Button("Use model") { choose(custom.trimmingCharacters(in: .whitespacesAndNewlines)) }
                                .disabled(custom.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                        }
                        .tint(Design.accent)
                    }
                }
            }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
            .background(Design.background)
            .searchable(text: $filter, placement: .navigationBarDrawer(displayMode: .always),
                        prompt: picker == .model ? "Find a model" : "Find a profile")
            .navigationTitle(picker == .model ? "Choose model" : picker.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } } }
            .task {
                do { choices = try await composer.choices(for: picker) } catch { self.error = error.localizedDescription }
            }
        }
        .presentationDetents(picker == .model ? [.large] : [.medium, .large])
    }

    private func choiceRow(_ choice: ComposerModel.Choice) -> some View {
        let selected = picker == .model && composer.model == choice.id
        return Button { choose(choice.id) } label: {
            HStack {
                Text(choice.label).font(.subheadline.weight(selected ? .semibold : .regular))
                Spacer(minLength: 8)
                if selected { Image(systemName: "checkmark.circle.fill").foregroundStyle(Design.accent) }
            }
            .foregroundStyle(Design.ink)
            .frame(minHeight: Design.control)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .listRowBackground(selected ? Design.inset : Design.surface)
        .accessibilityAddTraits(selected ? .isSelected : [])
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

/// Shown while dictation records: a red dot, elapsed time, a live input level and Stop.
struct RecordingBar: View {
    var dictation: Dictation
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        HStack(spacing: 10) {
            Circle().fill(.red).frame(width: 10, height: 10)
                .opacity(reduceMotion ? 1 : Double(0.5 + dictation.level / 2))
                .accessibilityHidden(true)
            Text("Recording").font(.subheadline.weight(.semibold))
            if let start = dictation.startedAt {
                TimelineView(.periodic(from: start, by: 1)) { context in
                    Text(Duration.seconds(max(0, context.date.timeIntervalSince(start).rounded(.down))),
                         format: .time(pattern: .minuteSecond))
                        .font(.subheadline.monospacedDigit())
                        .foregroundStyle(.secondary)
                }
                .accessibilityIdentifier("composer.recording.elapsed")
            }
            LevelMeter(level: dictation.level)
                .frame(maxWidth: .infinity, alignment: .leading)
            Button("Stop", systemImage: "stop.fill") { dictation.stop() }
                .tint(.red)
                .buttonStyle(.borderedProminent)
                .buttonBorderShape(.capsule)
                .accessibilityIdentifier("composer.recording.stop")
        }
        .padding(.leading, 14)
        .padding(.trailing, 6)
        .padding(.vertical, 6)
        .floatingGlass()
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Recording")
        .accessibilityIdentifier("composer.recording")
    }
}

/// Bars that follow the microphone input level.
struct LevelMeter: View {
    var level: Float
    private let weights: [Float] = [0.55, 0.8, 1, 0.8, 0.55, 0.7, 0.9]

    var body: some View {
        HStack(spacing: 3) {
            ForEach(weights.indices, id: \.self) { index in
                Capsule()
                    .fill(Color.red.opacity(0.8))
                    .frame(width: 3, height: 4 + CGFloat(level * weights[index]) * 18)
            }
        }
        .frame(height: 22)
        .accessibilityHidden(true)
    }
}
