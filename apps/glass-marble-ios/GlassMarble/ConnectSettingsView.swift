import SwiftUI

/// Settings submenus, grouped by what they are for. Activity and Scheduled
/// live here instead of in their own tabs.
enum MarbleSettingsRoute: String, Hashable, CaseIterable {
    case connection, marbles, activity, scheduled, notifications, data, about

    var title: String {
        switch self {
        case .connection: "Connection"
        case .marbles: "Marbles and models"
        case .activity: "Approvals and activity"
        case .scheduled: "Scheduled"
        case .notifications: "Notifications"
        case .data: "Chats and data"
        case .about: "About"
        }
    }

    var systemImage: String {
        switch self {
        case .connection: "desktopcomputer"
        case .marbles: "circle.hexagongrid"
        case .activity: "checkmark.shield"
        case .scheduled: "clock"
        case .notifications: "bell"
        case .data: "tray.full"
        case .about: "info.circle"
        }
    }

    @MainActor @ViewBuilder
    var destination: some View {
        switch self {
        case .connection: ConnectionSettingsView()
        case .marbles: MarblesModelsSettingsView()
        case .activity: ActivityView()
        case .scheduled: ScheduledView()
        case .notifications: NotificationSettingsView()
        case .data: DataSettingsView()
        case .about: AboutSettingsView()
        }
    }
}

/// Settings: native grouped clusters, each opening its own screen.
struct SettingsView: View {
    @EnvironmentObject private var store: MarbleStore
    @State private var confirmStop = false

    var body: some View {
        List {
            Section {
                NavigationLink(value: MarbleSettingsRoute.connection) {
                    LabeledContent {
                        Text(store.statusLabel)
                    } label: {
                        Label {
                            Text(store.endpoint?.displayHost ?? "Host")
                                .lineLimit(1)
                        } icon: {
                            Image(systemName: MarbleSettingsRoute.connection.systemImage)
                        }
                    }
                }
                .accessibilityIdentifier("settings.connection")
                link(.marbles)
            }
            Section {
                NavigationLink(value: MarbleSettingsRoute.activity) {
                    LabeledContent {
                        if waiting > 0 {
                            Text("\(waiting) waiting")
                                .foregroundStyle(Color.accentColor)
                        }
                    } label: {
                        Label(MarbleSettingsRoute.activity.title, systemImage: MarbleSettingsRoute.activity.systemImage)
                    }
                }
                .accessibilityIdentifier("settings.activity")
                NavigationLink(value: MarbleSettingsRoute.scheduled) {
                    LabeledContent {
                        let count = store.routines.routines.count
                        if count > 0 { Text("\(count)") }
                    } label: {
                        Label(MarbleSettingsRoute.scheduled.title, systemImage: MarbleSettingsRoute.scheduled.systemImage)
                    }
                }
                .accessibilityIdentifier("settings.scheduled")
            }
            Section {
                link(.notifications)
                link(.data)
            }
            Section {
                link(.about)
            }
            Section {
                Button(role: .destructive) {
                    confirmStop = true
                } label: {
                    Label("Stop everything", systemImage: "stop.circle")
                        .foregroundStyle(.red)
                }
                .accessibilityIdentifier("settings.stopEverything")
            } footer: {
                Text("Cancels every running turn on the host.")
            }
        }
        .scrollContentBackground(.hidden)
        .background(MarbleTheme.paper)
        .navigationTitle("Settings")
        .navigationDestination(for: MarbleSettingsRoute.self) { $0.destination }
        .confirmationDialog("Stop everything?", isPresented: $confirmStop, titleVisibility: .visible) {
            Button("Stop everything", role: .destructive) {
                Task { await store.stopEverything() }
            }
        } message: {
            Text("Every running turn is cancelled.")
        }
    }

    private var waiting: Int {
        store.state.approvals.count + store.memory.suggestions.count
    }

    private func link(_ route: MarbleSettingsRoute) -> some View {
        NavigationLink(value: route) {
            Label(route.title, systemImage: route.systemImage)
        }
        .accessibilityIdentifier("settings.\(route.rawValue)")
    }
}

/// The host this iPhone talks to, how the connection is, and how to leave it.
struct ConnectionSettingsView: View {
    @EnvironmentObject private var store: MarbleStore
    @State private var showsChangeHost = false

