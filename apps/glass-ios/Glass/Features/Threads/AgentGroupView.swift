import GlassModels
import SwiftUI

extension Teammate {
    var isWorking: Bool { status == "working" }
    var isDone: Bool { status == "idle" && (terminalEvents ?? 0) > 0 || status == "completed" }
    var isFailed: Bool { status == "blocked" || status == "failed" }
    var agentStatusTitle: String {
        if isWorking { return "Working" }
        if isDone { return "Done" }
        if isFailed { return "Failed" }
        return statusTitle
    }
    var friendlyModel: String? { model.map { ModelName.friendly($0) ?? $0 } }
    var startDate: Date? {
        guard let start = startedAt ?? (isWorking ? updated : nil), start > 0 else { return nil }
        return Date(timeIntervalSince1970: start)
    }

    func elapsed(at date: Date) -> Duration? {
        guard let startDate else { return nil }
        let end = isWorking ? date : updated.map(Date.init(timeIntervalSince1970:)) ?? date
        return .seconds(max(0, Int(end.timeIntervalSince(startDate))))
    }
}

struct AgentGroupView: View {
    let entry: TimelineEntry
    let conversation: ConversationModel
    var open: (Teammate) -> Void
    @State private var expanded = false
    @State private var error: String?
    @Environment(\.sidebarOpen) private var sidebarOpen
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    private var glass: Glass { sidebarOpen || reduceTransparency ? .identity : .regular.interactive() }

    private var agents: [Teammate] {
        conversation.agents.filter { $0.parentCallID == entry.id && $0.parentID == nil }
    }

    var body: some View {
        if case .tool(_, let name, _, let output, let failed) = entry,
           !agents.isEmpty || name == "run_subagents" || failed != false {
            TimelineView(.periodic(from: .now, by: 1)) { time in
                VStack(spacing: Design.gap) {
                    Button {
                        expanded.toggle()
                    } label: {
                        HStack(spacing: Design.gap) {
                            Image(systemName: "person.2").accessibilityHidden(true)
                            if agents.isEmpty {
                                Text(failed == true ? "Delegation failed" : "Agent delegation")
                            } else {
                                ViewThatFits(in: .horizontal) {
                                    HStack(spacing: Design.gap) {
                                        summary.fixedSize()
                                        if let elapsed = longest(at: time.date) {
                                            Text(workElapsed(elapsed))
                                                .font(.caption).monospacedDigit().foregroundStyle(Design.muted)
                                                .accessibilityLabel("Longest: \(workElapsed(elapsed))")
                                                .fixedSize()
                                        }
                                    }
                                    summary.fixedSize()
                                    summary.fixedSize(horizontal: false, vertical: true)
                                }
                            }
                            Spacer(minLength: 0)
                            Image(systemName: expanded ? "chevron.up" : "chevron.down")
                                .font(.caption.weight(.semibold)).accessibilityHidden(true)
                        }
                        .font(.subheadline.weight(.semibold))
                        .padding(.horizontal, Design.padding)
                        .frame(minHeight: Design.control)
                        .contentShape(.capsule)
                    }
                    .buttonStyle(.plain)
                    .glassEffect(glass, in: Capsule())
                    .accessibilityIdentifier("agents.group")
                    .accessibilityHint(expanded ? "Collapses agents" : "Expands agents")

                    if expanded {
                        if agents.isEmpty {
                            Text(agentHeadline(output.isEmpty ? "No agents recorded for this launch yet." : output))
                                .font(.footnote)
                                .frame(maxWidth: .infinity, minHeight: Design.control, alignment: .leading)
                                .padding(.horizontal, Design.padding)
                                .glassEffect(glass, in: Capsule())
                        }
                        ForEach(agents) { agent in
                            row(agent, at: time.date)
                        }
                    }
                }
            }
            .task(id: agents.map(\.id)) {
                await withTaskGroup(of: Void.self) { tasks in
                    for agent in agents {
                        tasks.addTask { await conversation.prefetchAgent(agent) }
                    }
                }
            }
            .alert("Agent action failed", isPresented: Binding(get: { error != nil }, set: { if !$0 { error = nil } })) {
                Button("OK", role: .cancel) {}
            } message: { Text(error ?? "") }
        }
    }

