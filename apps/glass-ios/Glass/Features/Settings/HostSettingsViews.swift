import GlassModels
import SwiftUI

/// Host-wide defaults, grouped like the web UI: General, Connections, Tools & skills, Instructions, Advanced.
enum HostSettingsSection: String, CaseIterable, Identifiable, Hashable {
    case general, connections, tools, instructions, advanced
    var id: String { rawValue }
    var title: String {
        switch self {
        case .general: "General"
        case .connections: "Connections"
        case .tools: "Tools & skills"
        case .instructions: "Instructions"
        case .advanced: "Advanced"
        }
    }
    var systemImage: String {
        switch self {
        case .general: "slider.horizontal.3"
        case .connections: "key"
        case .tools: "wrench.and.screwdriver"
        case .instructions: "doc.text"
        case .advanced: "lock.shield"
        }
    }
}

struct HostSettingsView: View {
    var section: HostSettingsSection
    @Bindable var settings: HostSettingsModel

    var body: some View {
        Form {
            if settings.conflict || (settings.failed && settings.feedback != nil) {
                Section {
                    Label(settings.feedback ?? "", systemImage: "exclamationmark.triangle")
                        .foregroundStyle(.red)
                        .accessibilityIdentifier("settings.feedback")
                    if settings.conflict {
                        Button("Refresh Settings") { Task { await settings.load() } }
                            .accessibilityIdentifier("settings.refresh")
                    }
                }
            }
            if let snapshot = settings.snapshot {
                switch section {
                case .general: GeneralSettings(settings: settings, snapshot: snapshot)
                case .connections: ConnectionSettings(settings: settings, snapshot: snapshot)
                case .tools: ToolSettings(settings: settings, snapshot: snapshot)
                case .instructions: InstructionSettings(settings: settings, snapshot: snapshot)
                case .advanced: AdvancedSettings(settings: settings, snapshot: snapshot)
                }
                Section {
                } footer: {
                    Text(snapshot.applies.isEmpty ? "Changes apply to new conversations on this host." : snapshot.applies)
                }
            }
        }
        .disabled(settings.saving)
        .overlay { if settings.snapshot == nil, settings.loading { ProgressView() } }
        .refreshable { await settings.load() }
        .navigationTitle(section.title)
        .toolbar {
            if settings.saving {
                ToolbarItem(placement: .status) { ProgressView() }
            } else if let feedback = settings.feedback, !settings.failed {
                ToolbarItem(placement: .status) {
                    Text(feedback).font(.footnote).foregroundStyle(.secondary)
                        .accessibilityIdentifier("settings.saved")
                }
            }
        }
        .task { if settings.snapshot == nil { await settings.load() } }
        .accessibilityIdentifier("settings.\(section.rawValue)")
    }
}

/// A text preference that keeps its unsaved value in the model's drafts and saves on Return or Save.
struct PreferenceField: View {
    var title: String
    var key: String
    var saved: String
    var kind: CommandField.Kind = .text
    @Bindable var settings: HostSettingsModel
    var save: (String) -> SettingsChange

    private var value: Binding<String> {
        Binding { settings.draft(key, saved) } set: { settings.drafts[key] = $0 == saved ? nil : $0 }
    }
    private var edited: Bool { settings.drafts[key] != nil }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if kind == .textarea {
                Text(title).font(.footnote).foregroundStyle(.secondary)
                TextField(title, text: value, axis: .vertical).lineLimit(4...16)
                    .accessibilityIdentifier("settings.field.\(key)")
            } else {
                LabeledContent(title) {
                    TextField(title, text: value)
                        .multilineTextAlignment(.trailing)
                        .keyboardType(kind == .number ? .numbersAndPunctuation : kind == .url ? .URL : .default)
                        .onSubmit(commit)
                        .accessibilityIdentifier("settings.field.\(key)")
                }
            }
            if edited {
                HStack {
                    Button("Revert") { settings.drafts[key] = nil }
                    Spacer()
                    Button("Save", action: commit).bold()
                        .accessibilityIdentifier("settings.save.\(key)")
                }
                .buttonStyle(.borderless)
            }
        }
        .textInputAutocapitalization(kind == .textarea ? .sentences : .never)
        .autocorrectionDisabled(kind != .textarea)
    }

    private func commit() {
        guard edited else { return }
        let change = save(settings.draft(key, saved))
        Task { await settings.save(change, draft: key) }
    }
}

private func number(_ text: String) -> JSONValue {
    let trimmed = text.trimmingCharacters(in: .whitespaces)
    return Double(trimmed).map(JSONValue.number) ?? (trimmed.isEmpty ? .null : .string(trimmed))
}

