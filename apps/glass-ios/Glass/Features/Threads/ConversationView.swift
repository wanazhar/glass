import GlassModels
import SwiftUI

struct ConversationView: View {
    @Bindable var conversation: ConversationModel
    var threads: ThreadsModel
    @Environment(\.scenePhase) private var scenePhase
    @State private var following = true

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
        .task { await conversation.start() }
        .onChange(of: scenePhase) { _, phase in
            // iOS suspends sockets in the background; resume from the last cursor on return.
            if phase == .active { Task { await conversation.start() } }
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
                if let notice = conversation.timeline.notice ?? conversation.error.map({ Timeline.Notice(text: $0, isError: true) }) {
                    HStack(alignment: .top, spacing: 10) {
                        Image(systemName: notice.isError ? "exclamationmark.triangle.fill" : "info.circle.fill")
                            .foregroundStyle(notice.isError ? .red : .accentColor)
                        Text(notice.text).font(.footnote).frame(maxWidth: .infinity, alignment: .leading)
                        Button("Dismiss", systemImage: "xmark") {
                            conversation.dismissNotice()
                            conversation.error = nil
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
                    if !following {
                        Button("Latest", systemImage: "arrow.down") {
                            following = true
                            withAnimation { proxy.scrollTo(Self.bottom, anchor: .bottom) }
                        }
                        .glassButtonStyle()
                        .accessibilityIdentifier("conversation.latest")
                    }
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 6)
        }
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
