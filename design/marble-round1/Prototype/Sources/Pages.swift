import SwiftUI

struct DetailPage: View {
    let screen: Screen
    let person: Person
    @Environment(\.colorScheme) private var scheme
    @Environment(\.dismiss) private var dismiss
    @Environment(\.dynamicTypeSize) private var type
    @State private var editing: String
    @State private var draftName = "Jules"
    @State private var personality = "Bring a few creative ideas, then help me choose one."
    @State private var form: Form = .square
    @State private var hue = Hue.all[2]
    @State private var paused = false
    @State private var stopped = false
    @State private var saved = false
    @State private var search = "river"
    var p: Palette { Palette(dark: scheme == .dark) }

    init(screen: Screen, person: Person) {
        self.screen = screen
        self.person = person
        _editing = State(initialValue: screen == .confirm ? "I prefer a quiet walk before lunch." : "Keep Saturday afternoons free.")
    }

    var title: String {
        switch screen {
        case .profile: "About \(person.name)"
        case .memory: "Memory"
        case .edit: "Edit memory"
        case .confirm: "Remember this?"
        case .new: "New Marble"
        case .activity: "Activity"
        case .settings: "Settings"
        case .threads: "Threads"
        case .search: "Find in chat"
        default: "Marble"
        }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                switch screen {
                case .profile: profile
                case .memory: memory
                case .edit, .confirm: memoryEditor
                case .new: newMarble
                case .activity: activity
                case .settings: settings
                case .threads: threads
                case .search: searchPage
                default: EmptyView()
                }
            }
            .frame(maxWidth: 620).padding(.horizontal, 22).padding(.vertical, 18)
            .frame(maxWidth: .infinity)
        }
        .background(p.paper.ignoresSafeArea())
        .navigationTitle(title)
        .navigationBarTitleDisplayMode(.inline)
    }

    var profile: some View {
        VStack(alignment: .leading, spacing: 18) {
            face(person, 88)
            heading(person.name, person.note)
            VStack(alignment: .leading, spacing: 8) {
                Text("Personality").font(.headline)
                Text(profileNote).foregroundStyle(p.muted)
            }.surface(p)
            NavigationLink(value: Screen.memory) {
                LineItem(title: "Memory", detail: "Only \(person.name) and shared with everyone", icon: "bookmark")
                    .surface(p)
            }.buttonStyle(.plain)
            DisclosureGroup("Look") {
                Text("\(person.form.label) · \(person.hue.name)").foregroundStyle(p.muted).padding(.top, 8)
            }.surface(p)
            DisclosureGroup("Workspace and abilities") {
                Text("Uses your host workspace. Voice stays on this device.").foregroundStyle(p.muted).padding(.top, 8)
            }.surface(p)
        }
    }

    var profileNote: String {
        switch person {
        case .ben: "Calm and observant. Find the quiet option and tell me what makes it worth choosing."
        case .mina: "Playful and curious. Offer a fresh angle when I get stuck."
        default: "Practical and gentle. Help me make a short plan without filling the whole day."
        }
    }

    var memory: some View {
        VStack(alignment: .leading, spacing: 18) {
            heading("What \(person.name) remembers", "You can edit or forget anything here.")
            section("Only \(person.name)") {
                NavigationLink(value: Screen.edit) {
                    LineItem(title: "Keep Saturday afternoons free.", detail: "Saved yesterday", icon: "bookmark")
                }.buttonStyle(.plain)
            }
            section("Shared with everyone") {
                LineItem(title: "Saturday walks start after coffee.", detail: "All Marbles can use this", icon: "person.2", arrow: false)
            }
            section("Suggested by \(person.name)") {
                RequestCard(kind: .memory, onReview: { review = true })
            }
        }
        .navigationDestination(isPresented: $review) { DetailPage(screen: .confirm, person: person) }
    }

    @State private var review = false
    var memoryEditor: some View {
        VStack(alignment: .leading, spacing: 18) {
            heading(screen == .confirm ? "\(person.name) can keep this" : "Change a memory",
                    screen == .confirm ? "Check the wording before it’s saved." : "Editing the wording won’t change who can see it.")
            VStack(alignment: .leading, spacing: 10) {
                Label("Only \(person.name)", systemImage: "lock").font(.subheadline.weight(.semibold)).foregroundStyle(p.fig)
                Divider().overlay(p.line)
                TextField("Memory", text: $editing, axis: .vertical).lineLimit(3...5)
            }.surface(p)
            GlassEffectContainer(spacing: 12) {
                VStack(alignment: .leading, spacing: 12) {
                    Action(text: screen == .confirm ? "Save to \(person.name)’s memory" : "Save changes", prominent: true) { saved = true }
                    Action(text: screen == .confirm ? "Not now" : "Cancel") { dismiss() }
                }
            }
            if saved { Label("Saved", systemImage: "checkmark.circle").foregroundStyle(p.good) }
        }
    }

    var newMarble: some View {
        VStack(alignment: .leading, spacing: 15) {
            HStack(spacing: 14) {
                Avatar(form: form, hue: hue, size: 58)
                VStack(alignment: .leading, spacing: 3) {
                    Text(draftName.isEmpty ? "Your new Marble" : draftName).font(.title2.weight(.semibold))
                    Text("A voice and memory of their own").font(.subheadline).foregroundStyle(p.muted)
                }
            }
            field("Name") { TextField("Name", text: $draftName) }
            field("Personality") {
                TextField("How should they talk with you?", text: $personality, axis: .vertical)
                Menu("Try a starter") {
                    Button("A practical planner") { personality = "Help me turn a big idea into a short list." }
                    Button("A patient listener") { personality = "Give me time to think and ask one question at a time." }
                    Button("A creative friend") { personality = "Bring a few creative ideas, then help me choose one." }
                }.font(.subheadline.weight(.semibold))
            }
            VStack(alignment: .leading, spacing: 8) {
                Text("Look").font(.headline)
                let columns = Array(repeating: GridItem(.flexible(minimum: 42), spacing: 4), count: type.isAccessibilitySize ? 3 : 6)
                LazyVGrid(columns: columns, spacing: 4) {
                    ForEach(Form.allCases, id: \.self) { option in
                        Button { form = option } label: {
                            Avatar(form: option, hue: hue, size: 25)
                                .frame(width: 44, height: 44)
                                .overlay { RoundedRectangle(cornerRadius: 10).strokeBorder(option == form ? p.fig : p.line, lineWidth: option == form ? 2 : 1) }
                        }.buttonStyle(.plain).accessibilityLabel(option.label)
                            .accessibilityAddTraits(option == form ? .isSelected : [])
                    }
                }
                LazyVGrid(columns: columns, spacing: 4) {
                    ForEach(Hue.all) { option in
                        Button { hue = option } label: {
                            Circle().fill(p.tone(option.light, option.dark)).frame(width: 24, height: 24)
                                .overlay { if option == hue { Image(systemName: "checkmark").font(.caption.bold()).foregroundStyle(Color(hex: 0x282022)) } }
                                .frame(width: 44, height: 44)
                        }.buttonStyle(.plain).accessibilityLabel(option.name)
                            .accessibilityAddTraits(option == hue ? .isSelected : [])
                    }
                }
            }.surface(p, padding: 12)
            Action(text: saved ? "Marble created" : "Create Marble", prominent: true) { saved = true }
        }
    }

    var activity: some View {
        VStack(alignment: .leading, spacing: 16) {
            heading("What’s happening", "Requests come here when a Marble needs you.")
            section("Needs you") { RequestCard(kind: .approval) }
            section("Under way") {
                HStack(spacing: 10) {
                    face(.ben, 32, working: !stopped)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(stopped ? "Ben stopped" : "Ben is looking for a route").font(.subheadline.weight(.semibold))
                        Text("Everyone").font(.footnote).foregroundStyle(p.muted)
                    }
                }.surface(p, padding: 11, radius: 14)
            }
            GlassEffectContainer(spacing: 8) {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 8) { pause; halt }
                    VStack(alignment: .leading, spacing: 8) { pause; halt }
                }
            }
            section("Recent decisions") {
                Label("Ada saved a memory · Yesterday", systemImage: "checkmark.circle")
                    .font(.subheadline).foregroundStyle(p.muted)
            }
        }
    }

    var pause: some View { Action(text: paused ? "Resume new work" : "Pause new work", icon: "pause") { paused.toggle() } }
    var halt: some View { Action(text: "Stop everything", icon: "stop") { stopped = true } }

    var settings: some View {
        VStack(alignment: .leading, spacing: 18) {
            VStack(alignment: .leading, spacing: 6) {
                Label("Connected", systemImage: "checkmark.circle.fill").font(.subheadline.weight(.semibold)).foregroundStyle(p.good)
                Text("Your Marble host").font(.title2.weight(.semibold))
                Text("studio.example.ts.net:4783").font(.subheadline).foregroundStyle(p.muted)
                NavigationLink(value: Screen.pairing) {
                    Text("Connection settings").font(.subheadline.weight(.semibold)).frame(minHeight: 44)
                }
            }.surface(p)
            section("Preferences") {
                rows([
                    ("Schedules & quiet hours", "clock"), ("Notifications", "bell"),
                    ("Voice", "waveform"), ("Model & permissions", "lock.shield"),
                    ("Background & Live Activity", "iphone")
                ])
            }
            section("Your data") {
                rows([("Export & clear Everyone", "square.and.arrow.up"), ("About Marble", "info.circle")])
            }
        }
    }

    var threads: some View {
        VStack(alignment: .leading, spacing: 18) {
            heading("Side conversations", "Follow an idea without interrupting the chat.")
            LineItem(title: "A quieter way to the river", detail: "2 replies · Today", icon: "bubble.left")
                .surface(p)
            LineItem(title: "Coffee before the walk", detail: "1 reply · Yesterday", icon: "bubble.left")
                .surface(p)
        }
    }

    var searchPage: some View {
        VStack(alignment: .leading, spacing: 18) {
            Label {
                TextField("Find in chat", text: $search)
            } icon: { Image(systemName: "magnifyingglass").foregroundStyle(p.fig) }
                .surface(p)
            section("1 match in this chat") {
                LineItem(title: "Coffee near the river", detail: "\(person.name) · Today", icon: "bubble.left")
                    .surface(p)
            }
        }
    }

    func heading(_ title: String, _ note: String) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(title).font(.largeTitle.weight(.semibold)).tracking(-0.7)
            Text(note).font(.subheadline).foregroundStyle(p.muted)
        }
    }

    func section<V: View>(_ title: String, @ViewBuilder content: () -> V) -> some View {
        VStack(alignment: .leading, spacing: 8) { Text(title).font(.headline); content() }
    }

    func field<V: View>(_ name: String, @ViewBuilder content: () -> V) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(name).font(.subheadline.weight(.semibold)).foregroundStyle(p.fig)
            content()
        }.surface(p, padding: 12, radius: 14)
    }

    func rows(_ items: [(String, String)]) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            ForEach(items, id: \.0) { title, icon in
                DisclosureGroup {
                    Text(settingsDetail(title)).font(.subheadline).foregroundStyle(p.muted).padding(.bottom, 8)
                } label: { LineItem(title: title, icon: icon, arrow: false) }
                if title != items.last?.0 { Divider().overlay(p.line).padding(.leading, 43) }
            }
        }.surface(p, padding: 10)
    }

    func settingsDetail(_ title: String) -> String {
        switch title {
        case "Schedules & quiet hours": "Choose when Marbles can run reminders and when notifications should wait."
        case "Notifications": "Choose which requests and replies can alert you."
        case "Voice": "Dictation and read-aloud stay on this device."
        case "Model & permissions": "Choose the model for new chats and what needs your permission."
        case "Background & Live Activity": "Choose whether to keep the connection active and show its status."
        case "Export & clear Everyone": "Export a copy of your data, or clear the shared chat without deleting individual chats."
        default: "Marble keeps chats and memories on your own host."
        }
    }
}

