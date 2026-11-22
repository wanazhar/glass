import GlassModels
import SwiftUI

/// Watch and use the host's desktop: start or stop it, take control, and send touches and keys.
struct DesktopView: View {
    enum Pointer: String, CaseIterable, Identifiable {
        case tap = "Tap", trackpad = "Trackpad", scroll = "Scroll", pan = "Pan"
        var id: String { rawValue }
    }

    @Environment(AppModel.self) private var app
    @State private var model = DesktopModel()
    @AppStorage("glass.desktop.stats") private var showStats = false
    @State private var pointer: Pointer = .tap
    @State private var zoom: CGFloat = 1
    @State private var baseZoom: CGFloat = 1
    @State private var pan: CGSize = .zero
    @State private var basePan: CGSize = .zero
    @State private var dragOrigin: CGPoint?
    @State private var holding = false
    @State private var scrollLast: CGSize = .zero
    @State private var rightClick = false
    @State private var showKeys = false
    @State private var typed = ""
    @State private var confirmStop = false
    @State private var confirmDelete = false
    @FocusState private var keyboardFocus: Bool

    var body: some View {
        Group {
            if let status = model.status {
                if status.isRunning { running(status) } else { idle(status) }
            } else if let error = model.error {
                ContentUnavailableView("Can’t reach the desktop", systemImage: "exclamationmark.triangle",
                                       description: Text(error))
            } else {
                ProgressView()
            }
        }
        .navigationTitle("Desktop")
        .navigationBarTitleDisplayMode(.inline)
        .task {
            model.attach(app.api)
            model.isVisible = true
            await model.watch()
        }
        .onDisappear {
            model.isVisible = false
            model.stopPicture()
            if model.status?.youHaveControl == true { Task { await model.giveBackControl() } }
        }
        .sensoryFeedback(trigger: model.status?.lease) { _, lease in lease == "human" ? .start : .stop }
    }

    // MARK: Idle

