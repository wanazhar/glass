import SwiftUI

enum MarbleTheme {
    static let paper = Color("MarblePaper")
    static let tint = adaptive(0x653E52, 0xE3B7CA)
    static let card = adaptive(0xFFFBF6, 0x2D2523)
    static let wash = adaptive(0xEEE3E0, 0x3C2C32)
    static let chatWash = adaptive(0xF4DCD2, 0x3E2931)
    static let ink = adaptive(0x282022, 0xFFF4E9)
    static let muted = adaptive(0x695B59, 0xCBB9AD)
    static let line = adaptive(0xE2D6CB, 0x51443F)
    static let ready = adaptive(0x425A38, 0xBBD6A7)
    static let waiting = adaptive(0x76511F, 0xE9C284)
    static let problem = adaptive(0x913B42, 0xF5ABAF)
    static let clay = adaptive(0xEAB5A5, 0xDEA28F)
    static let moss = adaptive(0xC4D0A3, 0xB3C48C)
    static let panel = RoundedRectangle(cornerRadius: 24, style: .continuous)

    static func adaptive(_ light: UInt32, _ dark: UInt32) -> Color {
        Color(uiColor: UIColor { traits in
            let hex = traits.userInterfaceStyle == .dark ? dark : light
            return UIColor(red: Double(hex >> 16) / 255, green: Double(hex >> 8 & 255) / 255,
                           blue: Double(hex & 255) / 255, alpha: 1)
        })
    }

    static let hues = MarbleAvatarAppearance.hues

    static func paletteIndex(for hue: Int) -> Int {
        let normalized = (hue % 360 + 360) % 360
        return hues.indices.min { lhs, rhs in
            func distance(_ index: Int) -> Int {
                let delta = abs(hues[index].degrees - normalized)
                return min(delta, 360 - delta)
            }
            return distance(lhs) < distance(rhs)
        } ?? 0
    }

    /// The shared hue for a marble's avatar, name and reply.
    static func marble(hue: Int, brightness: Double = 0.72) -> Color {
        Color(
            hue: Double((hue % 360 + 360) % 360) / 360,
            saturation: 0.55, brightness: brightness
        )
    }

    static func status(_ name: String) -> Color {
        switch name {
        case "completed", "done", "live", "ok": return ready
        case "failed", "error", "offline", "blocked", "denied": return problem
        case "running", "cancelled", "interrupted": return waiting
        default: return muted
        }
    }
}

struct MarbleInlineButton: View {
    let title: LocalizedStringKey
    var role: ButtonRole? = nil
    var prominent = false
    var expands = false
    let action: () -> Void

    var body: some View {
        Button(role: role, action: action) {
            Text(title)
                .font(.subheadline.weight(.semibold))
                .multilineTextAlignment(.center)
                .foregroundStyle(role == .destructive ? Color.red : MarbleTheme.ink)
                .padding(.horizontal, 12)
                .padding(.vertical, 4)
                .frame(minWidth: 44, maxWidth: expands ? .infinity : nil, minHeight: 44)
                .contentShape(Capsule())
                .marbleFloating(tint: prominent ? (role == .destructive ? .red : MarbleTheme.tint) : nil,
                                interactive: true)
        }
        .buttonStyle(.plain)
    }
}

extension View {
    /// Background for controls that float above the transcript.
    /// Liquid Glass with an opaque fill when transparency is reduced.
    func marbleFloating(in shape: some Shape = Capsule(), tint: Color? = nil, interactive: Bool = false) -> some View {
        modifier(MarbleFloatingMaterial(shape: shape, tint: tint, interactive: interactive))
    }

    func marbleChatBar<Bar: View>(edge: VerticalEdge, @ViewBuilder content: () -> Bar) -> some View {
        safeAreaBar(edge: edge, spacing: 0, content: content)
    }

    func marbleChatScrollEdges() -> some View {
        scrollEdgeEffectStyle(.hard, for: .top)
            .scrollEdgeEffectStyle(.soft, for: .bottom)
    }

    func marbleNewChatBackground() -> some View {
        background {
            MarbleTheme.paper.overlay(alignment: .top) {
                LinearGradient(colors: [MarbleTheme.chatWash, MarbleTheme.paper],
                               startPoint: .top, endPoint: .bottom)
            }
            .ignoresSafeArea()
        }
    }

    func marbleToolbarTitle() -> some View {
        padding(.horizontal, 14)
            .padding(.vertical, 5)
            .marbleFloating()
    }
}

