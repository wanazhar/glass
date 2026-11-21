import SwiftUI

/// Settings: the connection, what Marble may do, this iPhone's alerts, and the
/// host's facts. No hidden features — everything else lives on the host.
struct SettingsView: View {
    @EnvironmentObject private var store: MarbleStore
    @State private var showsChangeHost = false

    var body: some View {
        Form {
            Section {
                LabeledContent("Host", value: store.endpoint?.displayHost ?? "not connected")
                LabeledContent("Status", value: store.statusLabel)
                Button("Change host…") { showsChangeHost = true }
                Button("Sign out", role: .destructive) { store.signOut() }
            } header: {
                Text("Connection")
            } footer: {
                Text("Signing out forgets this host and token on this iPhone.")
            }

            Section(header: Text("Assistant")) {
                NavigationLink("Model") { ModelSettingsView() }
                NavigationLink("Permissions") { PermissionsView() }
                NavigationLink("Memory") { MemoryView() }
            }

            Section {
                Toggle("Alerts on this iPhone", isOn: alertsBinding)
                Toggle("Read replies aloud", isOn: voiceBinding)
            } header: {
                Text("On this iPhone")
            } footer: {
                Text("Alerts only arrive while Marble is open or recently used — remote push isn't available with this signing.")
            }

            Section(header: Text("Data")) {
                Button("Export all chats") {
                    Task { exportURL = await store.exportFile() }
                }
                if let exportURL {
                    ShareLink(item: exportURL) {
                        Label("Share \(exportURL.lastPathComponent)", systemImage: "square.and.arrow.up")
                    }
                }
            }

            Section(header: Text("About")) {
                LabeledContent("Version") { Text(versionLabel) }
                NavigationLink("About this host") { AboutHostView() }
            }
        }
        .navigationTitle("Settings")
        .sheet(isPresented: $showsChangeHost) {
            NavigationStack { ChangeHostView() }
        }
    }

    @State private var exportURL: URL?

    private var alertsBinding: Binding<Bool> {
        Binding(
            get: { store.localNotificationsEnabled },
            set: { store.localNotificationsEnabled = $0 }
        )
    }

    private var voiceBinding: Binding<Bool> {
        Binding(
            get: { store.voice.enabled },
            set: { _ in Task { await store.toggleVoice() } }
        )
    }

    private var versionLabel: String {
        let info = Bundle.main.infoDictionary
        let version = info?["CFBundleShortVersionString"] as? String ?? "dev"
        let build = info?["CFBundleVersion"] as? String ?? ""
        let commit = info?["MarbleGitCommit"] as? String ?? "dev"
        return "\(version) (\(build)) · \(commit)"
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
                LabeledContent("Host version", value: store.hostVersion ?? "unknown")
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

