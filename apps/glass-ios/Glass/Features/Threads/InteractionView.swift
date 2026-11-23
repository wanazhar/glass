import GlassModels
import SwiftUI

/// The decision a turn is paused on: approval, questions, or a secret.
struct InteractionView: View {
    var interaction: Interaction
    var composer: ComposerModel
    var canWrite: Bool
    var secretsAllowed: String?

    @State private var selections: [Int: Set<String>] = [:]
    @State private var written: [Int: String] = [:]
    @State private var secret = ""
    @State private var busy = false

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            switch interaction.kind {
            case .approval(let reason, let operation):
                Text("Approval needed").font(.headline)
                if !reason.isEmpty { Text(reason).font(.subheadline) }
                if let summary = Self.summary(operation) {
                    Text(summary)
                        .font(.system(.caption, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .lineLimit(6)
                        .textSelection(.enabled)
                }
                actions(primary: "Allow", secondary: "Deny") {
                    await composer.answer(interaction, Interaction.approval(allow: true))
                }
            case .questions(let questions):
                ForEach(Array(questions.enumerated()), id: \.offset) { index, question in
                    questionView(question, index: index)
                }
                actions(primary: "Send answers", secondary: "Skip", enabled: answers(questions).allSatisfy { !$0.isEmpty }) {
                    await composer.answer(interaction, Interaction.answers(answers(questions)))
                }
            case .secret(let purpose, let destination, let secure):
                Text("Secret requested").font(.headline)
                if !purpose.isEmpty { Text(purpose).font(.subheadline) }
                if !destination.isEmpty {
                    LabeledContent("Sent to", value: destination).font(.footnote)
                }
                if let reason = secretsAllowed ?? (secure ? nil : "This host can’t receive secrets securely. Enter it on the host instead.") {
                    Text(reason).font(.footnote).foregroundStyle(.secondary).accessibilityIdentifier("interaction.secret.unavailable")
                    actions(primary: nil, secondary: "Decline") {}
                } else {
                    SecureField("Secret", text: $secret)
                        .textContentType(.password)
                        .autocorrectionDisabled()
                        .textInputAutocapitalization(.never)
                        .privacySensitive()
                        .padding(10)
                        .background(Color(.tertiarySystemBackground), in: .rect(cornerRadius: 10, style: .continuous))
                        .accessibilityIdentifier("interaction.secret.field")
                    Text("Sent once over HTTPS and never stored on this iPhone.").font(.caption).foregroundStyle(.secondary)
                    actions(primary: "Send secret", secondary: "Decline", enabled: !secret.isEmpty && secret.utf8.count <= 4096) {
                        var value = secret
                        secret = ""
                        await composer.submitSecret(&value, for: interaction)
                    }
                }
            case .other(let kind):
                Text("The host is waiting for input this version of Glass can’t show (\(kind)).").font(.subheadline)
                actions(primary: nil, secondary: "Decline") {}
            }
            if !canWrite {
                Text("Take control to answer.").font(.footnote).foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
        .glassPanel(cornerRadius: 24)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier("interaction")
        .onDisappear { secret = "" }
    }

    private func questionView(_ question: Interaction.Question, index: Int) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(question.text).font(.subheadline.weight(.semibold))
            ForEach(question.options, id: \.label) { option in
                let chosen = selections[index, default: []].contains(option.label)
                Button {
                    var set = question.multiSelect ? selections[index, default: []] : []
                    if chosen { set.remove(option.label) } else { set.insert(option.label) }
                    selections[index] = set
                    written[index] = nil
                } label: {
                    HStack(alignment: .firstTextBaseline) {
                        Image(systemName: chosen ? (question.multiSelect ? "checkmark.square.fill" : "largecircle.fill.circle")
                                                 : (question.multiSelect ? "square" : "circle"))
                            .foregroundStyle(chosen ? Color.accentColor : .secondary)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(option.label).foregroundStyle(.primary)
                            if let detail = option.detail { Text(detail).font(.caption).foregroundStyle(.secondary) }
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(.rect)
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(chosen ? .isSelected : [])
                .accessibilityIdentifier("interaction.option")
            }
            TextField("Other answer", text: Binding(get: { written[index] ?? "" }, set: {
                written[index] = $0
                if !$0.isEmpty { selections[index] = [] }
            }))
            .textFieldStyle(.roundedBorder)
            .accessibilityIdentifier("interaction.other")
        }
    }

    private func answers(_ questions: [Interaction.Question]) -> [[String]] {
        questions.indices.map { index in
            if let text = written[index]?.trimmingCharacters(in: .whitespacesAndNewlines), !text.isEmpty { return [text] }
            let chosen = selections[index, default: []]
            return questions[index].options.map(\.label).filter(chosen.contains)
        }
    }

    private func actions(primary: String?, secondary: String, enabled: Bool = true, perform: @escaping () async -> Void) -> some View {
        HStack {
            Button(secondary, role: .cancel) { run { await composer.decline(interaction) } }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("interaction.decline")
            if let primary {
                Button(primary) { run(perform) }
                    .buttonStyle(.borderedProminent)
                    .disabled(!enabled)
                    .accessibilityIdentifier("interaction.submit")
            }
            if busy { ProgressView().padding(.leading, 4) }
        }
        .disabled(!canWrite || busy)
    }

    private func run(_ action: @escaping () async -> Void) {
        busy = true
        Task {
            await action()
            busy = false
        }
    }

    static func summary(_ operation: JSONValue) -> String? {
        switch operation {
        case .null: return nil
        case .string(let text): return text
        default:
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
            return (try? encoder.encode(operation)).flatMap { String(data: $0, encoding: .utf8) }
        }
    }
}
