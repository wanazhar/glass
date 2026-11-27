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

    private var contextUsed: String {
        if let used = context?.body["used"]?.stringValue { return used }
        if let text = context?.body.stringValue, let estimate = text.firstMatch(of: /Estimated context: (\d+) tokens/) {
            return "\((Int(estimate.1) ?? 0).formatted()) tokens"
        }
        return "Not available"
    }

    private var cost: String {
        guard let amount = usage?.body["known_cost_subtotal_usd"]?.doubleValue else { return "Not available" }
        return amount.formatted(.currency(code: "USD").precision(.fractionLength(2...4)))
    }

    var body: some View {
        NavigationStack {
            List {
                Section {
                    ViewThatFits(in: .horizontal) {
                        HStack(alignment: .top, spacing: 22) { metrics }
                        VStack(alignment: .leading, spacing: 12) { metrics }
                    }
                    .padding(.vertical, Design.gap)
                }
                Section {
                    LabeledContent("Model", value: conversation.composer.model ?? conversation.thread.model ?? "Host default")
                    if let host = app.endpoint?.host { LabeledContent("Machine", value: host) }
                    if let workspace = conversation.thread.workspace {
                        LabeledContent("Directory", value: workspace)
                    }
                }
                if conversation.canWrite {
                    Section {
                        Button("Switch typing to another device", systemImage: "keyboard") {
                            dismiss()
                            Task { await conversation.composer.releaseControl() }
                        }
                        .accessibilityIdentifier("conversation.releaseControl")
                        .tint(Design.accent)
                    }
                }
                if let usage {
                    Section("Usage") {
                        ForEach(["reported_prompt_total", "reported_output"], id: \.self) { key in
                            if let value = usage.body[key] {
                                let label = key == "reported_prompt_total" ? "Input tokens" : "Output tokens"
                                LabeledContent(label, value: CommandLabel.display(value, key: key))
                            }
                        }
                    }
                }
                Section {
                    Button("Files and activity", systemImage: "folder") { open(.details) }
                    Button("Commands", systemImage: "command") { open(.commands) }
                    Button("Execution profile", systemImage: "slider.horizontal.3") { open(.permissions) }
                }
            }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
            .background(Design.background)
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

    @ViewBuilder private var metrics: some View {
        metric("Context used", contextUsed)
        metric("Estimated cost", cost)
    }

    private func metric(_ label: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: Design.small) {
            Text(label).font(.subheadline).foregroundStyle(Design.muted)
            Text(value).font(.title3.weight(.semibold)).monospacedDigit()
        }
        .fixedSize(horizontal: true, vertical: false)
        .accessibilityElement(children: .combine)
    }
}
