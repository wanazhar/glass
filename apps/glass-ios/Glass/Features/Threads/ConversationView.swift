import GlassModels
import SwiftUI

struct ConversationView: View {
    @Bindable var conversation: ConversationModel
    var threads: ThreadsModel
    @Environment(\.scenePhase) private var scenePhase
    @State private var following = true
    @State private var picker: ComposerModel.Picker?
    @State private var showingChecklist = false
    @Environment(AppModel.self) private var app

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 16) {
                    if conversation.timeline.hasOlder {
                        Button {
                            Task { await conversation.loadOlder() }
                        } label: {
                            if conversation.loadingOlder { ProgressView() } else { Label("Earlier messages", systemImage: "arrow.up.circle") }
                        }
                        .glassButtonStyle()
                        .frame(maxWidth: .infinity)
                        .accessibilityIdentifier("conversation.older")
                    }
                    if conversation.loadedHistory, conversation.timeline.isEmpty {
                        emptyState
                    }
                    ForEach(conversation.timeline.entries) { entry in
                        TimelineRow(entry: entry).id(entry.id)
                    }
                    if let interaction = conversation.timeline.interaction {
                        InteractionView(interaction: interaction, composer: conversation.composer,
                                        canWrite: conversation.canWrite, secretsAllowed: secretsBlocked)
                            .id(interaction.id)
                    }
                    Color.clear.frame(height: 1).id(Self.bottom)
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
                .frame(maxWidth: 760)
                .frame(maxWidth: .infinity)
            }
            .defaultScrollAnchor(.bottom)
            .onScrollGeometryChange(for: Bool.self) { geometry in
                geometry.contentSize.height <= geometry.containerSize.height
                    || geometry.visibleRect.maxY >= geometry.contentSize.height - 80
            } action: { _, atBottom in
                following = atBottom
            }
            .onChange(of: conversation.timeline.interaction) {
                guard following else { return }
                proxy.scrollTo(Self.bottom, anchor: .bottom)
            }
            .onChange(of: conversation.timeline.entries) {
                guard following else { return }
                proxy.scrollTo(Self.bottom, anchor: .bottom)
            }
            .safeAreaInset(edge: .bottom) {
                bottomBar(proxy)
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("conversation")
        .accessibilityValue("\(conversation.timeline.entries.count) entries\(conversation.timeline.hasOlder ? ", earlier available" : "")")
        .navigationTitle(conversation.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .principal) {
                VStack(spacing: 1) {
                    Text(conversation.title).font(.headline).lineLimit(1)
                    if let origin = conversation.originLabel {
                        Text(origin).font(.caption2).foregroundStyle(.secondary).accessibilityIdentifier("conversation.origin")
                    }
                }
            }
            ToolbarItem(placement: .primaryAction) {
                Menu {
                    if let workspace = conversation.thread.workspace, !workspace.isEmpty {
                        Section(workspace) {}
                    }
                    Button("Model…", systemImage: "cpu") { picker = .model }
                        .disabled(!conversation.canWrite)
                        .accessibilityIdentifier("conversation.model")
                    Button("Execution Profile…", systemImage: "slider.horizontal.3") { picker = .profiles }
                        .disabled(!conversation.canWrite)
                        .accessibilityIdentifier("conversation.profile")
                    if !conversation.composer.checklist.isEmpty {
                        Button("Checklist", systemImage: "checklist") { showingChecklist = true }
                    }
                    if conversation.canWrite {
                        Button("Release Control", systemImage: "hand.raised") { Task { await conversation.composer.releaseControl() } }
                            .accessibilityIdentifier("conversation.releaseControl")
                    }
                    Button("Close conversation", systemImage: "xmark.circle") {
                        Task { await threads.close(conversation.thread.id) }
                    }
                    .accessibilityIdentifier("conversation.close")
                } label: {
                    Label("More", systemImage: "ellipsis")
                }
                .accessibilityIdentifier("conversation.menu")
            }
        }
        .sheet(item: $picker) { ChoicePicker(picker: $0, composer: conversation.composer) }
        .sheet(isPresented: $showingChecklist) { ChecklistSheet(tasks: conversation.composer.checklist) }
        .environment(\.attachmentLoader) { [composer = conversation.composer] id in await composer.attachment(id) }
        .task { await conversation.start() }
        .onChange(of: scenePhase) { _, phase in
            // iOS suspends sockets in the background; resume from the last cursor on return.
            if phase == .active { Task { await conversation.start() } }
            if phase != .active { conversation.composer.flushDraft() }
        }
    }

    private static let bottom = "timeline.bottom"

    private var emptyState: some View {
        Text("No messages yet. Messages sent from the web UI or the host appear here as they arrive.")
            .font(.footnote)
            .foregroundStyle(.secondary)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.top, 24)
            .accessibilityIdentifier("conversation.empty")
    }

    @ViewBuilder
    private func bottomBar(_ proxy: ScrollViewProxy) -> some View {
        GlassGroup(spacing: 8) {
            VStack(spacing: 8) {
                if let notice = conversation.timeline.notice
                    ?? (conversation.error ?? conversation.composer.error).map({ Timeline.Notice(text: $0, isError: true) })
                    ?? conversation.composer.confirmation.map({ Timeline.Notice(text: $0, isError: false) }) {
                    HStack(alignment: .top, spacing: 10) {
                        Image(systemName: notice.isError ? "exclamationmark.triangle.fill" : "info.circle.fill")
                            .foregroundStyle(notice.isError ? .red : .accentColor)
                        Text(notice.text).font(.footnote).frame(maxWidth: .infinity, alignment: .leading)
                        Button("Dismiss", systemImage: "xmark") {
                            conversation.dismissNotice()
                            conversation.error = nil
                            conversation.composer.error = nil
                            conversation.composer.confirmation = nil
                        }
                        .labelStyle(.iconOnly)
                        .font(.footnote)
                    }
                    .padding(12)
                    .background(Color(.secondarySystemGroupedBackground), in: .rect(cornerRadius: 12, style: .continuous))
                    .accessibilityElement(children: .combine)
                    .accessibilityIdentifier("conversation.notice")
                }
                HStack(spacing: 8) {
                    statusPill
                    if let activity = conversation.timeline.activity {
                        HStack(spacing: 6) {
                            ProgressView().controlSize(.mini)
                            Text(activity).font(.caption.weight(.medium))
                        }
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .floatingGlass()
                        .accessibilityIdentifier("conversation.activity")
                    }
                    Spacer()
                    if let tasks = Optional(conversation.composer.checklist), !tasks.isEmpty {
                        Button("\(tasks.filter(\.isDone).count) of \(tasks.count)", systemImage: "checklist") { showingChecklist = true }
                            .font(.caption)
                            .glassButtonStyle()
                            .accessibilityIdentifier("conversation.checklist")
                    }
                    if !following {
                        Button("Latest", systemImage: "arrow.down") {
                            following = true
                            withAnimation { proxy.scrollTo(Self.bottom, anchor: .bottom) }
                        }
                        .glassButtonStyle()
                        .accessibilityIdentifier("conversation.latest")
                    }
                }
                ComposerView(conversation: conversation)
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 6)
            .frame(maxWidth: 760)
        }
    }

    private var secretsBlocked: String? {
        guard app.endpoint?.isSecure == true else { return "Secrets are only sent over HTTPS. Use the Tailscale HTTPS link for this host." }
        guard app.secretEntryUnlocked else { return "Secret entry is locked. Pair again with a secret-entry link from the host to enable it." }
        return nil
    }

    private var statusPill: some View {
        let (title, tint): (String, Color) = switch conversation.connection {
        case .live: ("Live", .green)
        case .connecting, .idle: ("Connecting", .secondary)
        case .reconnecting: ("Reconnecting", .orange)
        case .readOnly: ("Read-only", .secondary)
        }
        return StatusLabel(title: title, color: tint)
            .font(.caption)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .floatingGlass()
            .accessibilityElement(children: .combine)
            .accessibilityIdentifier("conversation.connection")
            .accessibilityValue("\(title), \(conversation.resets) resets")
    }
}
