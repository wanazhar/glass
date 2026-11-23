import GlassModels
import SwiftUI

/// Teammates working in this conversation, kept current by the host's roster updates.
struct TeammatesView: View {
    let conversation: ConversationModel
    @Environment(AppModel.self) private var app
    @State private var teammates: [Teammate]?
    @State private var error: String?
    @State private var offline = false

    var body: some View {
        List {
            if let teammates {
                if teammates.isEmpty {
                    ContentUnavailableView("No teammates", systemImage: "person.2",
                                           description: Text("Teammates appear here when the host starts them for this conversation."))
                } else {
                    ForEach(teammates) { teammate in
                        NavigationLink { TranscriptView(conversation: conversation, teammate: teammate) } label: {
                            VStack(alignment: .leading, spacing: 3) {
                                HStack {
                                    Text(teammate.title).font(.body.weight(.medium))
                                    Spacer()
                                    Text(teammate.statusTitle).font(.caption).foregroundStyle(.secondary)
                                }
                                if let task = teammate.task, !task.isEmpty {
                                    Text(task).font(.footnote).foregroundStyle(.secondary).lineLimit(2)
                                }
                            }
                            .padding(.leading, CGFloat(min(teammate.depth ?? 0, 4)) * 12)
                        }
                        .accessibilityIdentifier("teammates.row")
                    }
                }
            } else if let error {
                ContentUnavailableView("Can’t load teammates", systemImage: "exclamationmark.triangle", description: Text(error))
            }
            if offline {
                Text("Live updates stopped. Pull to refresh.").font(.footnote).foregroundStyle(.secondary)
            }
        }
        .overlay { if teammates == nil && error == nil { ProgressView() } }
        .navigationTitle("Teammates")
        .task { await watch() }
        .refreshable { await load() }
    }

    private func load() async {
        guard let api = app.api else { return }
        do {
            teammates = try await api.teammates(thread: conversation.thread.id, sourceRef: conversation.thread.sourceRef)
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }

    private func watch() async {
        await load()
        guard let api = app.api else { return }
        do {
            let signals = try await api.teammateSignals(thread: conversation.thread.id, sourceRef: conversation.thread.sourceRef)
            for try await signal in signals {
                switch signal {
                case .revision: await load()
                case .offline: offline = true
                }
            }
        } catch {
            if !Task.isCancelled { offline = true }
        }
    }
}

struct TranscriptView: View {
    let conversation: ConversationModel
    let teammate: Teammate
    @Environment(AppModel.self) private var app
    @State private var parts: [String] = []
    @State private var before: Int?
    @State private var hasMore = false
    @State private var loading = false
    @State private var error: String?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if hasMore {
                    Button(loading ? "Loading…" : "Show earlier") { Task { await load(earlier: true) } }
                        .disabled(loading)
                        .accessibilityIdentifier("transcript.earlier")
                }
                ForEach(Array(parts.enumerated()), id: \.offset) { _, text in
                    MarkdownView(text: text)
                }
                if let error { Text(error).foregroundStyle(.secondary) }
                if parts.isEmpty && error == nil && !loading {
                    Text("Nothing recorded yet.").foregroundStyle(.secondary)
                }
            }
            .padding()
            .frame(maxWidth: 760, alignment: .leading)
        }
        .overlay { if loading && parts.isEmpty { ProgressView() } }
        .navigationTitle(teammate.title)
        .navigationBarTitleDisplayMode(.inline)
        .accessibilityIdentifier("transcript")
        .task { await load(earlier: false) }
        .refreshable { await load(earlier: false) }
    }

    private func load(earlier: Bool) async {
        guard let api = app.api, !loading else { return }
        loading = true
        defer { loading = false }
        do {
            let page = try await api.teammateTranscript(teammate.id, thread: conversation.thread.id,
                                                        sourceRef: conversation.thread.sourceRef, before: earlier ? before : nil)
            if earlier { parts.insert(page.text, at: 0) } else { parts = [page.text] }
            before = page.before
            hasMore = page.hasMore
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}
