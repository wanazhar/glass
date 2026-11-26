import GlassModels
import SwiftUI

enum ChatRow: Identifiable {
    case message(TimelineEntry)
    case work(WorkGroup)

    var id: String {
        switch self {
        case .message(let entry): entry.id
        case .work(let group): group.id
        }
    }

    static func make(from timeline: Timeline) -> [Self] {
        var result: [Self] = []
        var messages: [TimelineEntry] = []
        var steps: [TimelineEntry] = []
        var turn = "continued"
        var seconds: Int?
        func flush(activity: String? = nil) {
            if !steps.isEmpty || activity != nil {
                let anchor = activity == nil ? (messages.last?.id ?? steps.last?.id ?? turn) : turn
                result.append(.work(WorkGroup(id: "work-" + anchor, steps: steps, activity: activity,
                                              seconds: seconds)))
            }
            result.append(contentsOf: messages.map(Self.message))
            messages.removeAll(keepingCapacity: true)
            steps.removeAll(keepingCapacity: true)
            seconds = nil
        }
        for entry in timeline.entries {
            switch entry {
            case .user:
                flush()
                result.append(.message(entry))
                turn = entry.id
            case .assistant, .image:
                messages.append(entry)
            case .turnTiming(_, let elapsed):
                seconds = elapsed
            case .tool(_, let name, _, _, false) where name == "display_image":
                messages.append(entry)
            default:
                steps.append(entry)
            }
        }
        flush(activity: timeline.isActive ? timeline.activity ?? "Working…" : nil)
        return result
    }
}

struct WorkGroup {
    let id: String
    let steps: [TimelineEntry]
    let activity: String?
    let seconds: Int?

    var summary: String {
        var counts: [String] = []
        let kinds = steps.map(WorkStep.kind)
        for (kind, singular, plural) in [(WorkStep.Kind.command, "command", "commands"), (.edit, "edit", "edits"), (.read, "read", "reads")] {
            let count = kinds.count(where: { $0 == kind })
            if count > 0 { counts.append("\(count) \(count == 1 ? singular : plural)") }
        }
        if counts.isEmpty && !steps.isEmpty {
            counts.append("\(steps.count) \(steps.count == 1 ? "step" : "steps")")
        }
        if let seconds {
            let duration = Duration.seconds(max(1, seconds)).formatted(.units(allowed: [.minutes, .seconds], width: .abbreviated))
            return "Worked for \(duration)" + (counts.isEmpty ? "" : " · " + counts.joined(separator: ", "))
        }
        return counts.isEmpty ? "Work" : "Work · " + counts.joined(separator: ", ")
    }

    var failures: String? {
        let failed = steps.filter {
            if case .tool(_, _, _, _, true) = $0 { return true }
            if case .notice(_, _, true) = $0 { return true }
            return false
        }
        guard !failed.isEmpty else { return nil }
        let noun = failed.allSatisfy { WorkStep.kind($0) == .command } ? "command" : "step"
        return "\(failed.count) \(noun)\(failed.count == 1 ? "" : "s") failed"
    }
}

struct WorkRow: View {
    let group: WorkGroup
    var open: (ActivityDetail) -> Void
    @AppStorage(ToolDensity.key) private var density: ToolDensity = .compact
    @State private var expansion: Bool?
    private var expanded: Bool { expansion ?? (density == .expanded) }

    var body: some View {
        VStack(alignment: .leading, spacing: Design.small) {
            if let activity = group.activity {
                HStack(spacing: Design.gap) {
                    ActivityIndicator().accessibilityHidden(true)
                    Text(activity).lineLimit(1)
                }
                .font(.footnote)
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("conversation.activity")
            } else {
                Button {
                    expansion = !expanded
                } label: {
                    HStack(spacing: Design.gap) {
                        Text(group.summary).lineLimit(1)
                        if let failures = group.failures {
                            Label(failures, systemImage: "exclamationmark.circle.fill")
                                .foregroundStyle(Design.failure)
                        }
                        Image(systemName: expanded ? "chevron.up" : "chevron.down")
                            .imageScale(.small)
                            .accessibilityHidden(true)
                    }
                    .padding(.vertical, Design.small)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .font(.footnote)
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("timeline.work")
                if expanded {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(group.steps) { entry in
                            WorkStep(entry: entry, open: open)
                        }
                    }
                    .padding(.leading, Design.gap)
                    .background(alignment: .leading) {
                        Rectangle().fill(.quaternary).frame(width: 1).padding(.vertical, Design.gap)
                    }
                }
            }
        }
    }
}

private struct WorkStep: View {
    enum Kind { case command, read, edit, other }
    var entry: TimelineEntry
    var open: (ActivityDetail) -> Void
    @Environment(\.openSavedActivity) private var openSavedActivity

