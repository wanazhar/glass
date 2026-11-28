import SwiftUI

/// Saved prompts on a schedule — the host's routines, shown in plain
/// language. Times follow this iPhone's time zone automatically.
struct ScheduledView: View {
    @EnvironmentObject private var store: MarbleStore
    @State private var editing: MarbleRoutine?
    @State private var adding = false

    var body: some View {
        List {
            Section {
                if store.routines.routines.isEmpty {
                    Text("Nothing scheduled. Add one to have Marble do something on a timer.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                ForEach(store.routines.routines) { routine in
                    Button {
                        editing = routine
                    } label: {
                        MarbleAdaptiveRow {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(routine.title.isEmpty ? routine.prompt : routine.title)
                                    .font(.subheadline.weight(.semibold))
                                    .foregroundStyle(.primary)
                                Text(scheduleSummary(routine))
                                    .font(.footnote)
                                    .foregroundStyle(.secondary)
                                if let next = routine.nextRunLabel {
                                    Text(next)
                                        .font(.footnote)
                                        .foregroundStyle(.secondary)
                                }
                            }
                            .fixedSize(horizontal: false, vertical: true)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            Toggle("", isOn: enabledBinding(routine))
                                .labelsHidden()
                                .accessibilityLabel("Schedule enabled")
                        }
                        .frame(minHeight: 44)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .swipeActions(edge: .trailing) {
                        Button("Delete", role: .destructive) {
                            Task { await store.remove(routine) }
                        }
                    }
                    .contextMenu {
                        Button("Run now") { Task { await store.run(routine) } }
                        Button("Delete", role: .destructive) {
                            Task { await store.remove(routine) }
                        }
                    }
                }
            }

            Section {
                Toggle("Don't run between", isOn: quietEnabled)
                if quietHours.enabled {
                    DatePicker("From", selection: quietStart, displayedComponents: .hourAndMinute)
                    DatePicker("To", selection: quietEnd, displayedComponents: .hourAndMinute)
                        .onChange(of: quietEnd.wrappedValue) { saveQuiet() }
                }
                Toggle("Run missed schedules after downtime", isOn: catchUp)
            } header: {
                Text("When schedules may run")
            } footer: {
                Text("Times follow this iPhone's time zone.")
            }
        }
        .navigationTitle("Scheduled")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button { adding = true } label: {
                    Image(systemName: "plus")
                }
                .accessibilityLabel("Add schedule")
            }
        }
        .sheet(isPresented: $adding) {
            NavigationStack { ScheduleEditor(routine: nil) }
        }
        .sheet(item: $editing) { routine in
            NavigationStack { ScheduleEditor(routine: routine) }
        }
        // Time zone is set silently — never a control for it.
        .task { await store.useLocalRoutineTime() }
    }

    private var quietHours: MarbleQuietHours {
        store.routines.quietHours ?? .off
    }

    private func scheduleSummary(_ routine: MarbleRoutine) -> String {
        MarbleSchedule(spec: routine.schedule)?.summary ?? routine.schedule
    }

    private func enabledBinding(_ routine: MarbleRoutine) -> Binding<Bool> {
        Binding(
            get: { routine.enabled },
            set: { enabled in Task { await store.setRoutine(routine, enabled: enabled) } }
        )
    }

    private var quietEnabled: Binding<Bool> {
        Binding(
            get: { quietHours.enabled },
            set: { on in
                var quiet = quietHours
                quiet.enabled = on
                quiet.offsetMinutes = TimeZone.current.secondsFromGMT() / 60
                Task { await store.setRoutineQuiet(quiet) }
            }
        )
    }

    private var quietStart: Binding<Date> {
        quietTime(\.startMinute)
    }

    private var quietEnd: Binding<Date> {
        quietTime(\.endMinute)
    }

    private func quietTime(_ keyPath: WritableKeyPath<MarbleQuietHours, Int>) -> Binding<Date> {
        Binding(
            get: {
                Calendar.current.date(
                    bySettingHour: quietHours[keyPath: keyPath] / 60 % 24,
                    minute: quietHours[keyPath: keyPath] % 60,
                    second: 0,
                    of: Date()
                ) ?? Date()
            },
            set: { date in
                var quiet = quietHours
                let parts = Calendar.current.dateComponents([.hour, .minute], from: date)
                quiet[keyPath: keyPath] = (parts.hour ?? 0) * 60 + (parts.minute ?? 0)
                quiet.offsetMinutes = TimeZone.current.secondsFromGMT() / 60
                Task { await store.setRoutineQuiet(quiet) }
            }
        )
    }

    private func saveQuiet() {
        var quiet = quietHours
        quiet.offsetMinutes = TimeZone.current.secondsFromGMT() / 60
        Task { await store.setRoutineQuiet(quiet) }
    }

    private var catchUp: Binding<Bool> {
        Binding(
            get: { store.routines.routines.allSatisfy { $0.catchUp != false } },
            set: { on in
                for routine in store.routines.routines {
                    Task { await store.setRoutineCatchUp(routine, enabled: on) }
                }
            }
        )
    }
}

