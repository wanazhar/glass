import GlassModels
import SwiftUI

/// A command's generated form, its run and its result.
struct CommandView: View {
    @State var run: CommandRun
    var canWrite: Bool

    var body: some View {
        Form {
            if !run.command.description.isEmpty || !run.fields.isEmpty || !run.command.readonly {
                Section {
                    ForEach(run.fields) { field in
                        FieldRow(field: field, value: binding(field.key))
                    }
                    if run.command.needsConfirmation, !run.command.readonly {
                        Toggle("Confirm this change", isOn: $run.confirmed)
                            .accessibilityIdentifier("command.confirm")
                    }
                } header: {
                    if !run.command.description.isEmpty { Text(run.command.description) }
                } footer: {
                    if run.command.readonly {
                        Text("Read-only. Nothing on the host changes.")
                    } else if !canWrite {
                        Text("Take control of this conversation to make changes.")
                    } else {
                        Text(run.status).accessibilityIdentifier("command.status")
                    }
                }
            }
            Section {
                Button {
                    Task { await run.run(canWrite: canWrite) }
                } label: {
                    HStack {
                        Text(run.command.runLabel(action: run.action))
                        if run.running { Spacer(); ProgressView() }
                    }
                }
                .disabled(run.running || (!run.command.readonly && !canWrite))
                .keyboardShortcut(.defaultAction)
                .accessibilityIdentifier("command.run")
                if run.canCancel {
                    Button("Cancel Command", role: .destructive) { Task { await run.cancel() } }
                        .accessibilityIdentifier("command.cancel")
                }
            } footer: {
                if run.running, run.command.readonly || run.inspecting { Text(run.status) }
            }
            if let error = run.error {
                Section {
                    Label(error, systemImage: "exclamationmark.triangle")
                        .foregroundStyle(.red)
                        .accessibilityIdentifier("command.error")
                }
            }
            if let output = run.output {
                CommandResultView(output: output)
            }
        }
        .navigationTitle("/" + run.command.name)
        .navigationBarTitleDisplayMode(.inline)
        .accessibilityIdentifier("command.\(run.command.name)")
        .task { await run.open(canWrite: canWrite) }
    }

    private func binding(_ key: String) -> Binding<String> {
        Binding { run.inputs[key] ?? "" } set: { run.inputs[key] = $0 }
    }
}

private struct FieldRow: View {
    var field: CommandField
    @Binding var value: String

    var body: some View {
        Group {
            if let choices = field.choices {
                Picker(field.key == "action" ? "Action" : "Kind", selection: $value) {
                    ForEach(choices, id: \.self) { Text(CommandField.choiceTitle($0)).tag($0) }
                }
            } else {
                switch field.kind {
                case .textarea:
                    TextField(field.label, text: $value, axis: .vertical).lineLimit(3...10)
                case .password:
                    SecureField(field.label, text: $value).textContentType(.newPassword)
                case .number:
                    TextField(field.label, text: $value).keyboardType(.numberPad)
                case .url:
                    TextField(field.label, text: $value).keyboardType(.URL).textContentType(.URL)
                case .text:
                    TextField(field.label, text: $value)
                }
            }
        }
        .textInputAutocapitalization(.never)
        .autocorrectionDisabled()
        .accessibilityIdentifier("command.field.\(field.key)")
    }
}

/// Renders a command result: Markdown or text bodies, and structured data as grouped rows.
struct CommandResultView: View {
    var output: CommandOutput

    var body: some View {
        switch output.body {
        case .string(let text):
            Section("Result") {
                if output.isMarkdown {
                    MarkdownView(text: text).textSelection(.enabled)
                } else {
                    Text(text).font(.system(.footnote, design: .monospaced)).textSelection(.enabled)
                }
            }
            .accessibilityIdentifier("command.result")
        case .object(let object):
            let scalars = object.filter { !$0.value.isContainer }.sorted { $0.key < $1.key }
            let nested = object.filter(\.value.isContainer).sorted { $0.key < $1.key }
            if !scalars.isEmpty {
                Section("Result") {
                    ForEach(scalars, id: \.key) { ResultRow(key: $0.key, value: $0.value) }
                }
                .accessibilityIdentifier("command.result")
            }
            ForEach(nested.prefix(40), id: \.key) { key, value in
                Section(CommandLabel.title(key)) { NestedRows(value: value, depth: 0) }
            }
        case .array:
            Section("Result") { NestedRows(value: output.body, depth: 0) }
                .accessibilityIdentifier("command.result")
        default:
            Section("Result") { Text(CommandLabel.display(output.body)) }
                .accessibilityIdentifier("command.result")
        }
    }
}

private struct ResultRow: View {
    var key: String
    var value: JSONValue

    var body: some View {
        if case .string(let text) = value, text.contains("\n") || CommandLabel.literalKeys.contains(key) || CommandLabel.proseKeys.contains(key) {
            VStack(alignment: .leading, spacing: 4) {
                Text(CommandLabel.title(key)).font(.footnote).foregroundStyle(.secondary)
                Text(String(text.prefix(65_536))).font(.system(.footnote, design: .monospaced)).textSelection(.enabled)
            }
        } else {
            LabeledContent(CommandLabel.title(key), value: String(CommandLabel.display(value, key: key).prefix(65_536)))
                .textSelection(.enabled)
        }
    }
}

/// Arrays become rows named by their identity key; nested objects drill down.
private struct NestedRows: View {
    var value: JSONValue
    var depth: Int

    var body: some View {
        switch value {
        case .array(let items):
            if items.isEmpty {
                Text("Nothing to show yet.").foregroundStyle(.secondary)
            }
            ForEach(Array(items.prefix(300).enumerated()), id: \.offset) { index, item in
                if item.isContainer, depth < 16 {
                    NavigationLink(CommandLabel.identity(item) ?? "Item \(index + 1)") {
                        Form { NestedRows(value: item, depth: depth + 1) }
                            .navigationTitle(CommandLabel.identity(item) ?? "Item \(index + 1)")
                    }
                } else {
                    Text(CommandLabel.display(item)).textSelection(.enabled)
                }
            }
            if items.count > 300 { Text("Display limit reached. Narrow the query.").foregroundStyle(.secondary) }
        case .object(let object):
            ForEach(object.sorted { $0.key < $1.key }, id: \.key) { key, value in
                if value.isContainer, depth < 16 {
                    NavigationLink(CommandLabel.title(key)) {
                        Form { NestedRows(value: value, depth: depth + 1) }.navigationTitle(CommandLabel.title(key))
                    }
                } else {
                    ResultRow(key: key, value: value)
                }
            }
        default:
            Text(CommandLabel.display(value))
        }
    }
}

extension JSONValue {
    var isContainer: Bool {
        switch self {
        case .array, .object: true
        default: false
        }
    }
}