private func display(_ value: Double?) -> String {
    guard let value else { return "" }
    return value.rounded() == value ? String(Int64(value)) : String(value)
}

private struct GeneralSettings: View {
    @Bindable var settings: HostSettingsModel
    var snapshot: SettingsSnapshot

    var body: some View {
        let defaults = snapshot.defaults
        let active = snapshot.connections.first { $0.id == snapshot.activeConnection }
        Section("Model") {
            if !snapshot.connections.isEmpty {
                Picker("Default connection", selection: Binding {
                    snapshot.activeConnection ?? ""
                } set: { id in Task { await settings.save(.useConnection(id: id)) } }) {
                    Text("Global provider").tag("")
                    ForEach(snapshot.connections) { Text($0.name).tag($0.id) }
                }
                .accessibilityIdentifier("settings.connection")
            }
            if let active {
                PreferenceField(title: "Default model", key: "connection-model-\(active.id)", saved: active.model,
                                settings: settings) { .connectionModel(id: active.id, model: $0) }
                LabeledContent("Endpoint", value: active.endpoint)
            } else {
                Picker("Provider", selection: Binding { defaults.provider } set: { value in
                    Task { await settings.save(.preference(key: "provider", value: .string(value))) }
                }) {
                    ForEach(SettingsChange.providers, id: \.id) { Text($0.title).tag($0.id) }
                }
                PreferenceField(title: "API endpoint", key: "base_url", saved: defaults.baseURL, kind: .url,
                                settings: settings) { .preference(key: "base_url", value: .string($0)) }
                PreferenceField(title: "Default model", key: "model", saved: defaults.model,
                                settings: settings) { .preference(key: "model", value: .string($0)) }
            }
        }
        Section {
            PreferenceField(title: "Maximum output tokens", key: "max_tokens", saved: String(defaults.maxTokens), kind: .number,
                            settings: settings) { .preference(key: "max_tokens", value: number($0)) }
            PreferenceField(title: "Reasoning allowance", key: "reasoning_budget",
                            saved: defaults.reasoningBudget.map(String.init) ?? "", kind: .number,
                            settings: settings) { .preference(key: "reasoning_budget", value: number($0)) }
            Toggle("Adaptive pacing", isOn: Binding { defaults.adaptivePacing } set: { on in
                Task { await settings.save(.preference(key: "adaptive_pacing", value: .bool(on))) }
            })
            Toggle("Image input", isOn: Binding { defaults.imageInput } set: { on in
                Task { await settings.save(.preference(key: "image_input", value: .bool(on))) }
            })
            .accessibilityIdentifier("settings.image_input")
        } header: {
            Text("Responses")
        } footer: {
            Text("Leave the reasoning allowance empty to use the provider default. Enable image input only for models that support images.")
        }
        Section {
            PreferenceField(title: "Spending limit (USD)", key: "cost_limit", saved: display(defaults.costLimit), kind: .number,
                            settings: settings) { .preference(key: "cost_limit", value: number($0)) }
        } header: {
            Text("Spending")
        } footer: {
            Text("Per new conversation. Empty means no limit; existing conversations keep their own limit.")
        }
    }
}

private struct ConnectionSettings: View {
    @Bindable var settings: HostSettingsModel
    var snapshot: SettingsSnapshot
    @Environment(AppModel.self) private var app
    @Environment(\.openURL) private var openURL
    @State private var removing: SettingsSnapshot.Connection?
    @State private var adding = false
    @State private var signingIn = false

