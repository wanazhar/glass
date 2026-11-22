import SwiftUI

enum MarbleTheme {
    /// The one accent: the standard iOS system blue (the asset catalog's
    /// AccentColor carries the same values), used for interactive tint only.
    static let tint = Color.accentColor

    /// A marble's own colour, from its roster hue.
    static func marble(hue: Int?) -> Color {
        Color(
            hue: Double(((hue ?? 210) % 360 + 360) % 360) / 360,
            saturation: 0.55, brightness: 0.72
        )
    }

    static func status(_ name: String) -> Color {
        switch name {
        case "completed", "done", "live", "ok": return .green
        case "failed", "error", "offline", "blocked", "denied": return .red
        case "running", "cancelled", "interrupted": return .orange
        default: return .secondary
        }
    }
}

extension View {
    /// Background for controls that float above content — the composer bar.
    /// Real glass on iOS 26, the system bar material below it, and an opaque
    /// fill when transparency is reduced.
    @ViewBuilder
    func marbleFloating(in shape: some Shape = Capsule()) -> some View {
        if #available(iOS 26.0, *) {
            modifier(MarbleFloatingGlass(shape: shape))
        } else {
            modifier(MarbleFloatingBar(shape: shape))
        }
    }
}

@available(iOS 26.0, *)
private struct MarbleFloatingGlass<S: Shape>: ViewModifier {
    var shape: S
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    func body(content: Content) -> some View {
        if reduceTransparency {
            content.background(Color(.secondarySystemBackground), in: shape)
        } else {
            content.glassEffect(.regular, in: shape)
        }
    }
}

private struct MarbleFloatingBar<S: Shape>: ViewModifier {
    var shape: S
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    func body(content: Content) -> some View {
        if reduceTransparency {
            content.background(Color(.secondarySystemBackground), in: shape)
        } else {
            content.background(.bar, in: shape)
        }
    }
}

/// Toolbar title with a quiet connection mark: a small green dot while
/// connected, and a word only when it is not.
struct MarbleTitleView: View {
    var title: String
    @EnvironmentObject private var store: MarbleStore

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 5) {
                Text(title)
                    .font(.headline)
                    .lineLimit(1)
                if store.connection == .live {
                    Circle()
                        .fill(.green)
                        .frame(width: 6, height: 6)
                }
            }
            if store.connection != .live {
                Text(store.connection == .offline ? "Offline" : "Connecting…")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(title), \(store.connection == .live ? "connected" : store.connection == .offline ? "offline" : "connecting")")
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

    var body: some View {
        HStack(spacing: 10) {
            MarbleWorkingDots()
                .padding(.horizontal, 12)
                .padding(.vertical, 11)
                .background(
                    Color(.secondarySystemBackground),
                    in: RoundedRectangle(cornerRadius: 16, style: .continuous)
                )
            Text("\(name) is working")
                .font(.footnote)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(name) is working")
    }
}
