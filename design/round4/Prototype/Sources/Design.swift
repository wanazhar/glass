import SwiftUI

enum TypeToken {
    static let display = Font.system(.largeTitle, weight: .semibold)
}

struct Palette {
    let dark: Bool
    var paper: Color { Color(hex: dark ? 0x0B1013 : 0xF5F7FA) }
    var panel: Color { Color(hex: dark ? 0x161E23 : 0xFFFFFF) }
    var inset: Color { Color(hex: dark ? 0x252D33 : 0xE8EBEF) }
    var line: Color { Color(hex: dark ? 0x34434B : 0xD9E0E6) }
    var ink: Color { Color(hex: dark ? 0xF1F6F8 : 0x131C22) }
    var muted: Color { Color(hex: dark ? 0xA9BAC4 : 0x4D5E69) }
    var accent: Color { Color(hex: dark ? 0x60E4E8 : 0x08737D) }
    var onAccent: Color { Color(hex: dark ? 0x092A30 : 0xFFFFFF) }
    var success: Color { Color(hex: dark ? 0x9CDEA9 : 0x28613D) }
    var warning: Color { Color(hex: dark ? 0xF7CB84 : 0x785011) }
    var failure: Color { Color(hex: dark ? 0xFFAA9D : 0xA23330) }
    var terminal: Color { Color(hex: dark ? 0x151D23 : 0xEBF0F4) }
}

extension Color {
    init(hex: UInt32) {
        self.init(red: Double((hex >> 16) & 255) / 255,
                  green: Double((hex >> 8) & 255) / 255,
                  blue: Double(hex & 255) / 255)
    }
}

extension EnvironmentValues {
    var palette: Palette { Palette(dark: colorScheme == .dark) }
}

enum MarkKind: Int, CaseIterable {
    case lens, drop

    var name: String {
        switch self {
        case .lens: "Lens v1"
        case .drop: "Lens v5"
        }
    }
}

struct LensSilhouette: Shape {
    let kind: MarkKind
    func path(in rect: CGRect) -> Path {
        if kind == .lens { return Path(ellipseIn: rect) }
        var path = Path()
        func point(_ x: CGFloat, _ y: CGFloat) -> CGPoint {
            CGPoint(x: rect.minX + x * rect.width, y: rect.minY + y * rect.height)
        }
        path.move(to: point(0.77, 0.04))
        path.addCurve(to: point(0.94, 0.40), control1: point(0.94, -0.01), control2: point(0.90, 0.23))
        path.addCurve(to: point(0.86, 0.83), control1: point(1.03, 0.64), control2: point(0.90, 0.76))
        path.addCurve(to: point(0.12, 0.84), control1: point(0.68, 1.03), control2: point(0.29, 1.05))
        path.addCurve(to: point(0.23, 0.38), control1: point(-0.02, 0.68), control2: point(0.04, 0.53))
        path.addCurve(to: point(0.77, 0.04), control1: point(0.41, 0.22), control2: point(0.58, 0.20))
        path.closeSubpath()
        return path
    }
}

struct Horizon: Shape {
    var bend: CGFloat = 0.15
    var fullWidth = false

    func path(in rect: CGRect) -> Path {
        let x = rect.minX, y = rect.midY
        let w = rect.width, h = rect.height
        let start = fullWidth ? 0.28 : 0.0
        let end = 1 - start
        let span = end - start
        var path = Path()
        path.move(to: CGPoint(x: x, y: y))
        path.addLine(to: CGPoint(x: x + start * w, y: y))
        path.addCurve(to: CGPoint(x: x + (start + 0.48 * span) * w, y: y + h * bend * 0.55),
                      control1: CGPoint(x: x + (start + 0.20 * span) * w, y: y - h * bend * 0.65),
                      control2: CGPoint(x: x + (start + 0.33 * span) * w, y: y + h * bend * 0.73))
        path.addCurve(to: CGPoint(x: x + end * w, y: y + h * bend * 0.07),
                      control1: CGPoint(x: x + (start + 0.70 * span) * w, y: y + h * bend * 0.42),
                      control2: CGPoint(x: x + (start + 0.76 * span) * w, y: y - h * bend * 0.86))
        path.addLine(to: CGPoint(x: x + w, y: y + h * bend * 0.07))
        return path
    }
}

struct LensMark: View {
    var kind: MarkKind = .lens
    var compact = false
    var optical = false
    var fixedPalette: Palette?
    var tint: Color?
    @Environment(\.palette) private var systemPalette