    var body: some View {
        Section {
            if snapshot.connections.isEmpty {
                Text("No saved connections. Add a provider below.").foregroundStyle(.secondary)
            }
            ForEach(snapshot.connections) { account in
                VStack(alignment: .leading, spacing: 4) {
                    HStack {
                        Text(account.name).font(.headline)
                        if account.id == snapshot.activeConnection {
                            Text("Default").font(.caption).foregroundStyle(.secondary)
                        }
                    }
                    Text("\(account.provider) · \(account.endpoint)").font(.footnote).foregroundStyle(.secondary)
                }
                PreferenceField(title: "Model", key: "connection-model-\(account.id)", saved: account.model,
                                settings: settings) { .connectionModel(id: account.id, model: $0) }
                if account.id != snapshot.activeConnection {
                    Button("Use as Default") { Task { await settings.save(.useConnection(id: account.id)) } }
                }
                Button("Remove Connection", role: .destructive) { removing = account }
            }
        } header: {
            Text("Saved connections")
        } footer: {
            Text("Shared across this host. Credentials stay on the host.")
        }
        Section {
            Button("Add Connection") { adding = true }
                .accessibilityIdentifier("settings.addConnection")
            Button("Connect ChatGPT") {
                Task { if await settings.save(.startSignIn) { signingIn = true } }
            }
            if signingIn, let signIn = settings.snapshot?.signIn, let url = URL(string: signIn.authorizeURL) {
                Button("Continue Sign-in") { openURL(url) }
            }
        } footer: {
            if app.endpoint?.isSecure == false {
                Text("Credentials are only sent over HTTPS. Connect through Tailscale HTTPS to add one.")
            }
        }
        .disabled(app.endpoint?.isSecure == false)
        .confirmationDialog("Remove this connection?", isPresented: Binding { removing != nil } set: { if !$0 { removing = nil } },
                            titleVisibility: .visible, presenting: removing) { account in
            Button("Remove Connection", role: .destructive) {
                Task { await settings.save(.removeConnection(id: account.id)) }
            }
        } message: { _ in
            Text("Conversations using it may need to reconnect.")
        }
        .sheet(isPresented: $adding) { ConnectionForm(settings: settings, callback: nil) }
        .sheet(isPresented: Binding { signingIn && settings.snapshot?.signIn != nil } set: { signingIn = $0 }) {
            ConnectionForm(settings: settings, callback: settings.snapshot?.signIn?.attempt ?? "")
        }
    }
}

/// Adds a connection with an API key, or completes a ChatGPT sign-in with its callback URL.
private struct ConnectionForm: View {
    @Bindable var settings: HostSettingsModel
    /// The sign-in attempt when completing a sign-in.
    var callback: String?
    @Environment(\.dismiss) private var dismiss
    @State private var label = ""
    @State private var provider = "openai"
    @State private var endpoint = SettingsChange.defaultEndpoint(provider: "openai")
    @State private var model = ""
    @State private var secret = ""

    var body: some View {
        NavigationStack {
            Form {
                TextField("Connection name", text: $label)
                if callback == nil {
                    Picker("Provider", selection: $provider) {
                        ForEach(SettingsChange.providers, id: \.id) { Text($0.title).tag($0.id) }
                    }
                    .onChange(of: provider) { endpoint = SettingsChange.defaultEndpoint(provider: provider) }
                    TextField("API endpoint", text: $endpoint).keyboardType(.URL)
                }
                TextField("Model", text: $model)
                Section {
                    SecureField(callback == nil ? "API key" : "Final callback URL", text: $secret)
                        .textContentType(.newPassword)
                } footer: {
                    Text(callback == nil ? "Sent once over HTTPS and stored on the host, not on this iPhone."
                                         : "After signing in, paste the final localhost URL from the browser.")
                }
                if settings.failed, let feedback = settings.feedback {
                    Text(feedback).foregroundStyle(.red)
                }
            }
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            .navigationTitle(callback == nil ? "Add Connection" : "Complete Sign-in")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { secret = ""; dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") { Task { await save() } }
                        .disabled(model.isEmpty || secret.isEmpty || settings.saving)
                }
            }
        }
        .onDisappear { secret = "" }
    }

    private func save() async {
        let change: SettingsChange = if let callback {
            .completeSignIn(attempt: callback.isEmpty ? nil : callback, callbackURL: secret, model: model, label: label)
        } else {
            .addConnection(label: label, provider: provider, endpoint: endpoint, model: model, secret: secret)
        }
        let saved = await settings.save(change)
        secret = ""
        if saved { dismiss() }
    }
}

private struct ToolSettings: View {
    @Bindable var settings: HostSettingsModel
    var snapshot: SettingsSnapshot

    var body: some View {
        if !snapshot.availableMods.isEmpty {
            Section("Mods") {
                ForEach(snapshot.availableMods, id: \.self) { name in
                    Toggle(name, isOn: Binding { snapshot.defaults.mods.contains(name) } set: { on in
                        let mods = snapshot.defaults.mods.filter { $0 != name } + (on ? [name] : [])
                        Task { await settings.save(.preference(key: "mods", value: .array(mods.map(JSONValue.string)))) }
                    })
                }
            }
        }
        Section("Extensions") {
            if snapshot.extensions.isEmpty { Text("No extensions installed on this host.").foregroundStyle(.secondary) }
            ForEach(snapshot.extensions) { bundle in
                Toggle(isOn: Binding { bundle.enabled } set: { on in
                    Task { await settings.save(.extensionEnabled(id: bundle.id, enabled: on)) }
                }) {
                    Text(bundle.name)
                    if !bundle.description.isEmpty { Text(bundle.description) }
                }
            }
        }
        Section {
            PreferenceField(title: "Additional skills folder", key: "skills_dir", saved: snapshot.defaults.skillsDir ?? "",
                            settings: settings) { .preference(key: "skills_dir", value: .string($0)) }
            ForEach(snapshot.skills) { skill in
                VStack(alignment: .leading, spacing: 2) {
                    Text(skill.name)
                    if !skill.description.isEmpty { Text(skill.description).font(.footnote).foregroundStyle(.secondary) }
                }
            }
        } header: {
            Text("Skills")
        } footer: {
            Text("An existing folder on the host. Leave empty to use no additional folder.")
        }
    }
}