    var body: some View {
        Form {
            Section {
                LabeledContent("Host", value: store.endpoint?.displayHost ?? "not connected")
                LabeledContent("Status", value: store.statusLabel)
                Button(store.connectionPaused ? "Connect" : "Reconnect now") { store.reconnect(force: true) }
            }
            Section {
                Toggle("Keep connection alive in background", isOn: $store.backgroundConnectionEnabled)
                    .accessibilityIdentifier("settings.backgroundConnection")
                if store.locationPermissionDenied {
                    Text("Allow location while using Marble in iPhone Settings to keep the connection open.")
                        .foregroundStyle(.secondary)
                }
            } footer: {
                Text("While connected, Marble uses low-accuracy location updates to stay online. This can use more battery, and iOS shows the location indicator. Marble does not save or send your location.")
            }
            Section {
                MarbleAppVersionRows()
                MarbleHostVersionRow()
            }
            Section {
                Button("Change host…") { showsChangeHost = true }
                Button("Sign out", role: .destructive) { store.signOut() }
            } footer: {
                Text("Signing out forgets this host and token on this iPhone.")
            }
        }
        .navigationTitle("Connection")
        .sheet(isPresented: $showsChangeHost) {
            NavigationStack { ChangeHostView() }
        }
    }
}

/// Which model answers, what Marble may do, and what it remembers.
struct MarblesModelsSettingsView: View {
    var body: some View {
        List {
            Section {
                NavigationLink { ModelSettingsView() } label: { Label("Model", systemImage: "cpu") }
                NavigationLink { PermissionsView() } label: { Label("Permissions", systemImage: "hand.raised") }
                NavigationLink { MemoryView() } label: { Label("Memory", systemImage: "brain") }
                NavigationLink { WhatMarbleCanUseView() } label: {
                    Label("What Marble can use", systemImage: "wrench.and.screwdriver")
                }
            } footer: {
                Text("Each Marble's own personality and look are in the Marbles tab.")
            }
        }
        .navigationTitle("Marbles and models")
    }
}

/// Alerts and spoken replies on this iPhone.
struct NotificationSettingsView: View {
    @EnvironmentObject private var store: MarbleStore

    var body: some View {
        Form {
            Section {
                Toggle("Alerts on this iPhone", isOn: Binding(
                    get: { store.localNotificationsEnabled },
                    set: { store.localNotificationsEnabled = $0 }
                ))
                Toggle("Read replies aloud", isOn: Binding(
                    get: { store.voice.enabled },
                    set: { _ in Task { await store.toggleVoice() } }
                ))
            } footer: {
                Text("Marble alerts you to new replies in other chats. With background connection enabled, alerts can also arrive while Marble is in the background.")
            }
        }
        .navigationTitle("Notifications")
    }
}

/// Export and clearing.
struct DataSettingsView: View {
    @EnvironmentObject private var store: MarbleStore
    @State private var confirmClearEveryone = false
    @State private var exportURL: URL?

    var body: some View {
        Form {
            Section {
                Button("Export all chats") {
                    Task { exportURL = await store.exportFile() }
                }
                if let exportURL {
                    ShareLink(item: exportURL) {
                        Label("Share \(exportURL.lastPathComponent)", systemImage: "square.and.arrow.up")
                    }
                }
            }
            Section {
                Button("Clear the Everyone chat", role: .destructive) {
                    confirmClearEveryone = true
                }
            } footer: {
                Text("Clearing Everyone erases what was said there, including Marbles' replies; one-to-one chats are untouched.")
            }
        }
        .navigationTitle("Chats and data")
        .confirmationDialog(
            "Clear the Everyone chat?",
            isPresented: $confirmClearEveryone,
            titleVisibility: .visible
        ) {
            Button("Clear Everyone", role: .destructive) {
                Task { await store.clearEveryone() }
            }
        } message: {
            Text("Everything in the shared chat is erased. Other chats stay.")
        }
    }
}

/// This app's version and the host's facts.
struct AboutSettingsView: View {
    var body: some View {
        List {
            Section {
                MarbleAppVersionRows()
                MarbleHostVersionRow()
                NavigationLink("About this host") { AboutHostView() }
            }
        }
        .navigationTitle("About")
    }
}

private struct MarbleAppVersionRows: View {
    var body: some View {
        LabeledContent("App version", value: versionLabel)
        if let commit = Bundle.main.object(forInfoDictionaryKey: "MarbleGitCommit") as? String, !commit.isEmpty {
            LabeledContent("App commit", value: String(commit.prefix(12)))
        }
    }