    private func idle(_ status: DesktopStatus) -> some View {
        List {
            Section {
                Text("No desktop is running. Start one to watch Glass use apps, or to use it yourself.")
                    .foregroundStyle(.secondary)
            }
            Section("Start") {
                if status.capabilities.isEmpty {
                    Button("Start desktop") { Task { await model.start(nil) } }
                } else {
                    ForEach(status.capabilities) { option in
                        Button { Task { await model.start(option) } } label: {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(option.label)
                                Text(summary(option)).font(.footnote).foregroundStyle(.secondary)
                            }
                        }
                        .disabled(!option.available)
                        .accessibilityIdentifier("desktop.start.\(option.lifecycle)")
                    }
                }
            }
            .disabled(model.busy != nil)
            if status.capabilities.contains(where: { $0.isReusable && $0.profileExists }) {
                Section {
                    Button("Delete saved desktop", role: .destructive) { confirmDelete = true }
                } footer: {
                    Text("Removes the apps, files and sign-ins kept by the reusable desktop.")
                }
                .disabled(model.busy != nil)
            }
            if let message = model.error ?? status.error {
                Section { Label(message, systemImage: "exclamationmark.triangle").foregroundStyle(.secondary) }
            }
        }
        .overlay { if let busy = model.busy { ProgressView(busy) } }
        .refreshable { await model.refresh() }
        .confirmationDialog("Delete the saved desktop?", isPresented: $confirmDelete, titleVisibility: .visible) {
            Button("Delete", role: .destructive) { Task { await model.deleteSavedDesktop() } }
        } message: {
            Text("This can’t be undone.")
        }
    }

    private func summary(_ option: DesktopOption) -> String {
        guard option.available else { return option.reason ?? "Not available on this host." }
        if option.isReusable {
            return option.profileExists ? "Picks up where you left off." : "Keeps apps and files between sessions."
        }
        return "Starts clean and is erased when stopped."
    }

    // MARK: Running

    private func running(_ status: DesktopStatus) -> some View {
        VStack(spacing: 0) {
            GeometryReader { geometry in
                let viewport = DesktopViewport(container: geometry.size, image: model.frame?.size ?? CGSize(width: 16, height: 10),
                                               zoom: zoom, pan: pan)
                ZStack(alignment: .topLeading) {
                    Color.black
                    Group {
                        if let frame = model.frame {
                            Image(uiImage: frame).resizable().interpolation(.medium).aspectRatio(contentMode: .fit)
                        } else {
                            ProgressView().tint(.white)
                        }
                    }
                    .frame(width: geometry.size.width, height: geometry.size.height)
                    .scaleEffect(zoom)
                    .offset(pan)
                    if status.youHaveControl && pointer == .trackpad {
                        let spot = viewport.screenPoint(for: model.cursor)
                        Image(systemName: "cursorarrow")
                            .font(.title3)
                            .foregroundStyle(.white, .black)
                            .shadow(color: .black, radius: 1)
                            .position(x: spot.x + 6, y: spot.y + 9)
                            .accessibilityHidden(true)
                    }
                }
                .contentShape(Rectangle())
                .gesture(drag(viewport, control: status.youHaveControl))
                .simultaneousGesture(magnify(viewport))
                .accessibilityElement()
                .accessibilityLabel("Desktop screen")
                .accessibilityValue(status.controlSummary)
                .accessibilityIdentifier("desktop.screen")
            }
            .clipped()
            .focusable(status.youHaveControl)
            .focused($keyboardFocus)
            .onKeyPress(phases: .down) { press in
                guard status.youHaveControl else { return .ignored }
                return handle(press) ? .handled : .ignored
            }
            controls(status)
        }
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Menu {
                    Button("Reconnect view", systemImage: "arrow.clockwise") { model.reconnect() }
                    Toggle(isOn: $showStats) { Label("Connection details", systemImage: "speedometer") }
                        .accessibilityIdentifier("desktop.stats.toggle")
                    Button("Actual size", systemImage: "arrow.down.right.and.arrow.up.left") { resetZoom() }
                        .disabled(zoom == 1)
                    if status.lease != "read-only" {
                        Button("View only", systemImage: "eye") { Task { await model.viewOnly() } }
                    }
                    Divider()
                    Button("Stop desktop", systemImage: "stop", role: .destructive) { confirmStop = true }
                } label: {
                    Label("More", systemImage: "ellipsis")
                }
                .accessibilityIdentifier("desktop.more")
            }
        }
        .confirmationDialog("Stop the desktop?", isPresented: $confirmStop, titleVisibility: .visible) {
            Button("Stop desktop", role: .destructive) { Task { await model.stop() } }
        } message: {
            Text(status.current?.isReusable == true ? "Apps and files are kept for next time." : "Everything on this desktop is erased.")
        }
    }

    private func controls(_ status: DesktopStatus) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .firstTextBaseline) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(status.controlSummary).font(.subheadline.weight(.semibold))
                    Text(pictureSummary).font(.footnote).foregroundStyle(.secondary)
                    if showStats {
                        Text(model.stats.summary)
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(.secondary)
                            .accessibilityIdentifier("desktop.stats")
                    }
                }
                .accessibilityElement(children: .combine)
                .accessibilityIdentifier("desktop.state")
                Spacer()
                if status.youHaveControl {
                    Button { showKeys.toggle(); if showKeys { keyboardFocus = false } } label: {
                        Label("Keys", systemImage: "keyboard")
                    }
                    .buttonStyle(.bordered)
                    .accessibilityIdentifier("desktop.keys")
                    Button("Give back") { Task { await model.giveBackControl() } }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("desktop.giveBack")
                } else {
                    Button("Take control") { Task { await model.takeControl() } }
                        .buttonStyle(.borderedProminent)
                        .accessibilityIdentifier("desktop.takeControl")
                }
            }
            .disabled(model.busy != nil)
            if status.youHaveControl {
                HStack {
                    Picker("Touch", selection: $pointer) {
                        ForEach(Pointer.allCases) { Text($0.rawValue).tag($0) }
                    }
                    .pickerStyle(.segmented)
                    .accessibilityIdentifier("desktop.pointer")
                    Toggle(isOn: $rightClick) { Label("Right click", systemImage: "cursorarrow.click.2") }
                        .toggleStyle(.button)
                        .labelStyle(.iconOnly)
                        .accessibilityHint("The next tap or click is a right click.")
                }
                if showKeys { keys }
            } else {
                Text(status.lease == "read-only" ? "Nobody can use the mouse or keyboard. Take control to use them."
                                                 : "Glass is using the mouse and keyboard. Take control to use them yourself.")
                    .font(.footnote).foregroundStyle(.secondary)
            }
            if let message = model.error ?? status.error {
                Label(message, systemImage: "exclamationmark.triangle").font(.footnote).foregroundStyle(.secondary)
            }
        }
        .padding()
        .background(Color(.systemBackground))
    }

    private var pictureSummary: String {
        switch model.picture {
        case .live: "Live"
        case .stills: "Live view unavailable. Updating twice a second."
        case .connecting, .idle: "Connecting…"
        }
    }

    private var keys: some View {
        VStack(alignment: .leading, spacing: 8) {
            TextField("Type and press Return to send", text: $typed)
                .textFieldStyle(.roundedBorder)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .submitLabel(.send)
                .onSubmit { model.type(typed); typed = "" }
                .accessibilityIdentifier("desktop.text")
            ScrollView(.horizontal) {
                HStack(spacing: 6) {
                    Toggle("Ctrl", isOn: $model.control).toggleStyle(.button)
                    Toggle("Alt", isOn: $model.alt).toggleStyle(.button)
                    ForEach(Self.keyRow, id: \.key) { item in
                        Button(item.title) { model.tapKey(item.key) }
                            .accessibilityLabel(item.spoken)
                    }
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }
            .scrollIndicators(.hidden)
            Text("iPhone keeps shortcuts like ⌘Tab and ⌘Space for itself, so they can’t be sent. Use Ctrl and Alt above, or the keys on a connected keyboard.")
                .font(.caption).foregroundStyle(.secondary)
        }
    }

    private static let keyRow: [(key: String, title: String, spoken: String)] = [
        ("Escape", "Esc", "Escape"), ("Tab", "Tab", "Tab"), ("Enter", "Return", "Return"), ("BackSpace", "⌫", "Delete backward"),
        ("ArrowUp", "↑", "Arrow up"), ("ArrowDown", "↓", "Arrow down"), ("ArrowLeft", "←", "Arrow left"),
        ("ArrowRight", "→", "Arrow right"), ("Home", "Home", "Home"), ("End", "End", "End"),
        ("PageUp", "Page up", "Page up"), ("PageDown", "Page down", "Page down"), ("Delete", "Del", "Delete forward"),
    ]

    // MARK: Gestures

    private var button: DesktopButton { rightClick ? .right : .left }

    private func drag(_ viewport: DesktopViewport, control: Bool) -> some Gesture {
        DragGesture(minimumDistance: 0)
            .onChanged { value in
                let mode: Pointer = control ? pointer : .pan
                let moved = hypot(value.translation.width, value.translation.height) > 6
                switch mode {
                case .pan:
                    pan = viewport.clampedPan(CGSize(width: basePan.width + value.translation.width,
                                                     height: basePan.height + value.translation.height))
                case .tap:
                    guard moved, let point = viewport.desktopPoint(at: value.location, clamped: true) else { return }
                    if !holding, let start = viewport.desktopPoint(at: value.startLocation) {
                        holding = true
                        model.move(to: start, force: true)
                        model.send(.button(x: start.x, y: start.y, button: button, pressed: true))
                    }
                    if holding { model.move(to: point) }
                case .trackpad:
                    if dragOrigin == nil { dragOrigin = model.cursor }
                    guard moved, let origin = dragOrigin else { return }
                    let rect = viewport.pictureRect
                    guard rect.width > 0 else { return }
                    let speed = 1.4 / zoom
                    model.move(to: CGPoint(x: min(max(origin.x + value.translation.width / rect.width * speed, 0), 1),
                                           y: min(max(origin.y + value.translation.height / rect.height * speed, 0), 1)))
                case .scroll:
                    let delta = CGSize(width: value.translation.width - scrollLast.width,
                                       height: value.translation.height - scrollLast.height)
                    scrollLast = value.translation
                    if let point = viewport.desktopPoint(at: value.startLocation, clamped: true) {
                        model.scroll(at: point, by: CGSize(width: -delta.width, height: -delta.height))
                    }
                }
            }
            .onEnded { value in
                let mode: Pointer = control ? pointer : .pan
                let moved = hypot(value.translation.width, value.translation.height) > 6
                switch mode {
                case .pan:
                    basePan = pan
                case .tap:
                    if holding, let point = viewport.desktopPoint(at: value.location, clamped: true) {
                        model.move(to: point, force: true)
                        model.send(.button(x: point.x, y: point.y, button: button, pressed: false))
                        rightClick = false
                    } else if !moved, let point = viewport.desktopPoint(at: value.location) {
                        model.cursor = point
                        model.send(.click(x: point.x, y: point.y, button: button))
                        rightClick = false
                    }
                    holding = false
                case .trackpad:
                    if moved {
                        model.move(to: model.cursor, force: true)
                    } else {
                        model.send(.click(x: model.cursor.x, y: model.cursor.y, button: button))
                        rightClick = false
                    }
                    dragOrigin = nil
                case .scroll:
                    scrollLast = .zero
                }
            }
    }

    private func magnify(_ viewport: DesktopViewport) -> some Gesture {
        MagnifyGesture()
            .onChanged { value in zoom = min(max(baseZoom * value.magnification, 1), 5) }
            .onEnded { _ in
                baseZoom = zoom
                if zoom <= 1.01 { resetZoom() } else { pan = viewport.clampedPan(pan); basePan = pan }
            }
    }

    private func resetZoom() {
        zoom = 1; baseZoom = 1; pan = .zero; basePan = .zero
    }

    // MARK: Hardware keyboard

    private func handle(_ press: KeyPress) -> Bool {
        let named: [KeyEquivalent: String] = [
            .return: "Enter", .escape: "Escape", .tab: "Tab", .delete: "BackSpace", .deleteForward: "Delete",
            .upArrow: "ArrowUp", .downArrow: "ArrowDown", .leftArrow: "ArrowLeft", .rightArrow: "ArrowRight",
            .home: "Home", .end: "End", .pageUp: "PageUp", .pageDown: "PageDown", .space: "Space",
        ]
        var modifiers: [String] = []
        if press.modifiers.contains(.control) { modifiers.append("Control") }
        if press.modifiers.contains(.option) { modifiers.append("Alt") }
        if press.modifiers.contains(.command) { modifiers.append("Meta") }
        let key = named[press.key] ?? (press.characters.count == 1 ? press.characters.uppercased() : nil)
        if !modifiers.isEmpty, let key {
            if press.modifiers.contains(.shift) { modifiers.append("Shift") }
            model.send(.chord(Array(modifiers.prefix(3)) + [key]))
        } else if let name = named[press.key], name != "Space" {
            model.send(.keyTap(name))
        } else if !press.characters.isEmpty {
            model.send(.text(press.characters))
        } else {
            return false
        }
        return true
    }
}