private struct InstructionSettings: View {
    @Bindable var settings: HostSettingsModel
    var snapshot: SettingsSnapshot
    @State private var newProfile = ""
    @State private var assigning: SettingsSnapshot.Profile?
    @State private var workspace = ""
    @State private var deleting: SettingsSnapshot.Profile?

    var body: some View {
        Section {
            PreferenceField(title: "Instructions", key: "system_prompt", saved: snapshot.defaults.systemPrompt, kind: .textarea,
                            settings: settings) { .preference(key: "system_prompt", value: .string($0)) }
        } header: {
            Text("Shared instructions")
        } footer: {
            Text("Sent with every new conversation on this host.")
        }
        ForEach(snapshot.profiles) { profile in
            Section {
                PreferenceField(title: "Profile model", key: "profile-model-\(profile.name)", saved: profile.model,
                                settings: settings) { .updateProfile(name: profile.name, model: $0) }
                PreferenceField(title: "Profile instructions", key: "profile-instructions-\(profile.name)",
                                saved: profile.instructions, kind: .textarea,
                                settings: settings) { .updateProfile(name: profile.name, instructions: $0) }
                Button("Assign Workspace") { workspace = ""; assigning = profile }
                Button("Delete Profile", role: .destructive) { deleting = profile }
            } header: {
                Text(profile.name)
            } footer: {
                Text(profile.workspaces.isEmpty ? "Not assigned to a workspace." : profile.workspaces.joined(separator: " · "))
            }
        }
        Section("New profile") {
            TextField("Profile name", text: $newProfile)
                .textInputAutocapitalization(.never)
                .onSubmit(create)
            Button("Create Profile", action: create).disabled(newProfile.isEmpty)
        }
        .alert("Assign workspace", isPresented: Binding { assigning != nil } set: { if !$0 { assigning = nil } },
               presenting: assigning) { profile in
            TextField("Workspace path", text: $workspace).textInputAutocapitalization(.never)
            Button("Assign") { Task { await settings.save(.assignProfile(name: profile.name, workspace: workspace)) } }
            Button("Cancel", role: .cancel) {}
        }
        .confirmationDialog("Delete this profile?", isPresented: Binding { deleting != nil } set: { if !$0 { deleting = nil } },
                            titleVisibility: .visible, presenting: deleting) { profile in
            Button("Delete Profile", role: .destructive) { Task { await settings.save(.deleteProfile(name: profile.name)) } }
        }
    }

    private func create() {
        let name = newProfile.trimmingCharacters(in: .whitespaces)
        guard !name.isEmpty else { return }
        Task { if await settings.save(.createProfile(name: name)) { newProfile = "" } }
    }
}

private struct AdvancedSettings: View {
    @Bindable var settings: HostSettingsModel
    var snapshot: SettingsSnapshot

    var body: some View {
        Section {
            Picker("Execution profile", selection: Binding { snapshot.defaults.permissionProfile } set: { value in
                Task { await settings.save(.preference(key: "permission_profile", value: .string(value))) }
            }) {
                ForEach(SettingsChange.permissionProfiles, id: \.id) { Text($0.title).tag($0.id) }
            }
            Picker("Shell confinement", selection: Binding { snapshot.defaults.sandbox } set: { value in
                Task { await settings.save(.preference(key: "sandbox", value: .string(value))) }
            }) {
                ForEach(SettingsChange.sandboxes, id: \.id) { Text($0.title).tag($0.id) }
            }
            .disabled(snapshot.defaults.sandboxManaged)
        } header: {
            Text("Execution defaults")
        } footer: {
            Text(snapshot.defaults.sandboxManaged
                 ? "Shell confinement is managed by the host administrator. Existing conversations keep their current settings."
                 : "Applies when a new conversation starts. Existing conversations keep their current settings.")
        }
        Section("Settings file") {
            LabeledContent("Scope", value: snapshot.scope)
            LabeledContent("Revision", value: String(snapshot.revision.prefix(12)))
        }
    }
}