    private var versionLabel: String {
        let info = Bundle.main.infoDictionary
        let version = info?["CFBundleShortVersionString"] as? String ?? "dev"
        guard let build = info?["CFBundleVersion"] as? String, !build.isEmpty else { return version }
        return "\(version) (\(build))"
    }
}

private struct MarbleHostVersionRow: View {
    @EnvironmentObject private var store: MarbleStore

    var body: some View {
        if let build = store.hostBuild {
            LabeledContent("Host version", value: "\(build.version) · \(build.gitCommitShort)")
        } else {
            Text("Host version unknown (older host — update it)")
                .foregroundStyle(.secondary)
        }
    }
}

/// Host address + token, validated before it is saved.
struct ChangeHostView: View {
    @EnvironmentObject private var store: MarbleStore
    @Environment(\.dismiss) private var dismiss
    @State private var address = ""
    @State private var token = ""
    @State private var checking = false
    @State private var note: String?

    var body: some View {
        Form {
            Section(header: Text("Host")) {
                TextField("http://100.x.y.z:4783", text: $address)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .keyboardType(.URL)
                SecureField("Access token", text: $token)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
            }
            Section {
                Button(checking ? "Checking…" : "Save") { changeHost() }
                    .disabled(checking)
                if let note {
                    Text(note)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
            } footer: {
                Text("The address is checked against the host before it is saved. The token is kept in the iOS keychain.")
            }
        }
        .navigationTitle("Change host")
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel") { dismiss() }
            }
        }
        .onAppear {
            address = store.endpoint?.baseURL.absoluteString ?? ""
        }
    }

    private func changeHost() {
        checking = true
        let rawAddress = address
        let rawToken = token.trimmingCharacters(in: .whitespacesAndNewlines)
        Task {
            defer { checking = false }
            let (verdict, endpoint) = await MarbleProbe.check(host: rawAddress, token: rawToken)
            if verdict.isReady, let endpoint {
                store.apply(endpoint)
                dismiss()
            }
            note = verdict.message
        }
    }
}

/// How much Marble may do on its own: the three host postures in plain words.
struct PermissionsView: View {
    @EnvironmentObject private var store: MarbleStore

    var body: some View {
        Form {
            Section {
                permissionRow("observe", "Read only")
                permissionRow("workspace", "Ask before changes")
                permissionRow("unattended", "Don't ask")
            } footer: {
                Text(footer)
            }

            Section(header: Text("Always allowed")) {
                if store.control.grants.isEmpty {
                    Text("Nothing is always allowed yet. Allow once inside a chat's approval and it lands here.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                ForEach(store.control.grants) { grant in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(grant.tool)
                            .font(.callout)
                        Text(grant.scopeLabel)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    .swipeActions(edge: .trailing) {
                        Button("Remove", role: .destructive) {
                            Task { await store.revokeGrant(grant.id) }
                        }
                    }
                }
            }

            Section(header: Text("Off-limits folders")) {
                if store.control.deniedPaths.isEmpty {
                    Text("No folders are off-limits.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                ForEach(store.control.deniedPaths, id: \.self) { path in
                    Text(path).font(.callout)
                }
            }
        }
        .navigationTitle("Permissions")
    }

    private var footer: String {
        switch store.control.profile {
        case "observe": return "Marble can read the workspace but never changes it."
        case "unattended": return "Marble acts without asking. Off-limits folders still apply."
        default: return "Marble asks you before each change unless it's always allowed."
        }
    }

    private func permissionRow(_ profile: String, _ title: String) -> some View {
        Button {
            Task { await store.setProfile(profile) }
        } label: {
            HStack {
                Text(title).foregroundStyle(.primary)
                Spacer()
                if store.control.profile == profile {
                    Image(systemName: "checkmark").foregroundStyle(.tint)
                }
            }
        }
    }
}

/// Model choice: empty means the host's engine default.
struct ModelSettingsView: View {
    var body: some View {
        Form {
            Section { ModelPickerRow() }
        }
        .navigationTitle("Model")
    }
}

/// Facts about the host this iPhone talks to, plus a read-only tool list.
struct AboutHostView: View {
    @EnvironmentObject private var store: MarbleStore

    var body: some View {
        Form {
            Section(header: Text("Host")) {
                LabeledContent("Address", value: store.endpoint?.displayHost ?? "not connected")
                MarbleHostVersionRow()
                LabeledContent("Engine", value: store.engineHealth?.health ?? "unknown")
            }
            Section {
                NavigationLink("What Marble can use") { WhatMarbleCanUseView() }
            }
        }
        .navigationTitle("About this host")
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
