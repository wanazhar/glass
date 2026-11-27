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

    var cause: String? {
        steps.reversed().compactMap { entry in
            if case .notice(_, let text, true) = entry {
                return text.hasPrefix("stopped: ") ? String(text.dropFirst("stopped: ".count)) : text
            }
            return nil
        }.first
    }

    var failedCommands: [ActivityDetail] {
        steps.compactMap { entry in
            guard case .tool(let id, _, let command, let output, true) = entry,
                  WorkStep.kind(entry) == .command else { return nil }
            return ActivityDetail(id: id, title: "Command output", text: output, monospaced: true,
                                  command: CommandActivity(command: command, output: output, duration: nil, failed: true))
        }
    }

    var runFailed: Bool {
        activity == nil && cause != nil
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
            let duration = workElapsed(.seconds(max(1, seconds)))
            return "Worked for \(duration)" + (counts.isEmpty ? "" : " · " + counts.joined(separator: ", "))
        }
        return counts.isEmpty ? "Work" : "Work · " + counts.joined(separator: ", ")
    }

    var failures: String? {
        let commands = failedCommands.count
        if commands > 0 {
            return "\(commands) \(commands == 1 ? "command" : "commands") failed"
        }
        return runFailed ? "Run stopped" : nil
    }
}

struct WorkRow: View {
    let group: WorkGroup
    let durations: [String: Duration]
    var open: (ActivityDetail) -> Void
    @AppStorage(ToolDensity.key) private var density: ToolDensity = .compact
    @State private var expansion: Bool?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    private var expanded: Bool {
        #if DEBUG
        if expansion == nil, LensPreview.current == .workLog || LensPreview.current == .ipad { return true }
        #endif
        // The log opens and closes only when the reader asks, so a streaming reply never jumps.
        return expansion ?? (density == .expanded)
    }

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
                        ActivityIndicator().tint(Design.accent).frame(width: 20).accessibilityHidden(true)
                    } else {
                        Image(systemName: group.failures == nil ? "checkmark.circle.fill" : "exclamationmark.circle.fill")
                            .foregroundStyle(group.failures == nil ? Design.success : Design.failure)
                            .frame(width: 20).accessibilityHidden(true)
                    }
                    Text(group.activity != nil ? "Work in progress" : group.summary)
                        .fixedSize(horizontal: false, vertical: true)
                    Spacer(minLength: Design.small)
                    Image(systemName: expanded ? "chevron.up" : "chevron.down")
                        .imageScale(.small)
                        .accessibilityHidden(true)
                }
                .frame(minHeight: Design.control)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .font(.subheadline.weight(.semibold))
            .foregroundStyle(Design.ink)
            .accessibilityHint(expanded ? "Collapses work log" : "Expands work log")
            .accessibilityIdentifier(group.activity != nil ? "conversation.activity" : "timeline.work")
            if expanded {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(group.steps) { entry in
                        WorkStep(entry: entry, duration: durations[entry.id], open: open)
                    }
                    if let activity = group.activity, !group.steps.contains(where: {
                        if case .tool(_, _, _, _, nil) = $0 { return true }
                        return false
                    }) {
                        HStack(spacing: Design.gap) {
                            ActivityIndicator().tint(Design.accent).frame(width: 18).accessibilityHidden(true)
                            Text(activity).font(.footnote)
                        }
                        .frame(minHeight: Design.control)
                    }
                }
                .padding(.leading, Design.gap)
                .background(alignment: .leading) {
                    Rectangle().fill(Design.accent.opacity(0.44)).frame(width: 1).padding(.vertical, Design.gap)
                }
            }
        }
        .padding(.horizontal, 13)
        .padding(.vertical, expanded ? 13 : 0)
        .background(expanded ? Design.surface : .clear, in: .rect(cornerRadius: Design.panel))
        .overlay(RoundedRectangle(cornerRadius: Design.panel).strokeBorder(expanded ? Design.line : .clear))
    }
}

struct RunRecovery: View {
    let group: WorkGroup
    let open: (ActivityDetail) -> Void
    let resume: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: Design.gap) {
            Label(group.failures ?? "Run stopped", systemImage: "exclamationmark.octagon.fill")
                .font(.subheadline.weight(.semibold)).foregroundStyle(Design.failure)
            if let cause = group.cause {
                Text(cause).font(.subheadline).foregroundStyle(Design.ink)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if let detail = group.failedCommands.last, let command = detail.command {
                Button { open(detail) } label: {
                    HStack(alignment: .firstTextBaseline, spacing: Design.gap) {
                        Image(systemName: "terminal").foregroundStyle(Design.failure)
                        VStack(alignment: .leading, spacing: Design.small) {
                            Text(command.command).font(.footnote.monospaced().weight(.medium))
                                .fixedSize(horizontal: false, vertical: true)
                            Text(command.exitCode.map { "Exit \($0) · View output" } ?? "View output")
                                .font(.caption).foregroundStyle(Design.failure)
                        }
                        Spacer(minLength: 0)
                        Image(systemName: "chevron.right").font(.caption2).foregroundStyle(Design.muted)
                    }
                    .frame(minHeight: Design.control)
                    .padding(.horizontal, 12).padding(.vertical, 4)
                    .background(Design.inset, in: .rect(cornerRadius: 10))
                }
                .buttonStyle(.plain)
                .accessibilityIdentifier("timeline.failedCommand")
            }
            AccentAction(title: "Continue", icon: "arrow.right", action: resume)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .background(Design.surface, in: .rect(cornerRadius: 16))
        .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(Design.line))
        .overlay(alignment: .leading) {
            Capsule().fill(Design.failure).frame(width: 3).padding(.vertical, 16)
        }
        .accessibilityIdentifier("timeline.failure")
    }
}