private struct MarbleFloatingMaterial<S: Shape>: ViewModifier {
    var shape: S
    var tint: Color?
    var interactive: Bool
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    func body(content: Content) -> some View {
        if reduceTransparency {
            content.background(tint == nil ? MarbleTheme.card : MarbleTheme.wash, in: shape)
        } else {
            content.glassEffect(.regular.tint(tint).interactive(interactive), in: shape)
        }
    }
}

/// Toolbar title with a quiet connection mark: a small green dot while
/// connected, and a word only when it is not.
struct MarbleTitleView: View {
    var title: String
    var avatar: MarbleSpeakerStyle? = nil
    var avatarWorking = false
    @EnvironmentObject private var store: MarbleStore

    private var connectionText: String? {
        if store.needsToken { return "Access token expired" }
        if store.connectionPaused { return "Not connected" }
        switch store.connection {
        case .idle: return "Not connected"
        case .connecting: return "Connecting…"
        case .live: return nil
        case .offline: return "Offline"
        }
    }

    var body: some View {
        let connectionText = connectionText
        VStack(spacing: 0) {
            HStack(spacing: 5) {
                if let avatar, let hue = avatar.hue {
                    MarbleAvatar(glyph: avatar.glyph, hue: hue, size: 20, isWorking: avatarWorking)
                }
                Text(title)
                    .font(.headline)
                    .lineLimit(1)
                if connectionText == nil {
                    Circle()
                        .fill(MarbleTheme.ready)
                        .frame(width: 6, height: 6)
                }
            }
            if let connectionText {
                Text(connectionText)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(title), \(connectionText ?? "Connected")")
    }
}

/// Toolbar status: a small colored dot plus plain text.
struct MarbleStatusText: View {
    var label: String
    var tone: Color

    var body: some View {
        HStack(spacing: 6) {
            Circle()
                .fill(tone)
                .frame(width: 7, height: 7)
            Text(label)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
    }
}

struct MarbleConnectionNotice: View {
    @EnvironmentObject private var store: MarbleStore
    @State private var replacesToken = false

    var body: some View {
        if store.needsToken || store.connectionPaused || store.connection == .offline || store.connection == .connecting {
            VStack(alignment: .leading, spacing: 6) {
                if store.needsToken {
                    Label("Access token expired", systemImage: "key.slash")
                    Button("Replace token") { replacesToken = true }
                } else if store.connectionPaused {
                    Label("Disconnected", systemImage: "network.slash")
                    Button("Connect") { store.reconnect(force: true) }
                } else if store.connection == .offline {
                    Label("Marble is offline", systemImage: "wifi.slash")
                    Text("Check that your host is running and reachable.")
                        .foregroundStyle(MarbleTheme.muted)
                    MarbleInlineButton(title: "Retry connection") { store.reconnect(force: true) }
                } else {
                    ProgressView("Connecting to your host…")
                }
            }
            .font(.subheadline)
            .foregroundStyle(MarbleTheme.ink)
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(MarbleTheme.card, in: MarbleTheme.panel)
            .sheet(isPresented: $replacesToken) {
                MarbleOnboardingView(expiredToken: true) { replacesToken = false }
            }
        }
    }
}

/// Three dots that rise and fade in turn, the way Messages shows typing.
struct MarbleWorkingDots: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 30, paused: reduceMotion)) { context in
            let time = context.date.timeIntervalSinceReferenceDate
            HStack(spacing: 4) {
                ForEach(0..<3, id: \.self) { index in
                    let phase = reduceMotion ? 0.5 : (sin((time * 2 * .pi / 1.2) - Double(index) * 0.9) + 1) / 2
                    Circle()
                        .fill(Color.secondary)
                        .frame(width: 7, height: 7)
                        .opacity(0.35 + 0.65 * phase)
                        .offset(y: -2.5 * phase)
                }
            }
        }
        .accessibilityHidden(true)
    }
}

/// Where the reply will appear: a small grey bubble with the dots and a
/// quiet line saying who is working.
struct MarbleThinkingBubble: View {
    var name: String = "Marble"
    var retrying = false
    var status: String? = nil
    var speaker: MarbleSpeakerStyle? = nil

    private var title: String { retrying ? "\(name) is retrying…" : status ?? "\(name) is working…" }

    var body: some View {
        HStack(spacing: 10) {
            if let speaker, let hue = speaker.hue {
                MarbleAvatar(glyph: speaker.glyph, hue: hue, size: 22, isWorking: true)
            }
            MarbleWorkingDots()
                .padding(.horizontal, 12)
                .padding(.vertical, 11)
                .background(
                    MarbleTheme.card,
                    in: MarbleTheme.panel
                )
            Text(title)
                .font(.footnote)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(title)
        .accessibilityIdentifier("\(name) is working")
    }
}
