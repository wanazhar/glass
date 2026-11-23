import GlassModels
import SwiftUI

/// Actions the app performs itself rather than asking the host.
enum LocalCommand: String {
    case help, diagnostics, layout, clear, tools, editor, animation, theme, settings, conversations, search

    var title: String {
        switch self {
        case .help: "Show all commands"
        case .diagnostics, .layout: "Open Diagnostics"
        case .clear: "Start a new conversation"
        case .tools: "Toggle tool card density"
        case .editor: "Open the full-screen editor"
        case .animation, .theme: "Open Appearance settings"
        case .settings: "Open Settings"
        case .conversations, .search: "Show conversations"
        }
    }
}

/// `⌘K` command palette: the host catalogue grouped as Conversations, Work and Settings.
struct CommandPaletteView: View {
    var conversation: ConversationModel
    var perform: (LocalCommand) -> Void
    @Environment(AppModel.self) private var app
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    @State private var path: [CommandDescriptor] = []
    @State private var highlighted: String?

    private var groups: [(CommandDescriptor.Group, [CommandDescriptor])] {
        CommandCatalogue.grouped(app.commands, query: query)
    }
    private var flat: [CommandDescriptor] { groups.flatMap(\.1).filter(isAvailable) }

    func isAvailable(_ command: CommandDescriptor) -> Bool {
        command.portable || LocalCommand(rawValue: command.name) != nil
    }

    var body: some View {
        NavigationStack(path: $path) {
            List {
                if let error = app.commandsError {
                    Section {
                        Text(error).foregroundStyle(.red)
                        Button("Try Again") { Task { await app.loadCommands(force: true) } }
                    }
                }
                ForEach(groups, id: \.0) { group, commands in
                    Section(group.title) {
                        ForEach(commands) { command in
                            row(command)
                        }
                    }
                }
            }
            .overlay {
                if app.commands.isEmpty, app.commandsError == nil {
                    ProgressView()
                } else if groups.isEmpty {
                    ContentUnavailableView.search(text: query)
                }
            }
            .searchable(text: $query, placement: .navigationBarDrawer(displayMode: .always), prompt: "Find a command")
            .onSubmit(of: .search) { if let first = highlighted.flatMap(command) ?? flat.first { choose(first) } }
            .onChange(of: query) { highlighted = flat.first?.name }
            .onKeyPress(keys: [.upArrow, .downArrow, .home, .end, .return, .space]) { press in
                move(press.key)
            }
            .navigationTitle("Commands")
            .navigationBarTitleDisplayMode(.inline)
            .navigationDestination(for: CommandDescriptor.self) { command in
                CommandView(run: CommandRun(command: command, composer: conversation.composer, app: app),
                            canWrite: conversation.canWrite)
            }
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { dismiss() }
                        .keyboardShortcut(.cancelAction)
                        .accessibilityIdentifier("palette.done")
                }
            }
            .accessibilityIdentifier("palette")
        }
        .task { await app.loadCommands() }
    }

    private func row(_ command: CommandDescriptor) -> some View {
        Button { choose(command) } label: {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("/" + command.name).font(.body.monospaced()).foregroundStyle(.primary)
                    if !command.description.isEmpty {
                        Text(command.description).font(.footnote).foregroundStyle(.secondary)
                    }
                }
                Spacer()
                if !isAvailable(command) {
                    Text("Terminal only").font(.caption).foregroundStyle(.secondary)
                } else if command.readonly {
                    Text("Read-only").font(.caption).foregroundStyle(.secondary)
                }
            }
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .disabled(!isAvailable(command))
        .listRowBackground(highlighted == command.name ? Color.accentColor.opacity(0.15) : nil)
        .accessibilityIdentifier("palette.\(command.name)")
        .accessibilityAddTraits(highlighted == command.name ? .isSelected : [])
    }

    private func command(_ name: String) -> CommandDescriptor? { flat.first { $0.name == name } }

    private func choose(_ command: CommandDescriptor) {
        if command.portable {
            path.append(command)
        } else if let local = LocalCommand(rawValue: command.name) {
            if local == .help { query = ""; return }
            dismiss()
            perform(local)
        }
    }

    private func move(_ key: KeyEquivalent) -> KeyPress.Result {
        guard !flat.isEmpty, path.isEmpty else { return .ignored }
        let index = highlighted.flatMap { name in flat.firstIndex { $0.name == name } }
        switch key {
        case .upArrow: highlighted = flat[max((index ?? 0) - 1, 0)].name
        case .downArrow: highlighted = flat[min((index ?? -1) + 1, flat.count - 1)].name
        case .home: highlighted = flat.first?.name
        case .end: highlighted = flat.last?.name
        case .return, .space:
            guard let name = highlighted, let command = command(name) else { return .ignored }
            choose(command)
        default: return .ignored
        }
        return .handled
    }
}