    private var summary: some View {
        let working = agents.count(where: \.isWorking)
        let done = agents.count(where: \.isDone)
        let failed = agents.count(where: \.isFailed)
        let waiting = agents.count - working - done - failed
        let workingText = working > 0 ? Text(" · \(working) working").foregroundColor(Design.accent) : Text("")
        let tail = [(done, "done"), (failed, "failed"), (waiting, "idle")]
            .filter { $0.0 > 0 }.map { " · \($0.0) \($0.1)" }.joined()
        return Text("\(agents.count) \(agents.count == 1 ? "agent" : "agents")\(workingText)\(tail)")
    }

    private func longest(at date: Date) -> Duration? {
        let active = agents.filter(\.isWorking)
        return (active.isEmpty ? agents : active).compactMap { $0.elapsed(at: date) }.max()
    }

    private func row(_ agent: Teammate, at now: Date) -> some View {
        HStack(spacing: Design.gap) {
            Button { open(agent) } label: {
                HStack(spacing: Design.gap) {
                    Image(systemName: agent.isFailed ? "exclamationmark.circle.fill" : agent.isDone ? "checkmark.circle.fill" : "person.circle")
                        .foregroundStyle(agent.isFailed ? Design.failure : agent.isDone ? Design.success : Design.accent)
                        .accessibilityHidden(true)
                    VStack(alignment: .leading, spacing: 2) {
                        HStack(spacing: Design.small) {
                            Text(agent.title).font(.subheadline.weight(.semibold)).lineLimit(1)
                            Text(agent.agentStatusTitle).font(.caption.weight(.semibold))
                                .foregroundStyle(agent.isWorking ? Design.accent : agent.isFailed ? Design.failure : Design.muted)
                        }
                        Text(agentHeadline(agent.isWorking ? agent.activity ?? "Working" : nonempty(agent.result) ?? agent.agentStatusTitle))
                            .font(.footnote).foregroundStyle(Design.muted).lineLimit(1)
                    }
                    Spacer(minLength: 0)
                    if agent.isWorking {
                        VStack(alignment: .trailing, spacing: 2) {
                            if let model = agent.friendlyModel { Text(model).lineLimit(1) }
                            if let elapsed = agent.elapsed(at: now) { Text(workElapsed(elapsed)).monospacedDigit() }
                        }
                        .font(.caption2).foregroundStyle(Design.muted)
                    }
                    if !agent.isFailed {
                        Image(systemName: "chevron.right").font(.caption2).accessibilityHidden(true)
                    }
                }
                .frame(minHeight: Design.control)
                .contentShape(.capsule)
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("agents.child.\(agent.id)")
            .contextMenu {
                Button("Open", systemImage: "arrow.right") { open(agent) }
                if agent.isWorking {
                    Button("Stop", systemImage: "stop.fill", role: .destructive) { control(agent) }
                }
            }
            if agent.isFailed {
                Button("Retry") { control(agent, retry: true) }
                    .font(.caption.weight(.semibold))
                    .frame(minWidth: Design.control, minHeight: Design.control)
                    .buttonStyle(.plain)
                    .foregroundStyle(Design.accent)
                    .accessibilityLabel("Retry \(agent.title)")
            }
        }
        .padding(.horizontal, Design.padding)
        .padding(.vertical, Design.small)
        .glassEffect(glass, in: Capsule())
    }

    private func control(_ agent: Teammate, retry: Bool = false) {
        Task {
            do { try await conversation.controlAgent(agent, retry: retry) }
            catch { self.error = error.localizedDescription }
        }
    }
}

func agentHeadline(_ text: String) -> AttributedString {
    let line = text.split(whereSeparator: \.isNewline).first.map(String.init) ?? text
    return (try? AttributedString(markdown: line, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace)))
        ?? AttributedString(line)
}

private func nonempty(_ text: String?) -> String? {
    text.flatMap { $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : $0 }
}