    private var p: Palette { fixedPalette ?? systemPalette }
    private var teal: Color { tint ?? Color(hex: p.dark ? 0x51F5F5 : 0x078995) }
    private var shape: LensSilhouette { LensSilhouette(kind: kind) }

    var body: some View {
        GeometryReader { proxy in
            let size = min(proxy.size.width, proxy.size.height)
            let stroke = compact ? 1.15 : max(0.8, size / 74)
            ZStack {
                if compact {
                    shape.stroke(p.accent, lineWidth: stroke)
                } else {
                    shape
                        .stroke(teal.opacity(p.dark ? 0.42 : 0.22), lineWidth: stroke * 2.6)
                        .blur(radius: size * 0.042)
                        .offset(x: size * 0.025, y: size * 0.055)
                    if optical && kind == .lens {
                        Color.clear.glassEffect(.clear, in: .circle)
                    }
                    shape.fill(LinearGradient(colors: [
                        Color.white.opacity(p.dark ? 0.13 : 0.76),
                        Color(hex: 0x698794).opacity(p.dark ? 0.10 : 0.14),
                        teal.opacity(p.dark ? 0.19 : 0.13)
                    ], startPoint: .topLeading, endPoint: .bottomTrailing))
                    shape.fill(
                        RadialGradient(colors: [
                            Color.clear,
                            Color(hex: 0x203C47).opacity(p.dark ? 0.34 : 0.15),
                            teal.opacity(p.dark ? 0.20 : 0.07)
                        ], center: UnitPoint(x: 0.34, y: 0.24), startRadius: size * 0.28, endRadius: size)
                    )
                    shape.stroke(
                        AngularGradient(gradient: Gradient(stops: [
                            .init(color: teal, location: 0),
                            .init(color: teal.opacity(0.85), location: 0.17),
                            .init(color: Color.white.opacity(0.95), location: 0.31),
                            .init(color: Color(hex: 0x596D78), location: 0.51),
                            .init(color: Color.white.opacity(0.95), location: 0.69),
                            .init(color: Color(hex: 0x596D78), location: 0.84),
                            .init(color: teal, location: 1)
                        ]), center: .center),
                        lineWidth: stroke
                    )
                    shape.stroke(Color.white.opacity(p.dark ? 0.36 : 0.75), lineWidth: stroke * 0.45)
                        .padding(stroke * 1.35)
                    Capsule()
                        .fill(Color.white.opacity(0.95))
                        .frame(width: size * 0.27, height: size * 0.045)
                        .rotationEffect(.degrees(-40))
                        .position(x: size * 0.27, y: size * (kind == .lens ? 0.19 : 0.35))
                }
                Horizon(bend: kind == .lens ? 0.14 : 0.26)
                    .stroke(compact ? teal : teal.opacity(0.95), lineWidth: stroke)
                    .clipShape(shape)
            }
            .frame(width: size, height: size)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .accessibilityHidden(true)
    }
}

struct Eyebrow: View {
    let title: String
    @Environment(\.palette) private var p

    init(_ title: String) { self.title = title }

    var body: some View {
        Text(title.uppercased())
            .font(.system(.caption2, design: .monospaced, weight: .semibold))
            .tracking(1.6)
            .foregroundStyle(p.muted)
    }
}

struct SolidPanel: ViewModifier {
    @Environment(\.palette) private var p
    var radius: CGFloat

    func body(content: Content) -> some View {
        content.background(p.panel, in: .rect(cornerRadius: radius))
            .overlay(RoundedRectangle(cornerRadius: radius).strokeBorder(p.line, lineWidth: 1))
    }
}

extension View {
    func panel(_ radius: CGFloat = 16) -> some View { modifier(SolidPanel(radius: radius)) }
}

struct ActionPill: View {
    let title: String
    var icon: String?
    var action: () -> Void = {}
    @Environment(\.palette) private var p

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                if let icon { Image(systemName: icon) }
                Text(title)
            }
            .font(.subheadline.weight(.semibold))
            .padding(.horizontal, 16)
            .frame(minHeight: 44)
            .foregroundStyle(p.onAccent)
            .background(p.accent, in: .capsule)
        }
        .buttonStyle(.plain)
    }
}

struct GlassIcon: View {
    let icon: String
    let label: String
    var action: () -> Void = {}
    @Environment(\.palette) private var p

    var body: some View {
        Button(action: action) {
            Image(systemName: icon)
                .font(.system(size: 17, weight: .semibold))
                .foregroundStyle(p.ink)
                .frame(width: 44, height: 44)
                .contentShape(.circle)
        }
        .buttonStyle(.plain)
        .glassEffect(.regular.interactive(), in: .circle)
        .accessibilityLabel(label)
    }
}
