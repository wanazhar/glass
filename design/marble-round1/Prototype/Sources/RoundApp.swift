import SwiftUI

enum Screen: String, CaseIterable {
    case picker, sharedHome = "home-everyone", everyone, chat, profile, memory
    case edit = "edit-memory", confirm = "confirm-memory", new = "new-marble"
    case activity, settings, pairing, empty, error, offline, connecting, disconnected, expired
    case threads, search, components, controls, system, identity, storyboard, splash
    var tab: Place {
        switch self {
        case .sharedHome, .everyone, .empty: .everyone
        case .activity: .activity
        case .settings: .settings
        default: .marbles
        }
    }
}

enum Place: String, CaseIterable, Identifiable {
    case marbles = "Marbles", everyone = "Everyone", activity = "Activity", settings = "Settings"
    var id: Self { self }
    var symbol: String {
        switch self {
        case .marbles: "hexagon"
        case .everyone: "bubble.left.and.bubble.right"
        case .activity: "tray"
        case .settings: "gearshape"
        }
    }
    var root: Screen {
        switch self {
        case .marbles: .picker
        case .everyone: .everyone
        case .activity: .activity
        case .settings: .settings
        }
    }
}

@main
struct MarbleRoundApp: App {
    var body: some Scene {
        WindowGroup {
            RoundRoot()
        }
    }
}

struct RoundRoot: View {
    let initial: Screen
    @State private var tab: Place
    @State private var path: [Screen] = []
    @State private var tabletPath: [Screen] = []
    @State private var tabletScreen: Screen?
    @State private var livePerson = Person.ada
    @State private var showingSplash: Bool
    @Environment(\.horizontalSizeClass) private var horizontal
    @Environment(\.colorScheme) private var scheme
    var p: Palette { Palette(dark: scheme == .dark) }

    init() {
        let args = ProcessInfo.processInfo.arguments
        let screen = args.dropFirst().compactMap(Screen.init(rawValue:)).first
        initial = screen == .splash ? .picker : (screen ?? .picker)
        _tab = State(initialValue: initial.tab)
        let start: [Screen]
        switch initial {
        case .chat: start = [.chat]
        case .profile: start = [.chat, .profile]
        case .memory: start = [.chat, .profile, .memory]
        case .edit, .confirm: start = [.chat, .profile, .memory, initial]
        case .threads, .search: start = [.chat, initial]
        case .new: start = [.new]
        default: start = []
        }
        _path = State(initialValue: start)
        _showingSplash = State(initialValue: screen == nil || screen == .splash)
    }

    var body: some View {
        Group {
            if showingSplash {
                SplashView { showingSplash = false }
            } else if initial == .controls {
                TabView {
                    ForEach(Place.allCases) { place in
                        Tab(place.rawValue, systemImage: place.symbol) {
                            Sheets(kind: .controls)
                        }
                    }
                }
            } else if [.components, .system, .identity, .storyboard].contains(initial) {
                Sheets(kind: initial)
            } else if [.pairing, .offline, .connecting, .disconnected, .expired].contains(initial) {
                NavigationStack { ConnectionPage(screen: initial) }
            } else if horizontal == .regular && (initial == .picker || initial == .chat) {
                split
            } else {
                tabs
            }
        }
        .tint(p.fig)
        .foregroundStyle(p.ink)
    }

    var tabs: some View {
        TabView(selection: $tab) {
            ForEach(Place.allCases) { place in
                Tab(place.rawValue, systemImage: place.symbol, value: place) {
                    NavigationStack(path: tab == place ? $path : .constant([])) {
                        let standalone: [Screen] = [.sharedHome, .empty, .error]
                        let current = initial.tab == place && standalone.contains(initial) ? initial : place.root
                        content(current)
                            .navigationDestination(for: Screen.self) { content($0) }
                    }
                }
            }
        }
        .onChange(of: tab) { _, _ in path = [] }
    }