/// Add or edit one schedule. "When" maps to the host's `every:` / `daily:`
/// specs; anything else the host supports keeps editing as raw text.
struct ScheduleEditor: View {
    var routine: MarbleRoutine?
    @EnvironmentObject private var store: MarbleStore
    @Environment(\.dismiss) private var dismiss
    @State private var title = ""
    @State private var prompt = ""
    @State private var kind = Kind.hourly
    @State private var interval = 1
    @State private var dailyAt = Calendar.current.date(
        bySettingHour: 9, minute: 0, second: 0, of: Date()
    ) ?? Date()
    /// Which Marble the scheduled turn belongs to; empty is the default.
    @State private var marbleId = ""

    enum Kind: String, CaseIterable { case everyMinutes, hourly, daily }

    var body: some View {
        Form {
            Section {
                TextField("Name (optional)", text: $title)
                TextField("What should Marble do each time?", text: $prompt, axis: .vertical)
                    .lineLimit(2...6)
            }
            Section {
                Picker("Repeat", selection: $kind) {
                    Text("Every N minutes").tag(Kind.everyMinutes)
                    Text("Every N hours").tag(Kind.hourly)
                    Text("Every day at a time").tag(Kind.daily)
                }
                if kind == .daily {
                    DatePicker("Time", selection: $dailyAt, displayedComponents: .hourAndMinute)
                } else {
                    Stepper("Every \(interval) \(kind == .hourly ? (interval == 1 ? "hour" : "hours") : (interval == 1 ? "minute" : "minutes"))", value: $interval, in: 1...99)
                }
            } header: {
                Text("When")
            } footer: {
                Text(schedule.summary)
            }
            Section {
                Picker("For", selection: $marbleId) {
                    Text("Marble").tag("")
                    ForEach(store.prism.marbles.filter { !$0.isDefault }) { marble in
                        Text(marble.name).tag(marble.id)
                    }
                }
            } footer: {
                Text("The schedule runs in that Marble's chat once it has one.")
            }
        }
        .navigationTitle(routine == nil ? "Add schedule" : "Edit schedule")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel") { dismiss() }
            }
            ToolbarItem(placement: .confirmationAction) {
                Button("Save") {
                    Task {
                        await store.addRoutine(
                            title: title,
                            prompt: prompt,
                            schedule: schedule.spec,
                            conversationId: marbleConversation
                        )
                        dismiss()
                    }
                }
                .disabled(prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .onAppear {
            guard let routine else { return }
            title = routine.title
            prompt = routine.prompt
            switch MarbleSchedule(spec: routine.schedule) {
            case .everyMinutes(let n): kind = .everyMinutes; interval = n
            case .everyHours(let n): kind = .hourly; interval = n
            case .daily(let h, let m):
                kind = .daily
                dailyAt = Calendar.current.date(bySettingHour: h, minute: m, second: 0, of: Date()) ?? dailyAt
            case nil: break
            }
        }
    }

    private var marbleConversation: String? {
        guard !marbleId.isEmpty else { return nil }
        return store.prism.marbles.first { $0.id == marbleId }?.conversationId
    }

    private var schedule: MarbleSchedule {
        switch kind {
        case .everyMinutes: return .everyMinutes(interval)
        case .hourly: return .everyHours(interval)
        case .daily:
            let parts = Calendar.current.dateComponents([.hour, .minute], from: dailyAt)
            return .daily(hour: parts.hour ?? 9, minute: parts.minute ?? 0)
        }
    }
}
