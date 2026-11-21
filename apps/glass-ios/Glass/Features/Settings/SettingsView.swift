import GlassModels
import Speech
import SwiftUI
import UserNotifications

/// Settings submenus. Host status and diagnostics live here instead of in their own tabs.
enum SettingsRoute: String, Hashable, CaseIterable {
    case host, defaults, appearance, notifications, voice, diagnostics, about

    var title: String {
        switch self {
        case .host: "Host"
        case .defaults: "Defaults for new conversations"
        case .appearance: "Appearance"
        case .notifications: "Notifications"
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
            Section {
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
            Section {
                link(.appearance)
                link(.notifications)
                link(.voice)
            }
            Section {
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
                Picker("Tool cards", selection: $density) {
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
                Text("Alerts you when a reply is ready or Glass needs an answer while the app is in the background. iPhone pauses the connection soon after you leave the app, so alerts stop then.")
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
                Text("Math is drawn with KaTeX and diagrams with Mermaid, both bundled with the app and used offline under the MIT license.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            } header: {
                Text("Open source")
            }
        }
        .navigationTitle("About")
    }
}