    var split: some View {
        NavigationSplitView {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    Text("Your Marbles").font(.title2.bold())
                    ForEach(Person.all) { person in
                        Button {
                            livePerson = person
                            tabletScreen = .chat
                            tabletPath = []
                        } label: {
                            HStack(spacing: 12) {
                                face(person, 36)
                                VStack(alignment: .leading, spacing: 3) {
                                    Text(person.name).font(.headline)
                                    Text(person.note).font(.footnote).foregroundStyle(p.muted)
                                }
                                Spacer()
                            }
                            .padding(10)
                            .background((tabletScreen ?? initial) == .chat && person == livePerson ? p.wash : .clear,
                                        in: RoundedRectangle(cornerRadius: 14))
                        }.buttonStyle(.plain)
                    }
                    Divider()
                    ForEach([Place.everyone, .activity, .settings]) { place in
                        Button {
                            tabletScreen = place.root
                            tabletPath = []
                        } label: {
                            Label(place.rawValue, systemImage: place.symbol)
                        }
                        .buttonStyle(.plain)
                    }
                }.padding(20)
            }
            .background(p.paper)
            .navigationTitle("Marble")
            .navigationSplitViewColumnWidth(min: 282, ideal: 316)
        } detail: {
            NavigationStack(path: $tabletPath) {
                Group {
                    let current = tabletScreen ?? initial
                    if current == .picker { tabletHome }
                    else { content(current) }
                }
                .navigationDestination(for: Screen.self) { content($0) }
            }
        }
        .navigationSplitViewStyle(.balanced)
    }

    var tabletHome: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                PebbleMark(concept: .pocket, colors: [p.fig, p.markClay]).frame(width: 48, height: 48)
                Text("Who’s on your mind?").font(.largeTitle.weight(.semibold)).tracking(-0.7)
                Text("Choose a Marble on the left to talk one-to-one. Each has its own voice and memories.")
                    .foregroundStyle(p.muted)
                VStack(alignment: .leading, spacing: 10) {
                    Text("Pick up where you left off").font(.headline)
                    NavigationLink(value: Screen.everyone) {
                        LineItem(title: "Everyone", detail: "A slower Saturday · Today", icon: "bubble.left.and.bubble.right")
                            .surface(p)
                    }.buttonStyle(.plain)
                }
            }
            .frame(maxWidth: 580).padding(28).frame(maxWidth: .infinity)
        }
        .background(p.paper.ignoresSafeArea())
        .navigationTitle("Home")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                NavigationLink(value: Screen.new) { Image(systemName: "plus").accessibilityLabel("New Marble") }
            }
        }
    }

    @ViewBuilder func content(_ screen: Screen) -> some View {
        switch screen {
        case .picker: picker
        case .sharedHome, .everyone, .chat, .empty, .error:
            Conversation(screen: screen, person: livePerson) { person in
                livePerson = person
                if horizontal == .regular {
                    tabletScreen = .chat
                    tabletPath = []
                } else {
                    path.append(.chat)
                }
            }
        case .pairing, .offline, .connecting, .disconnected, .expired:
            ConnectionPage(screen: screen)
        default: DetailPage(screen: screen, person: livePerson)
        }
    }

    var picker: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Who’s on your mind?").font(.largeTitle.weight(.semibold)).tracking(-0.7)
                    Text("Each Marble has its own voice and memories.")
                        .font(.subheadline).foregroundStyle(p.muted)
                }
                VStack(spacing: 0) {
                    ForEach(Person.all) { person in
                        if person != .ada { Divider().overlay(p.line).padding(.leading, 78) }
                        Button {
                            livePerson = person
                            path.append(.chat)
                        } label: {
                            HStack(spacing: 14) {
                                face(person, 50)
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(person.name).font(.title3.weight(.semibold))
                                    Text(person.note).font(.subheadline).foregroundStyle(p.muted)
                                }
                                Spacer(minLength: 6)
                                Image(systemName: "chevron.right").font(.footnote.weight(.semibold))
                                    .foregroundStyle(p.fig)
                            }
                            .padding(15)
                            .contentShape(Rectangle())
                        }.buttonStyle(.plain)
                    }
                }
                .surface(p, padding: 0)
                VStack(alignment: .leading, spacing: 10) {
                    Text("Pick up where you left off").font(.headline)
                    Button {
                        tab = .everyone
                        if horizontal == .regular { path = [.everyone] }
                    } label: {
                        LineItem(title: "Everyone", detail: "A slower Saturday · Today", icon: "bubble.left.and.bubble.right")
                            .surface(p)
                    }.buttonStyle(.plain)
                }
            }
            .frame(maxWidth: 580)
            .padding(.horizontal, 22).padding(.top, 22).padding(.bottom, 24)
            .frame(maxWidth: .infinity)
        }
        .background(p.paper.ignoresSafeArea())
        .navigationTitle("Marble")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                NavigationLink(value: Screen.new) { Image(systemName: "plus").accessibilityLabel("New Marble") }
            }
        }
    }
}

func face(_ person: Person, _ size: CGFloat, working: Bool = false) -> some View {
    Avatar(form: person.form, hue: person.hue, size: size, working: working)
}

extension View {
    func surface(_ p: Palette, padding: CGFloat = 16, radius: CGFloat = 18) -> some View {
        self.padding(padding)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(p.card, in: RoundedRectangle(cornerRadius: radius))
            .overlay { RoundedRectangle(cornerRadius: radius).strokeBorder(p.line, lineWidth: 1) }
    }
}

struct LineItem: View {
    let title: String
    var detail: String? = nil
    var icon: String? = nil
    var arrow = true
    @Environment(\.colorScheme) private var scheme
    var body: some View {
        let p = Palette(dark: scheme == .dark)
        HStack(spacing: 12) {
            if let icon {
                Image(systemName: icon).foregroundStyle(p.fig)
                    .frame(width: 32, height: 32)
                    .background(p.wash, in: RoundedRectangle(cornerRadius: 10))
            }
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.body.weight(.medium)).foregroundStyle(p.ink)
                if let detail { Text(detail).font(.subheadline).foregroundStyle(p.muted) }
            }
            Spacer(minLength: 4)
            if arrow { Image(systemName: "chevron.right").font(.caption.weight(.semibold)).foregroundStyle(p.fig) }
        }
        .frame(minHeight: 44)
        .contentShape(Rectangle())
    }
}

struct Action: View {
    let text: String
    var icon: String? = nil
    var prominent = false
    var action: () -> Void = {}
    @Environment(\.colorScheme) private var scheme
    var body: some View {
        let p = Palette(dark: scheme == .dark)
        Button(action: action) {
            Group {
                if let icon { Label(text, systemImage: icon) } else { Text(text) }
            }
            .font(.subheadline.weight(.semibold))
            .padding(.horizontal, 16).padding(.vertical, 8).frame(minHeight: 44)
            .foregroundStyle(prominent ? p.onFig : p.fig)
            .frame(maxWidth: prominent ? .infinity : nil)
            .glassEffect(prominent ? .regular.tint(p.fig).interactive() : .regular.interactive(), in: .capsule)
        }
        .buttonStyle(.plain)
    }
}
