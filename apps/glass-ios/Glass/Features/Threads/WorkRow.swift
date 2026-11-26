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
                result.append(.work(WorkGroup(id: "work-" + turn, steps: steps, activity: activity,
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

    var runFailed: Bool {
        activity == nil && steps.contains { if case .notice(_, _, true) = $0 { true } else { false } }
    }

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
    var resume: (() -> Void)?
    @AppStorage(ToolDensity.key) private var density: ToolDensity = .compact
    @State private var expansion: Bool?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    private var expanded: Bool { expansion ?? (density == .expanded) }

    var body: some View {
        VStack(alignment: .leading, spacing: Design.small) {
            Button {
                UIImpactFeedbackGenerator(style: .light).impactOccurred()
                withAnimation(reduceMotion ? nil : .spring(duration: 0.28, bounce: 0.04)) {
                    expansion = !expanded
                }
            } label: {
                HStack(spacing: Design.gap) {
                    if group.activity != nil {
                        ActivityIndicator().accessibilityHidden(true)
                    }
                    Text(group.activity != nil ? "Glass is working" : group.summary).lineLimit(1)
                    if let failures = group.failures {
                        Label(failures, systemImage: "exclamationmark.circle.fill")
                            .foregroundStyle(Design.failure)
                    }
                    Image(systemName: expanded ? "chevron.up" : "chevron.down")
                        .imageScale(.small)
                        .accessibilityHidden(true)
                }
                .frame(minHeight: Design.control)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .font(.footnote)
            .foregroundStyle(.secondary)
            .accessibilityIdentifier(group.activity != nil ? "conversation.activity" : "timeline.work")
            if expanded {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(group.steps) { entry in
                        WorkStep(entry: entry, open: open)
                    }
                }
                .padding(.leading, Design.gap)
                .background(alignment: .leading) {
                    Rectangle().fill(.quaternary).frame(width: 1).padding(.vertical, Design.gap)
                }
            }
            if group.runFailed {
                VStack(alignment: .leading, spacing: Design.gap) {
                    Label("The run stopped with an error", systemImage: "exclamationmark.circle.fill")
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(Design.failure)
                    if let resume {
                        Button("Continue", action: resume).glassButtonStyle(prominent: true)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(Design.padding)
                .background(Design.failure.opacity(0.06), in: .rect(cornerRadius: Design.corner))
                .accessibilityIdentifier("timeline.failure")
            }
        }
    }
}

private struct WorkStep: View {
    enum Kind { case command, read, edit, other }
    var entry: TimelineEntry
    var open: (ActivityDetail) -> Void
    @Environment(\.openSavedActivity) private var openSavedActivity
    @State private var showsReasoning = false

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
        case .reasoning(_, let text):
            step("text.alignleft", String(text.prefix(while: { !$0.isNewline })), id: "timeline.reasoning") {
                showsReasoning.toggle()
            }
            if showsReasoning {
                Text(text).font(.footnote).foregroundStyle(.secondary)
                    .textSelection(.enabled).padding(.bottom, Design.gap)
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
                step(action.0, Self.kind(entry) == .command ? summary : title, id: "timeline.tool",
                     failed: failed == true, running: failed == nil, monospaced: Self.kind(entry) == .command) {
                    open(ActivityDetail(id: id, title: title, text: output.isEmpty ? summary : output, monospaced: true))
                }
            } else {
                ForEach(files) { file in
                    step("plusminus", "Edited \(file.name) +\(file.added) −\(file.removed)", id: "timeline.diff", failed: failed == true) {
                        open(ActivityDetail(id: id, title: file.name, text: file.diff, monospaced: true, isDiff: true))
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

    private func step(_ icon: String, _ text: String, id: String, failed: Bool = false, running: Bool = false,
                      monospaced: Bool = false, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: Design.gap) {
                if running { ProgressView().controlSize(.mini) }
                else { Image(systemName: failed ? "exclamationmark.circle" : icon).frame(width: 18) }
                Text(text)
                    .font(monospaced ? .footnote.monospaced() : .footnote)
                    .lineLimit(monospaced ? 2 : 1)
            }
            .frame(maxWidth: .infinity, minHeight: Design.control, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
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
    var isDiff = false
}

struct ActivityDetailView: View {
    let detail: ActivityDetail
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            ScrollView(detail.monospaced ? [.vertical, .horizontal] : .vertical) {
                Group {
                    if detail.isDiff {
                        LazyVStack(alignment: .leading, spacing: 0) {
                            ForEach(Array(detail.text.components(separatedBy: "\n").enumerated()), id: \.offset) { index, line in
                                HStack(alignment: .firstTextBaseline, spacing: Design.gap) {
                                    Text("\(index + 1)").foregroundStyle(.secondary).frame(minWidth: 26, alignment: .trailing)
                                    Text(line).fixedSize(horizontal: true, vertical: false)
                                }
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(.horizontal, Design.gap)
                                .padding(.vertical, 3)
                                .background(line.hasPrefix("+") ? Color.green.opacity(0.14) : line.hasPrefix("-") ? Color.red.opacity(0.12) : .clear)
                            }
                        }
                        .font(.footnote.monospaced())
                    } else {
                        Text(detail.text).font(detail.monospaced ? .footnote.monospaced() : .body)
                    }
                }
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
