import SwiftUI

struct ConnectSettingsView: View {
    @EnvironmentObject private var store: MarbleStore
    @Environment(\.dismiss) private var dismiss
    @State private var address = ""
    @State private var token = ""
    @State private var checking = false
    @State private var note: String?
    @State private var exportURL: URL?
    @State private var showsNotifications = false

    var body: some View {
        NavigationStack {
            Form {
                Section("Host") {
                    TextField("http://100.x.y.z:4783", text: $address)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)
                    SecureField("Access token", text: $token)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                }

                Section {
                    Button(checking ? "Checking…" : "Change host") { changeHost() }
                        .disabled(checking)
                    if let note {
                        Text(note)
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                } footer: {
                    Text("The address is validated against the host before it is saved. The token is kept in the iOS keychain.")
                }

                Section("Model") {
                    ModelPickerRow()
                }

                Section {
                    Button {
                        showsNotifications = true
                    } label: {
                        Label("Notifications & devices", systemImage: "bell.badge")
                    }
                }

                Section("Export") {
                    Button("Export this conversation") {
                        Task { exportURL = await store.exportFile(conversationId: store.selectedConversationId) }
                    }
                    .disabled(store.selectedConversationId == nil)
                    Button("Export everything") {
                        Task { exportURL = await store.exportFile() }
                    }
                    if let exportURL {
                        ShareLink(item: exportURL) {
                            Label("Share \(exportURL.lastPathComponent)", systemImage: "square.and.arrow.up")
                        }
                    }
                }

                Section("This device") {
                    LabeledContent("Status", value: store.statusLabel)
                    LabeledContent("State revision", value: String(store.state.revision))
                    LabeledContent("Pending approvals", value: String(store.state.approvals.count))
                }

                Section("About") {
                    LabeledContent("Version") {
                        Text(versionLabel)
                    }
                    LabeledContent("Host", value: store.endpoint?.displayHost ?? "not connected")
                    LabeledContent("Host version", value: store.hostVersion ?? "unknown")
                    LabeledContent("Engine", value: store.engineHealth?.health ?? "unknown")
                    Text("Glass Marble · a self-hosted assistant on your own hardware.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                Section {
                    Button("Sign out", role: .destructive) {
                        store.signOut()
                        dismiss()
                    }
                } footer: {
                    Text("Forgets this host and token on this device and returns to the connect screen.")
                }
            }
            .navigationTitle("Host")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .sheet(isPresented: $showsNotifications) {
                NotificationsSettingsView()
                    .environmentObject(store)
            }
        }
        .onAppear {
            address = store.endpoint?.baseURL.absoluteString ?? ""
            token = store.endpoint?.token ?? ""
            note = nil
        }
    }

    private var versionLabel: String {
        let info = Bundle.main.infoDictionary
        let version = info?["CFBundleShortVersionString"] as? String ?? "dev"
        let build = info?["CFBundleVersion"] as? String ?? ""
        let commit = info?["MarbleGitCommit"] as? String ?? "dev"
        return "\(version) (\(build)) · \(commit)"
    }

    /// Same validation as onboarding: nothing is saved unless the host answers.
    private func changeHost() {
        checking = true
        let rawAddress = address
        let rawToken = token.trimmingCharacters(in: .whitespacesAndNewlines)
        Task {
            defer { checking = false }
            let (verdict, endpoint) = await MarbleProbe.check(
                host: rawAddress,
                token: rawToken
            )
            if verdict.isReady, let endpoint {
                store.apply(endpoint)
            }
            note = verdict.message
        }
    }
}

/// Free-text model choice: empty means the host uses its engine default.
private struct ModelPickerRow: View {
    @EnvironmentObject private var store: MarbleStore
    @State private var draft = ""
    @State private var loaded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            TextField("engine default", text: $draft)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            HStack {
                Text(store.model.isEmpty ? "using engine default" : "using \(store.model)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                Button("Apply") {
                    Task { await store.setModel(draft.isEmpty ? nil : draft) }
                }
                Button("Use engine default") {
                    draft = ""
                    Task { await store.setModel(nil) }
                }
            }
            .font(.caption.weight(.semibold))
        }
        .onAppear {
            guard !loaded else { return }
            loaded = true
            draft = store.model
        }
    }
}

