import SwiftUI

/// The work Marble does without being asked.
struct RoutinesView: View {
    @EnvironmentObject private var store: MarbleStore
    @Environment(\.dismiss) private var dismiss
    @State private var title = ""
    @State private var prompt = ""
    @State private var schedule = "every:1h"

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    editor
                    list
                }
                .padding(20)
            }
            .background(MarbleBackdrop())
            .navigationTitle("Routines")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") { dismiss() }
                }
            }
            .task { await store.refresh() }
        }
    }

    private var editor: some View {
        VStack(alignment: .leading, spacing: 8) {
            SectionTitle("New routine")
            TextField("Title (optional)", text: $title)
                .marbleField()
            TextField("What should Marble do each time?", text: $prompt, axis: .vertical)
                .lineLimit(1...4)
                .marbleField()
            TextField("every:1h · every:30m · daily:09:30", text: $schedule)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .marbleField()
            Button {
                let newTitle = title
                let newPrompt = prompt
                let newSchedule = schedule
                title = ""
                prompt = ""
                Task {
                    await store.addRoutine(
                        title: newTitle,
                        prompt: newPrompt,
                        schedule: newSchedule
                    )
                }
            } label: {
                Label("Add routine", systemImage: "plus")
                    .font(.callout.weight(.medium))
                    .frame(maxWidth: .infinity, minHeight: 44)
            }
            .marbleGlass(cornerRadius: 18)
            .buttonStyle(.plain)
            .disabled(prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)

            Button("Use this phone's time for daily schedules") {
                Task { await store.useLocalRoutineTime() }
            }
            .font(.caption.weight(.semibold))
            .foregroundStyle(MarblePalette.accent)
        }
    }

    private var list: some View {
        VStack(alignment: .leading, spacing: 10) {
            SectionTitle("Scheduled")
            if store.routines.routines.isEmpty {
                EmptyNote("No routines yet.")
            }
            ForEach(store.routines.routines) { routine in
                VStack(alignment: .leading, spacing: 8) {
                    Text(routine.title)
                        .font(.callout.weight(.semibold))
                        .foregroundStyle(MarblePalette.ink)
                    Text(routine.prompt)
                        .font(.caption)
                        .foregroundStyle(MarblePalette.muted)
                        .fixedSize(horizontal: false, vertical: true)
                    HStack(spacing: 8) {
                        Tag(routine.schedule)
                        Tag(routine.lastStatus)
                        if routine.runs > 0 {
                            Tag("\(routine.runs) run\(routine.runs == 1 ? "" : "s")")
                        }
                        if let missed = routine.missedRuns, missed > 0 {
                            Tag("missed \(missed)")
                        }
                        Spacer(minLength: 0)
                    }
                    if let next = routine.nextRunLabel {
                        Text(next)
                            .font(.caption2)
                            .foregroundStyle(MarblePalette.muted)
                    }
                    HStack(spacing: 12) {
                        Button("Run now") {
                            Task { await store.run(routine) }
                        }
                        .foregroundStyle(MarblePalette.accent)
                        Button(routine.enabled ? "Pause" : "Resume") {
                            Task { await store.setRoutine(routine, enabled: !routine.enabled) }
                        }
                        .foregroundStyle(MarblePalette.accent)
                        Spacer(minLength: 0)
                        Button("Remove", role: .destructive) {
                            Task { await store.remove(routine) }
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
}