private struct WorkStep: View {
    enum Kind { case command, read, edit, other }
    var entry: TimelineEntry
    var duration: Duration?
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
            step(showsReasoning ? "chevron.up" : "chevron.down", "Reasoning summary", id: "timeline.reasoning") {
                showsReasoning.toggle()
            }
            if showsReasoning {
                Text(text).font(.footnote).foregroundStyle(Design.muted)
                    .textSelection(.enabled).padding(.leading, 26).padding(.bottom, Design.gap)
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
                let isCommand = Self.kind(entry) == .command
                step(action.0, isCommand ? summary : title, id: "timeline.tool",
                     failed: failed == true, running: failed == nil, monospaced: isCommand) {
                    open(ActivityDetail(id: id, title: isCommand ? "Command output" : title,
                                        text: output.isEmpty ? summary : output, monospaced: true,
                                        command: isCommand ? CommandActivity(command: summary, output: output,
                                                                             duration: duration, failed: failed == true) : nil))
                }
            } else {
                ForEach(files) { file in
                    Button {
                        open(ActivityDetail(id: id, title: file.name, text: file.diff, monospaced: true, isDiff: true))
                    } label: {
                        HStack(spacing: 7) {
                            Image(systemName: "point.topleft.down.to.point.bottomright.curvepath")
                            Text(file.name).lineLimit(1)
                            Text("+\(file.patch.added)").foregroundStyle(Design.success)
                            Text("−\(file.patch.removed)").foregroundStyle(Design.failure)
                        }
                        .font(.caption.monospaced())
                        .padding(.horizontal, 9).padding(.vertical, 6)
                        .background(Design.inset, in: .capsule)
                        .frame(minHeight: Design.control)
                        .contentShape(.rect)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("timeline.diff")
                }
            }
        case .largeRecord(_, let offset, _):
            step("doc.text.magnifyingglass", "Show more activity", id: "timeline.largeRecord") { openSavedActivity(offset) }
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
                if running { ActivityIndicator().tint(Design.accent).frame(width: 18).accessibilityHidden(true) }
                else { Image(systemName: failed ? "exclamationmark.circle" : icon).frame(width: 18).accessibilityHidden(true) }
                Text(text)
                    .font(monospaced ? .footnote.monospaced() : .footnote)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .frame(maxWidth: .infinity, minHeight: Design.control, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(failed ? Design.failure : id == "timeline.reasoning" ? Design.accent : Design.ink)
        .accessibilityIdentifier(id)
    }
}

private struct DiffSummary: Identifiable {
    let name: String
    var diff: String
    var patch: LensPatch { LensPatch(diff) }
    var id: String { name }