/// Host notification settings, registered push devices, and this phone's local
/// alert opt-in.
struct NotificationsSettingsView: View {
    @EnvironmentObject private var store: MarbleStore
    @Environment(\.dismiss) private var dismiss
    @State private var webhookUrl = ""
    @State private var webhookToken = ""
    @State private var selectedEvents: Set<String> = []
    @State private var quietEnabled = false
    @State private var quietStart = Date()
    @State private var quietEnd = Date()
    @State private var reAlert = 300
    @State private var note: String?
    @State private var loaded = false

    private var settings: MarbleNotificationSettings { store.notificationState }

    var body: some View {
        NavigationStack {
            Form {
                Section("On this phone") {
                    Toggle("Local alerts", isOn: $store.localNotificationsEnabled)
                    Text("Notifies when a turn settles or needs approval while you are not looking at it.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                Section("Webhook") {
                    TextField("https://hooks.example/marble", text: $webhookUrl)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)
                    SecureField("webhook token (optional)", text: $webhookToken)
                        .textInputAutocapitalization(.never)
                    Text(settings.webhookTokenSet ? "a token is already set — a new value replaces it" : "no token set")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                Section("Events") {
                    ForEach(settings.availableEvents, id: \.self) { event in
                        Toggle(event, isOn: eventBinding(event))
                    }
                }

                Section("Quiet hours") {
                    Toggle("Enabled", isOn: $quietEnabled)
                    DatePicker("Start", selection: $quietStart, displayedComponents: .hourAndMinute)
                    DatePicker("End", selection: $quietEnd, displayedComponents: .hourAndMinute)
                    Stepper("Re-alert every \(reAlert / 60) min", value: $reAlert, in: 60...3600, step: 60)
                }

                Section {
                    Button("Save") { save() }
                    Button("Send a test notification") {
                        Task {
                            note = await store.testNotification()
                                ? "test sent" : "test failed — see the host log"
                        }
                    }
                    if let note {
                        Text(note)
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                }

                Section("Registered devices") {
                    if store.state.deviceList.isEmpty {
                        Text("No devices registered. Remote push stays off in this build.")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                    ForEach(store.state.deviceList) { device in
                        HStack {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(device.tokenPrefix)
                                    .font(.caption.monospaced())
                                Text("\(device.platform) · \(device.topic)")
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            Button("Remove", role: .destructive) {
                                Task { await store.removeDevice(device) }
                            }
                            .font(.caption.weight(.semibold))
                        }
                    }
                }
            }
            .navigationTitle("Notifications")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .onAppear {
            guard !loaded else { return }
            loaded = true
            webhookUrl = settings.webhookUrl
            selectedEvents = Set(settings.events)
            let quiet = settings.quietHours ?? .off
            quietEnabled = quiet.enabled
            quietStart = minuteDate(quiet.startMinute)
            quietEnd = minuteDate(quiet.endMinute)
            reAlert = settings.reAlertSeconds ?? 300
        }
    }

    private func eventBinding(_ event: String) -> Binding<Bool> {
        Binding(
            get: { selectedEvents.contains(event) },
            set: { on in
                if on { selectedEvents.insert(event) } else { selectedEvents.remove(event) }
            }
        )
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
        let update = MarbleClient.NotificationUpdate(
            webhookUrl: webhookUrl.trimmingCharacters(in: .whitespacesAndNewlines),
            webhookToken: webhookToken.trimmingCharacters(in: .whitespacesAndNewlines),
            events: settings.availableEvents.filter { selectedEvents.contains($0) },
            apns: settings.apns,
            quietHours: MarbleQuietHours(
                enabled: quietEnabled,
                startMinute: dateMinute(quietStart),
                endMinute: dateMinute(quietEnd),
                offsetMinutes: offset
            ),
            reAlertSeconds: reAlert,
            retries: settings.retries,
            dedupeSeconds: settings.dedupeSeconds
        )
        Task {
            await store.saveNotifications(update)
            note = "saved"
        }
    }
}
