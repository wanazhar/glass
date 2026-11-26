import GlassModels
import Speech
import SwiftUI
import UserNotifications

/// Settings submenus. Host status and diagnostics live here instead of in their own tabs.
enum SettingsRoute: String, Hashable, CaseIterable {
    case host, defaults, appearance, notifications, connection, voice, diagnostics, about

    var title: String {
        switch self {
        case .host: "Host"
        case .defaults: "Defaults for new conversations"
        case .appearance: "Appearance"
        case .notifications: "Notifications"
        case .connection: "Background connection"
        case .voice: "Voice"
        case .diagnostics: "Diagnostics"
        case .about: "About"
        }
    }

    var systemImage: String {
        switch self {
        case .host: "desktopcomputer"
        case .defaults: "slider.horizontal.3"
        case .appearance: "textformat.size"
        case .notifications: "bell"
        case .connection: "antenna.radiowaves.left.and.right"
        case .voice: "mic"
        case .diagnostics: "stethoscope"
        case .about: "info.circle"
        }
    }

    @MainActor @ViewBuilder
    var destination: some View {
        switch self {
        case .host: OverviewView()
        case .defaults: HostDefaultsView()
        case .appearance: AppearanceSettingsView()
        case .notifications: NotificationSettingsView()
        case .connection: ConnectionSettingsView()
        case .voice: VoiceSettingsView()
        case .diagnostics: DiagnosticsView()
        case .about: AboutView()
        }
    }
}

struct SettingsView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        List {
            Section("Workspace") {
                NavigationLink(value: SettingsRoute.host) {
                    LabeledContent {
                        Text(hostState)
                    } label: {
                        Label {
                            Text(model.status?.workspaceName ?? "Host")
                        } icon: {
                            Image(systemName: SettingsRoute.host.systemImage)
                        }
                    }
                }
                .accessibilityIdentifier("settings.host")
                link(.defaults)
                link(.diagnostics)
            }
            Section("Preferences") {
                link(.appearance)
                link(.notifications)
                link(.connection)
                link(.voice)
            }
            Section("Glass") {
                link(.about)
            }
        }
        .navigationTitle("Settings")
    }

    private var hostState: String {
        switch model.phase {
        case .connected: "Connected"
        case .unreachable: "Unreachable"
        case .connecting, .unpaired: "Connecting"
        }
    }

    private func link(_ route: SettingsRoute) -> some View {
        NavigationLink(value: route) {
            Label(route.title, systemImage: route.systemImage)
        }
        .accessibilityIdentifier("settings.\(route.rawValue)")
    }
}

/// Host-wide defaults, saved on the host and shared by every client.
struct HostDefaultsView: View {
    @Environment(AppModel.self) private var model
    @State private var hostSettings: HostSettingsModel?

    var body: some View {
        List {
            if let hostSettings {
                Section {
                    ForEach(HostSettingsSection.allCases) { section in
                        NavigationLink {
                            HostSettingsView(section: section, settings: hostSettings)
                        } label: {
                            Label(section.title, systemImage: section.systemImage)
                        }
                        .accessibilityIdentifier("settings.section.\(section.rawValue)")
                    }
                } footer: {
                    Text("Saved on the host and shared by every client. New conversations use them.")
                }
            }
        }
        .navigationTitle("Defaults")
        .onAppear { if hostSettings == nil, model.api != nil { hostSettings = HostSettingsModel(app: model) } }
    }
}

struct AppearanceSettingsView: View {
    @AppStorage(ThemePreference.key) private var theme: ThemePreference = .system
    @AppStorage(ActivityAnimation.key) private var animation: ActivityAnimation = .dots
    @AppStorage(ToolDensity.key) private var density: ToolDensity = .compact

    var body: some View {
        Form {
            Section {
                Picker("Theme", selection: $theme) {
                    ForEach(ThemePreference.allCases) { Text($0.title).tag($0) }
                }
                .accessibilityIdentifier("settings.theme")
                Picker("Activity animation", selection: $animation) {
                    ForEach(ActivityAnimation.allCases) { Text($0.title).tag($0) }
                }
                .accessibilityIdentifier("settings.animation")
                Picker("Work details", selection: $density) {
                    ForEach(ToolDensity.allCases) { Text($0.title).tag($0) }
                }
                .accessibilityIdentifier("settings.tools")
            } footer: {
                Text("Saved on this iPhone. Reduce Motion always keeps the activity indicator still.")
            }
        }
        .navigationTitle("Appearance")
    }
}

