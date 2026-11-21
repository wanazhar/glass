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
            Form {
                Section("New routine") {
                    TextField("Title (optional)", text: $title)
                    TextField("What should Marble do each time?", text: $prompt, axis: .vertical)
                        .lineLimit(1...4)
                    TextField("every:1h · every:30m · daily:09:30", text: $schedule)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
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
                    }
                    .disabled(prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)

                    Button("Use this phone's time for daily schedules") {
                        Task { await store.useLocalRoutineTime() }
                    }
                }

                QuietHoursSection()

                Section("Scheduled") {
                    if store.routines.routines.isEmpty {
                        Text("No routines yet.")
                            .foregroundStyle(.secondary)
                    }
                    ForEach(store.routines.routines) { routine in
                        VStack(alignment: .leading, spacing: 4) {
                            Text(routine.title)
                            Text(routine.prompt)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .fixedSize(horizontal: false, vertical: true)
                            Text(meta(routine))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                            if let next = routine.nextRunLabel {
                                Text(next)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            HStack(spacing: 12) {
                                Button("Run now") {
                                    Task { await store.run(routine) }
                                }
                                .buttonStyle(.borderless)
                                Button(routine.enabled ? "Pause" : "Resume") {
                                    Task { await store.setRoutine(routine, enabled: !routine.enabled) }
                                }
                                .buttonStyle(.borderless)
                                Spacer(minLength: 0)
                                Button("Remove", role: .destructive) {
                                    Task { await store.remove(routine) }
                                }
                                .buttonStyle(.borderless)
                            }
                        }
                    }
                }
            }
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

    private func meta(_ routine: MarbleRoutine) -> String {
        var parts = [routine.schedule, routine.lastStatus]
        if routine.runs > 0 {
            parts.append("\(routine.runs) run\(routine.runs == 1 ? "" : "s")")
        }
        if let missed = routine.missedRuns, missed > 0 {
            parts.append("missed \(missed)")
        }
        return parts.joined(separator: " · ")
    }
}

/// Window in which routines keep quiet, saved in host minutes past midnight.
private struct QuietHoursSection: View {
    @EnvironmentObject private var store: MarbleStore
    @State private var enabled = false
    @State private var start = Date()
    @State private var end = Date()
    @State private var loaded = false

    var body: some View {
        Section("Quiet hours") {
            Toggle("Pause routines overnight", isOn: $enabled)
            if enabled {
                DatePicker("Start", selection: $start, displayedComponents: .hourAndMinute)
                DatePicker("End", selection: $end, displayedComponents: .hourAndMinute)
            }
            Button("Save quiet hours") { save() }
        }
        .onAppear {
            guard !loaded else { return }
            loaded = true
            let quiet = store.routines.quietHours ?? .off
            enabled = quiet.enabled
            start = minuteDate(quiet.startMinute)
            end = minuteDate(quiet.endMinute)
        }
    }

    private func minuteDate(_ minute: Int) -> Date {
        Calendar.current.date(
            bySettingHour: minute / 60,
            minute: minute % 60,
            second: 0,
            of: Date()
        ) ?? Date()
    }

    private func dateMinute(_ date: Date) -> Int {
        let parts = Calendar.current.dateComponents([.hour, .minute], from: date)
        return (parts.hour ?? 0) * 60 + (parts.minute ?? 0)
    }

    private func save() {
        let offset = TimeZone.current.secondsFromGMT() / 60
        let quiet = MarbleQuietHours(
            enabled: enabled,
            startMinute: dateMinute(start),
            endMinute: dateMinute(end),
            offsetMinutes: offset
        )
        Task { await store.setRoutineQuiet(quiet) }
    }
}