    static func files(in output: String) -> [Self] {
        var files: [Self] = []
        var current: Self?
        for line in output.split(separator: "\n", omittingEmptySubsequences: false) {
            if line.hasPrefix("diff --git "), let filename = line.split(separator: " ").last {
                if let current { files.append(current) }
                current = Self(name: String(filename.dropFirst(filename.hasPrefix("b/") ? 2 : 0)), diff: String(line))
            } else if current != nil {
                current?.diff += "\n" + line
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
    var command: CommandActivity?

    func copyTitle(hasSavedOutput: Bool) -> String {
        hasSavedOutput || command?.truncated == true ? "Copy preview" : "Copy"
    }
}

struct CommandActivity {
    let command: String
    let stdout: String
    let stderr: String?
    let exitCode: Int?
    let duration: Duration?
    let failed: Bool
    let truncated: Bool
    var durationLabel: String {
        duration?.formatted(.units(allowed: [.seconds, .milliseconds], width: .abbreviated, maximumUnitCount: 1)) ?? "—"
    }

    init(command: String, output: String, duration: Duration?, failed: Bool) {
        self.command = command
        self.duration = duration
        truncated = output.hasSuffix("\nPreview capped · full output remains on the host.")
        var body = output[...]
        if body.hasPrefix("$ "), let end = body.firstIndex(of: "\n") {
            body = body[body.index(after: end)...]
        }
        let firstLine = body.prefix { !$0.isNewline }
        let code = firstLine.hasPrefix("[exit ") && firstLine.hasSuffix("]")
            ? Int(firstLine.dropFirst(6).dropLast()) : nil
        exitCode = code
        if code != nil {
            body = body.dropFirst(firstLine.count)
            if body.first?.isNewline == true { body = body.dropFirst() }
        }
        if body.hasPrefix("stderr:\n") {
            stdout = ""
            stderr = String(body.dropFirst("stderr:\n".count))
        } else if let marker = body.range(of: "\nstderr:\n") {
            stdout = String(body[..<marker.lowerBound])
            stderr = String(body[marker.upperBound...])
        } else {
            stdout = String(body)
            stderr = nil
        }
        self.failed = code.map { $0 != 0 } ?? failed
    }
}

struct ActivityDetailView: View {
    let detail: ActivityDetail
    var savedOutput: SavedActivityView?
    @Environment(\.dismiss) private var dismiss
    @State private var showingFullOutput = false

    var body: some View {
        NavigationStack {
            ScrollView {
                Group {
                    if let command = detail.command {
                        commandOutput(command, preview: true)
                    } else if detail.isDiff {
                        let patch = LensPatch(detail.text)
                        VStack(alignment: .leading, spacing: 12) {
                            PatchCounts(added: patch.added, removed: patch.removed)
                            PatchRows(patch: patch)
                        }
                        .padding(Design.padding)
                        .lensCard()
                    } else {
                        Text(detail.text).font(detail.monospaced ? .footnote.monospaced() : .body)
                    }
                }
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(Design.padding)
                .accessibilityIdentifier("timeline.step.text")
            }
            .defaultScrollAnchor(.topLeading, for: .alignment)
            .background(Design.background)
            .navigationTitle(detail.title)
            .navigationBarTitleDisplayMode(.inline)
            .sheet(isPresented: $showingFullOutput) {
                if let savedOutput {
                    savedOutput
                } else if let command = detail.command {
                    NavigationStack {
                        ScrollView { commandOutput(command, preview: false).padding(Design.padding) }
                            .navigationTitle("Full command output")
                            .navigationBarTitleDisplayMode(.inline)
                            .toolbar {
                                ToolbarItem(placement: .confirmationAction) {
                                    Button("Done") { showingFullOutput = false }
                                }
                            }
                    }
                }
            }
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    let title = detail.copyTitle(hasSavedOutput: savedOutput != nil)
                    Button {
                        UIPasteboard.general.string = detail.text
                    } label: {
                        if title == "Copy" {
                            Label(title, systemImage: "doc.on.doc")
                        } else {
                            Text(title)
                        }
                    }
                    .frame(minHeight: Design.control)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }.frame(minHeight: Design.control)
                }
            }
        }
        .presentationDetents(detail.isDiff ? [.height(280), .large] : detail.command?.failed == true ? [.medium, .large] : [.large])
    }

    private func commandOutput(_ command: CommandActivity, preview: Bool) -> some View {
        VStack(alignment: .leading, spacing: Design.padding) {
            Text(command.command)
                .font(Design.mono.weight(.semibold))
                .textSelection(.enabled)
                .padding(Design.padding)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Design.surface, in: .rect(cornerRadius: Design.corner))

            HStack(spacing: Design.padding) {
                if let code = command.exitCode {
                    Label("Exit code \(code)", systemImage: command.failed ? "xmark.circle.fill" : "checkmark.circle.fill")
                        .foregroundStyle(command.failed ? Design.failure : .secondary)
                }
                Label(command.durationLabel, systemImage: "clock")
                    .foregroundStyle(.secondary)
                    .accessibilityLabel(command.duration == nil ? "Duration unavailable" : "Duration: \(command.durationLabel)")
                    .accessibilityIdentifier("command.duration")
            }
            .font(Design.footnote)
            if preview, savedOutput != nil {
                Label("Output preview. Open the saved record to read the rest.", systemImage: "info.circle")
                    .font(Design.footnote)
                    .foregroundStyle(.secondary)
            }
            if !command.stdout.isEmpty {
                outputSection("Output", command.stdout, tint: .primary, preview: preview)
            }
            if let stderr = command.stderr, !stderr.isEmpty {
                outputSection("Standard error", stderr, tint: Design.failure, preview: preview)
            }
            if preview, savedOutput != nil || command.stdout.count > 2_000 || (command.stderr?.count ?? 0) > 2_000 {
                Button("Show more", systemImage: "arrow.up.right.square") { showingFullOutput = true }
                .glassButtonStyle()
            }
        }
    }

    private func outputSection(_ title: String, _ output: String, tint: Color, preview: Bool) -> some View {
        VStack(alignment: .leading, spacing: Design.gap) {
            Text(title).font(Design.headline)
            ScrollView(.horizontal) {
                Text(preview && output.count > 2_000 ? String(output.prefix(2_000)) + "…" : output)
                    .font(Design.mono)
                    .foregroundStyle(tint)
                    .fixedSize(horizontal: true, vertical: false)
            }
        }
        .padding(Design.padding)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(tint == Design.failure ? Design.failure.opacity(0.10) : Design.terminal,
                    in: .rect(cornerRadius: Design.corner))
    }
}