struct NotificationSettingsView: View {
    @AppStorage(Attention.key) private var notifications = false
    @State private var denied = false

    var body: some View {
        Form {
            Section {
                Toggle("Alerts", isOn: Binding(get: { notifications }, set: { on in
                    Task {
                        notifications = await Attention.set(on)
                        await refresh()
                    }
                }))
                .accessibilityIdentifier("settings.notifications")
            } footer: {
                Text("Alerts you when a reply is ready or Glass needs an answer in a conversation you aren't looking at. Alerts come only while Glass is running. iPhone pauses Glass soon after you leave it, and older replies never alert.")
            }
            if denied {
                Section {
                    Button("Open iPhone Settings") {
                        if let url = URL(string: UIApplication.openNotificationSettingsURLString) { UIApplication.shared.open(url) }
                    }
                } footer: {
                    Text("Notifications are turned off for Glass in iPhone Settings.")
                }
            }
        }
        .navigationTitle("Notifications")
        .task { await refresh() }
    }

    private func refresh() async {
        denied = await UNUserNotificationCenter.current().notificationSettings().authorizationStatus == .denied
    }
}

struct ConnectionSettingsView: View {
    @AppStorage(KeepAlive.key) private var keepAlive = false
    @State private var denied = false

    var body: some View {
        Form {
            Section {
                Toggle("Keep connection alive in background", isOn: Binding(get: { keepAlive }, set: { on in
                    Task {
                        keepAlive = await KeepAlive.shared.set(on)
                        denied = on && !keepAlive
                    }
                }))
                .accessibilityIdentifier("settings.keepAlive")
            } footer: {
                Text("""
                iPhone normally pauses Glass soon after you leave it. With this on, Glass stays connected while a \
                conversation or the desktop is open, so replies and alerts keep arriving. To stay running it uses \
                iPhone's least precise location, so the status bar shows a location indicator, and the Lock Screen \
                shows the connection with a Disconnect button. Glass never stores or sends your location. It uses \
                more battery, and stops as soon as nothing is connected.
                """)
            }
            if denied {
                Section {
                    Button("Open iPhone Settings") {
                        if let url = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(url) }
                    }
                } footer: {
                    Text("Location is turned off for Glass in iPhone Settings, so it can't stay connected in the background.")
                }
            }
        }
        .navigationTitle("Background connection")
    }
}

struct VoiceSettingsView: View {
    @AppStorage(VoicePreference.localeKey) private var locale = ""
    @AppStorage(VoicePreference.onDeviceKey) private var onDevice = true

    private var locales: [Locale] {
        SFSpeechRecognizer.supportedLocales()
            .sorted { VoicePreference.name($0) < VoicePreference.name($1) }
    }

    var body: some View {
        Form {
            Section {
                Picker("Language", selection: $locale) {
                    Text("Same as iPhone").tag("")
                    ForEach(locales, id: \.identifier) { Text(VoicePreference.name($0)).tag($0.identifier) }
                }
                .accessibilityIdentifier("settings.voice.language")
                Toggle("Keep audio on this iPhone", isOn: $onDevice)
                    .accessibilityIdentifier("settings.voice.onDevice")
            } footer: {
                Text("Dictation in the message box. With audio kept on this iPhone, languages without on-device support fall back to Apple’s speech service.")
            }
        }
        .navigationTitle("Voice")
    }
}

/// Dictation preferences, read by `Dictation` when it starts.
enum VoicePreference {
    static let localeKey = "glass.voice.locale"
    static let onDeviceKey = "glass.voice.onDevice"

    static var recognizer: SFSpeechRecognizer? {
        let id = UserDefaults.standard.string(forKey: localeKey) ?? ""
        return id.isEmpty ? SFSpeechRecognizer() : SFSpeechRecognizer(locale: Locale(identifier: id))
    }

    static var prefersOnDevice: Bool { UserDefaults.standard.object(forKey: onDeviceKey) as? Bool ?? true }

    static func name(_ locale: Locale) -> String {
        Locale.current.localizedString(forIdentifier: locale.identifier) ?? locale.identifier
    }
}

struct AboutView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        List {
            Section {
                LabeledContent("Version", value: model.info.versionLabel)
                LabeledContent("Client protocol", value: "v\(GlassClientKitVersion.supportedProtocolVersion)")
            }
            Section {
                Text("Math is typeset with SwiftMath and diagrams are drawn with Mermaid. Both work offline and use the MIT license.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            } header: {
                Text("Open source")
            }
        }
        .navigationTitle("About")
    }
}
