import GlassModels
import SwiftUI

struct ThreadInfoView: View {
    enum Action { case details, commands, permissions }
    let conversation: ConversationModel
    let title: String
    var open: (Action) -> Void
    @Environment(AppModel.self) private var app
    @Environment(\.dismiss) private var dismiss
    @State private var usage: CommandOutput?
    @State private var context: CommandOutput?

    var body: some View {
        NavigationStack {
            List {
                Section {
                    LabeledContent("Model", value: conversation.composer.model ?? conversation.thread.model ?? "Host default")
                    if let host = app.endpoint?.host { LabeledContent("Machine", value: host) }
                    if let workspace = conversation.thread.workspace {
                        LabeledContent("Directory", value: workspace)
                    }
                }
                if let usage {
                    Section("Usage") {
                        ForEach(["reported_prompt_total", "reported_output", "known_cost_subtotal_usd"], id: \.self) { key in
                            if let value = usage.body[key] {
                                let label = switch key {
                                case "reported_prompt_total": "Input tokens"
                                case "reported_output": "Output tokens"
                                default: "Reported cost (USD)"
                                }
                                LabeledContent(label, value: CommandLabel.display(value, key: key))
                            }
                        }
                    }
                }
                if let context {
                    Section("Context") {
                        Text(CommandLabel.display(context.body))
                            .font(.footnote.monospaced())
                            .textSelection(.enabled)
                    }
                }
                Section {
                    Button("Files and activity", systemImage: "folder") { open(.details) }
                    Button("Commands", systemImage: "command") { open(.commands) }
                    Button("Execution profile", systemImage: "slider.horizontal.3") { open(.permissions) }
                }
            }
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
            }
            .task {
                if let result = try? await conversation.composer.command("usage", timeout: .seconds(5)) { usage = CommandOutput(result) }
            }
            .task {
                if let result = try? await conversation.composer.command("context", timeout: .seconds(5)) { context = CommandOutput(result) }
            }
        }
        .presentationDetents([.medium, .large])
    }
}