struct ConnectionPage: View {
    let screen: Screen
    @Environment(\.colorScheme) private var scheme
    @State private var address = ""
    @State private var token = ""
    @State private var phase: Screen?
    var p: Palette { Palette(dark: scheme == .dark) }
    var state: Screen { phase ?? screen }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                if state == .pairing {
                    PebbleMark(concept: .pocket, colors: [p.fig, p.markClay]).frame(width: 56, height: 56)
                } else {
                    Image(systemName: status.0).font(.system(size: 28, weight: .medium))
                        .foregroundStyle(state == .expired ? p.bad : p.fig)
                        .frame(width: 58, height: 58)
                        .background(p.wash, in: RoundedRectangle(cornerRadius: 18))
                }
                VStack(alignment: .leading, spacing: 7) {
                    Text(status.1).font(.largeTitle.weight(.semibold)).tracking(-0.7)
                    Text(status.2).foregroundStyle(p.muted)
                }
                switch state {
                case .pairing, .expired:
                    if state == .pairing {
                        field("Host address") {
                            TextField("Host address", text: $address,
                                      prompt: Text(verbatim: "https://your-mac.ts.net:4783").foregroundStyle(p.muted))
                                .keyboardType(.URL).textContentType(.URL).textInputAutocapitalization(.never).autocorrectionDisabled()
                        }
                    }
                    field(state == .expired ? "New access token" : "Access token") {
                        SecureField("Access token", text: $token,
                                    prompt: Text("Paste the token from your host").foregroundStyle(p.muted))
                    }
                    Action(text: state == .expired ? "Reconnect" : "Connect", prominent: true) { phase = .connecting }
                    if state == .pairing {
                        DisclosureGroup("Connection help") {
                            Text("Use your host’s Tailscale address and port 4783. If your host uses its own certificate, Marble checks that it belongs to your host before connecting. Your token is stored in the Keychain.")
                                .font(.subheadline).foregroundStyle(p.muted).padding(.top, 8)
                        }.surface(p)
                    }
                case .connecting:
                    HStack(spacing: 12) {
                        ProgressView()
                        Text(address.isEmpty ? "studio.example.ts.net:4783" : address).font(.subheadline)
                    }.surface(p)
                    Action(text: "Cancel") { phase = .disconnected }
                case .offline:
                    Action(text: "Try again", icon: "arrow.clockwise", prominent: true) { phase = .connecting }
                    Action(text: "Connection settings") { phase = .pairing }
                default:
                    Action(text: "Set up a connection", prominent: true) { phase = .pairing }
                }
            }
            .frame(maxWidth: 480).padding(.horizontal, 22).padding(.top, 30).padding(.bottom, 20)
            .frame(maxWidth: .infinity)
        }
        .background(p.paper.ignoresSafeArea())
        .navigationTitle("Marble")
        .navigationBarTitleDisplayMode(.inline)
    }

    var status: (String, String, String) {
        switch state {
        case .pairing: ("link", "Connect to Marble", "Enter the address and access token from your own Marble host.")
        case .expired: ("key.slash", "Access token expired", "Replace the token to continue. Your chats and memories are still on your host.")
        case .connecting: ("link", "Connecting…", "Checking your address and access token.")
        case .offline: ("wifi.slash", "You’re offline", "Marble can’t reach your host. Check your network and that Tailscale is connected.")
        default: ("link.badge.plus", "Not connected", "Add a Marble host to start talking and saving memories.")
        }
    }

    func field<V: View>(_ label: String, @ViewBuilder content: () -> V) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(label).font(.subheadline.weight(.semibold)).foregroundStyle(p.fig)
            content()
        }.surface(p)
    }
}
