import SwiftUI

enum MarblePalette {
    static let accent = Color(red: 0.43, green: 0.66, blue: 0.95)
    static let ink = Color(red: 0.93, green: 0.95, blue: 0.99)
    static let muted = Color(red: 0.62, green: 0.68, blue: 0.78)
    static let danger = Color(red: 1.0, green: 0.48, blue: 0.48)
    static let warn = Color(red: 1.0, green: 0.73, blue: 0.37)
    static let success = Color(red: 0.37, green: 0.84, blue: 0.64)

    static let aura = LinearGradient(
        colors: [
            Color(red: 0.04, green: 0.07, blue: 0.12),
            Color(red: 0.09, green: 0.07, blue: 0.18),
            Color(red: 0.03, green: 0.11, blue: 0.15),
        ],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
    )
}

/// Liquid Glass surface with a material fallback for older systems.
struct MarbleGlass: ViewModifier {
    var cornerRadius: CGFloat = 26

    @ViewBuilder
    func body(content: Content) -> some View {
        let shape = RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
        if #available(iOS 26.0, *) {
            content.glassEffect(.regular, in: shape)
        } else {
            content
                .background(.ultraThinMaterial, in: shape)
                .overlay(shape.strokeBorder(Color.white.opacity(0.12), lineWidth: 0.5))
        }
    }
}

extension View {
    func marbleGlass(cornerRadius: CGFloat = 26) -> some View {
        modifier(MarbleGlass(cornerRadius: cornerRadius))
    }

    /// A text field that belongs on the glass surface rather than inside a
    /// system-grey rounded border.
    func marbleField(cornerRadius: CGFloat = 14) -> some View {
        padding(.horizontal, 12)
            .padding(.vertical, 10)
            .marbleGlass(cornerRadius: cornerRadius)
    }
}

struct MarbleBackdrop: View {
    var body: some View {
        ZStack {
            MarblePalette.aura
            Circle()
                .fill(MarblePalette.accent.opacity(0.30))
                .frame(width: 520, height: 520)
                .blur(radius: 130)
                .offset(x: -140, y: -260)
            Circle()
                .fill(Color.purple.opacity(0.26))
                .frame(width: 460, height: 460)
                .blur(radius: 140)
                .offset(x: 170, y: 230)
        }
        .ignoresSafeArea()
    }
}

struct MarbleStatusPill: View {
    var label: String
    var tone: Color

    var body: some View {
        HStack(spacing: 6) {
            Circle()
                .fill(tone)
                .frame(width: 7, height: 7)
            Text(label)
                .font(.caption2.weight(.medium))
                .foregroundStyle(MarblePalette.muted)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 5)
        .marbleGlass(cornerRadius: 999)
    }
}

struct MarbleIconButton: View {
    var systemName: String
    var label: String
    var action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: systemName)
                .font(.system(size: 15, weight: .semibold))
                // A 38pt mark still needs a 44pt target to be reachable.
                .frame(width: 44, height: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(MarblePalette.ink)
        .marbleGlass(cornerRadius: 22)
        .accessibilityLabel(label)
    }
}

/// A shaped placeholder for content that is still on its way. It uses the same
/// corner radius as the rows it stands in for, so a sheet does not jump when
/// the real content replaces it.
struct MarbleSkeleton: View {
    var height: CGFloat = 16
    var width: CGFloat?
    var cornerRadius: CGFloat = 8
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var phase: CGFloat = -0.7

    var body: some View {
        RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
            .fill(Color.white.opacity(0.08))
            .overlay {
                GeometryReader { geometry in
                    LinearGradient(
                        colors: [.clear, Color.white.opacity(0.16), .clear],
                        startPoint: .leading,
                        endPoint: .trailing
                    )
                    .frame(width: geometry.size.width * 0.6)
                    .offset(x: geometry.size.width * phase)
                }
                .clipShape(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
            }
            .frame(width: width, height: height)
            .onAppear(perform: shimmer)
            .accessibilityHidden(true)
    }

    private func shimmer() {
        guard !reduceMotion else { return }
        withAnimation(.linear(duration: 1.2).repeatForever(autoreverses: false)) {
            phase = 1.3
        }
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
                    .fill(MarblePalette.accent)
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

struct MarbleMarbleMark: View {
    var body: some View {
        Circle()
            .fill(
                AngularGradient(
                    colors: [
                        Color(red: 0.50, green: 0.69, blue: 1.0),
                        Color(red: 0.71, green: 0.60, blue: 1.0),
                        Color(red: 0.50, green: 0.94, blue: 0.83),
                        Color(red: 0.50, green: 0.69, blue: 1.0),
                    ],
                    center: .center
                )
            )
            .frame(width: 26, height: 26)
            .overlay(Circle().stroke(Color.white.opacity(0.5), lineWidth: 0.5))
            .shadow(color: MarblePalette.accent.opacity(0.5), radius: 8, y: 3)
    }
}
