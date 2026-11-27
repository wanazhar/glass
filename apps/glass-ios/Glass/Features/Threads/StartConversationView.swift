import SwiftUI

struct StartConversationView: View {
    let threads: ThreadsModel
    let menu: () -> Void
    let newChat: () -> Void
    @State private var draft = ""
    @State private var height: CGFloat = 0
    @State private var starting = false

    var body: some View {
        ScrollView {
            NewChatView { draft = $0 }
                .padding(Design.padding)
                .accessibilityIdentifier("conversation.empty")
        }
        .defaultScrollAnchor(.center, for: .alignment)
        .scrollDismissesKeyboard(.interactively)
        .softTopScrollEdge()
        .safeAreaBar(edge: .top, spacing: Design.gap) {
            ChatToolbar(menu: menu, newChat: newChat) {
                Text("Glass").font(Design.headline)
                    .accessibilityAddTraits(.isHeader)
            }
        }
        .safeAreaInset(edge: .bottom, spacing: Design.gap) {
            composer
                .padding(.horizontal, Design.padding)
                .padding(.bottom, 6)
                .frame(maxWidth: Design.readingWidth)
                .frame(maxWidth: .infinity)
                .hiddenUnderSidebar()
        }
        .toolbarVisibility(.hidden, for: .navigationBar)
        .background(Design.background.ignoresSafeArea())
    }

    private var composer: some View {
        GlassGroup(spacing: 12) {
            VStack(alignment: .leading, spacing: Design.small) {
                if let error = threads.error {
                    Text(error).font(.footnote).foregroundStyle(Design.failure)
                        .padding(.horizontal, 12)
                }
                Button { start(.model) } label: {
                    HStack(spacing: 5) {
                        Text("Model").font(.subheadline.weight(.semibold))
                        Image(systemName: "chevron.down").font(.caption2.weight(.semibold))
                    }
                    .foregroundStyle(Design.muted)
                    .frame(minHeight: Design.control)
                    .contentShape(.capsule)
                }
                .buttonStyle(.plain)
                .padding(.horizontal, 12)
                .accessibilityIdentifier("composer.model")
                HStack(alignment: .bottom, spacing: Design.gap) {
                    Menu {
                        Button("Photo Library", systemImage: "photo.on.rectangle") { start(.photos) }
                        Button("Files", systemImage: "folder") { start(.files) }
                        Button("Camera", systemImage: "camera") { start(.camera) }
                            .disabled(!UIImagePickerController.isSourceTypeAvailable(.camera))
                        Divider()
                        Button("Expand editor", systemImage: "arrow.up.left.and.arrow.down.right") { start(.editor) }
                    } label: {
                        ComposerCircle(icon: "plus")
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Attach")
                    .accessibilityIdentifier("composer.attach")

                    ComposerTextField(placeholder: "Message Glass", text: $draft, isEnabled: !starting,
                                      height: height, onHeight: { height = $0 }) {
                        start(.send)
                    }
                    .padding(.vertical, 11)

                    if draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        Button { start(.dictate) } label: { ComposerCircle(icon: "mic") }
                            .buttonStyle(.plain)
                            .accessibilityLabel("Dictate")
                            .accessibilityIdentifier("composer.dictate")
                    }
                    Button { start(.send) } label: {
                        ComposerCircle(icon: "arrow.up", primary: true)
                            .opacity(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? 0.65 : 1)
                    }
                    .buttonStyle(.plain)
                    .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    .accessibilityLabel("Send")
                    .accessibilityIdentifier("composer.send")
                }
            }
            .disabled(starting)
            .padding(Design.gap)
            .lensComposerCard()
        }
    }

    private func start(_ action: ComposerModel.InitialAction) {
        guard !starting, action != .send || !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        starting = true
        Task {
            await threads.newThread(draft: draft, action: action)
            starting = false
        }
    }
}
