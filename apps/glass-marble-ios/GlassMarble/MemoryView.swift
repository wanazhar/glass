import SwiftUI

/// What Marble remembers about you, and what it would like permission to keep.
struct MemoryView: View {
    @EnvironmentObject private var store: MarbleStore
    @Environment(\.dismiss) private var dismiss
    @State private var draft = ""
    @State private var editing: MarbleMemoryFact?
    @State private var editDraft = ""

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    composer
                    memories
                    suggestions
                    voice
                }
                .padding(20)
            }
            .background(MarbleBackdrop())
            .navigationTitle("Memory")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") { dismiss() }
                }
            }
            .task { await store.refresh() }
            .alert("Edit memory", isPresented: editingBinding) {
                TextField("Memory", text: $editDraft, axis: .vertical)
                Button("Cancel", role: .cancel) { editing = nil }
                Button("Save") {
                    guard let fact = editing else { return }
                    let text = editDraft
                    editing = nil
                    Task { await store.update(fact, text: text) }
                }
            }
        }
    }

    private var editingBinding: Binding<Bool> {
        Binding(
            get: { editing != nil },
            set: { presented in
                if !presented { editing = nil }
            }
        )
    }

    private var composer: some View {
        HStack(alignment: .bottom, spacing: 8) {
            TextField("Remember that…", text: $draft, axis: .vertical)
                .lineLimit(1...4)
                .foregroundStyle(MarblePalette.ink)
            Button("Save") {
                let text = draft
                draft = ""
                Task { await store.remember(text) }
            }
            .font(.callout.weight(.semibold))
            .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
        .padding(12)
        .marbleGlass(cornerRadius: 20)
    }

    private var memories: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionTitle("Remembered")
            if store.memory.facts.isEmpty {
                EmptyNote("Nothing remembered yet.")
            }
            ForEach(store.memory.facts) { fact in
                HStack(alignment: .top, spacing: 10) {
                    VStack(alignment: .leading, spacing: 3) {
                        Text(fact.kind)
                            .font(.caption2.weight(.semibold))
                            .textCase(.uppercase)
                            .foregroundStyle(MarblePalette.muted)
                        Text(fact.text)
                            .font(.callout)
                            .foregroundStyle(MarblePalette.ink)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    Spacer(minLength: 0)
                    VStack(alignment: .trailing, spacing: 8) {
                        Button("Edit") {
                            editing = fact
                            editDraft = fact.text
                        }
                        .foregroundStyle(MarblePalette.accent)
                        Button("Forget") {
                            Task { await store.forget(fact) }
                        }
                        .foregroundStyle(MarblePalette.danger)
                    }
                    .font(.caption.weight(.semibold))
                    .buttonStyle(.plain)
                }
                .padding(14)
                .frame(maxWidth: .infinity, alignment: .leading)
                .marbleGlass(cornerRadius: 20)
            }
        }
    }

    private var suggestions: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionTitle("Suggested")
            if store.memory.suggestions.isEmpty {
                EmptyNote("No suggestions waiting.")
            }
            ForEach(store.memory.suggestions) { suggestion in
                SuggestionCard(suggestion: suggestion) { keep in
                    Task {
                        if keep {
                            await store.accept(suggestion)
                        } else {
                            await store.dismiss(suggestion)
                        }
                    }
                }
            }
        }
    }

    private var voice: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionTitle("Voice and reach")
            Button {
                Task { await store.toggleVoice() }
            } label: {
                Label(
                    store.voice.enabled ? "Stop reading replies" : "Read replies aloud",
                    systemImage: store.voice.enabled ? "speaker.slash" : "speaker.wave.2"
                )
                .font(.callout.weight(.medium))
                .frame(maxWidth: .infinity, minHeight: 44)
            }
            .marbleGlass(cornerRadius: 18)
            .buttonStyle(.plain)

            Text("Speech stays on this device.")
                .font(.caption2)
                .foregroundStyle(MarblePalette.muted)

            HStack(spacing: 8) {
                Image(
                    systemName: store.notificationState.apnsConfigured
                        ? "bell.badge"
                        : "bell.slash"
                )
                .font(.caption)
                .foregroundStyle(MarblePalette.muted)
                Text(
                    store.notificationState.apnsConfigured
                        ? "Push is configured. This phone registered when it connected."
                        : "No push yet. Add an APNs key in the host's Control panel."
                )
                .font(.caption2)
                .foregroundStyle(MarblePalette.muted)
            }
        }
    }
}

/// One memory Marble noticed, waiting for a decision.
struct SuggestionCard: View {
    var suggestion: MarbleMemorySuggestion
    var onDecision: (Bool) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("remember this?")
                .font(.caption2.weight(.semibold))
                .textCase(.uppercase)
                .foregroundStyle(MarblePalette.muted)
            Text(suggestion.text)
                .font(.callout)
                .foregroundStyle(MarblePalette.ink)
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 8) {
                Button("Not now") { onDecision(false) }
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(MarblePalette.muted)
                Spacer(minLength: 0)
                Button("Save") { onDecision(true) }
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 7)
                    .background(MarblePalette.accent, in: Capsule())
            }
            .buttonStyle(.plain)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .marbleGlass(cornerRadius: 20)
    }
}