    nonisolated static func kind(_ entry: TimelineEntry) -> Kind {
        guard case .tool(_, let name, _, _, _) = entry else { return .other }
        switch name.lowercased() {
        case "bash", "shell", "terminal", "run_command", "exec": return .command
        case "read", "read_file", "file_read", "list_files", "glob", "grep": return .read
        case "edit", "edit_file", "write", "write_file", "apply_patch", "patch": return .edit
        default: return .other
        }
    }

    var body: some View {
        switch entry {
        case .reasoning(let id, let text):
            step("text.alignleft", String(text.prefix(while: { !$0.isNewline })), id: "timeline.reasoning") {
                open(ActivityDetail(id: id, title: "Reasoning", text: text, monospaced: false))
            }
        case .tool(let id, let name, let summary, let output, let failed):
            let action: (String, String) = switch Self.kind(entry) {
            case .command: ("terminal", failed == true ? "Command failed" : "Ran")
            case .read: ("doc.text", "Read")
            case .edit: ("pencil", "Edited")
            case .other: ("wrench", name.replacingOccurrences(of: "_", with: " ").capitalized)
            }
            let subject = [.read, .edit].contains(Self.kind(entry)) ? (summary as NSString).lastPathComponent : summary
            let title = subject.isEmpty ? action.1 : "\(action.1) \(subject)"
            let files = Self.kind(entry) == .edit ? DiffSummary.files(in: output) : []
            if files.isEmpty {
                step(action.0, title, id: "timeline.tool", failed: failed == true) {
                    open(ActivityDetail(id: id, title: title, text: output.isEmpty ? summary : output, monospaced: true))
                }
            } else {
                ForEach(files) { file in
                    step("plusminus", "Edited \(file.name) +\(file.added) −\(file.removed)", id: "timeline.diff", failed: failed == true) {
                        open(ActivityDetail(id: id, title: file.name, text: file.diff, monospaced: true))
                    }
                }
            }
        case .largeRecord(_, let offset, _):
            step("doc.text.magnifyingglass", "Large activity", id: "timeline.largeRecord") { openSavedActivity(offset) }
        case .notice(let id, let text, let failed):
            step("info.circle", text, id: "timeline.notice", failed: failed) {
                open(ActivityDetail(id: id, title: "Notice", text: text, monospaced: false))
            }
        case .checklist(let id, let tasks):
            step("checklist", "Updated checklist", id: "timeline.checklist") {
                open(ActivityDetail(id: id, title: "Checklist", text: tasks.joined(separator: "\n"), monospaced: false))
            }
        default:
            EmptyView()
        }
    }

    private func step(_ icon: String, _ text: String, id: String, failed: Bool = false, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label(text, systemImage: failed ? "exclamationmark.circle" : icon)
                .lineLimit(1)
                .frame(maxWidth: .infinity, minHeight: Design.control, alignment: .leading)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .font(.footnote)
        .foregroundStyle(failed ? Design.failure : .secondary)
        .accessibilityIdentifier(id)
    }
}

private struct DiffSummary: Identifiable {
    let name: String
    var diff: String
    var added = 0
    var removed = 0
    var id: String { name }

    static func files(in output: String) -> [Self] {
        var files: [Self] = []
        var current: Self?
        var inHunk = false
        for line in output.split(separator: "\n", omittingEmptySubsequences: false) {
            if line.hasPrefix("diff --git "), let filename = line.split(separator: " ").last {
                if let current { files.append(current) }
                current = Self(name: String(filename.dropFirst(filename.hasPrefix("b/") ? 2 : 0)), diff: String(line))
                inHunk = false
            } else if current != nil {
                current?.diff += "\n" + line
                if line.hasPrefix("@@") { inHunk = true }
                if inHunk, line.hasPrefix("+") { current?.added += 1 }
                if inHunk, line.hasPrefix("-") { current?.removed += 1 }
            }
        }
        if let current { files.append(current) }
        return files
    }
}

struct ActivityDetail: Identifiable {
    let id: String
    let title: String
    let text: String
    let monospaced: Bool
}

struct ActivityDetailView: View {
    let detail: ActivityDetail
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            ScrollView(detail.monospaced ? [.vertical, .horizontal] : .vertical) {
                Text(detail.text)
                    .font(detail.monospaced ? .system(.footnote, design: .monospaced) : .body)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(Design.padding)
                    .accessibilityIdentifier("timeline.step.text")
            }
            .navigationTitle(detail.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    Button("Copy", systemImage: "doc.on.doc") { UIPasteboard.general.string = detail.text }
                }
                ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
            }
        }
    }
}
