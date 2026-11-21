import SwiftUI

enum MarbleTheme {
    /// The one accent, used for interactive tint only.
    static let tint = Color(red: 0.43, green: 0.66, blue: 0.95)

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

/// Three dots: a turn is running, without covering the text that is arriving.
struct MarbleWorkingDots: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var lit = false

    var body: some View {
        HStack(spacing: 5) {
            ForEach(0..<3, id: \.self) { index in
                Circle()
                    .fill(MarbleTheme.tint)
                    .frame(width: 6, height: 6)
                    .opacity(lit ? 1 : 0.3)
                    .animation(
                        .easeInOut(duration: 0.6)
                            .repeatForever()
                            .delay(Double(index) * 0.15),
                        value: lit
                    )
            }
        }
        .onAppear { lit = !reduceMotion }
        .accessibilityLabel("Marble is working")
    }
}
