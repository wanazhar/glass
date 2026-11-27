import SwiftUI

enum Design {
    static let small: CGFloat = 4
    static let gap: CGFloat = 8
    static let padding: CGFloat = 16
    static let spacious: CGFloat = 24
    static let control: CGFloat = 44
    static let corner: CGFloat = 14
    static let panel: CGFloat = 22
    static let largePanel: CGFloat = 28
    /// The floating Threads panel, close to the iPhone display corners.
    static let drawerCorner: CGFloat = 30
    static let transcriptClearance: CGFloat = 18
    static let transcriptEnd: CGFloat = transcriptClearance - padding
    static let readingWidth: CGFloat = 760
    static let background = adaptive(0xF5F7FA, 0x0B1013)
    static let surface = adaptive(0xFFFFFF, 0x161E23)
    static let inset = adaptive(0xE8EBEF, 0x252D33)
    static let line = adaptive(0xD9E0E6, 0x34434B)
    static let ink = adaptive(0x131C22, 0xF1F6F8)
    static let muted = adaptive(0x4D5E69, 0xA9BAC4)
    static let accent = adaptive(LensPalette.lightTeal, LensPalette.darkTeal)
    static let onAccent = adaptive(0xFFFFFF, 0x092A30)
    static let success = adaptive(0x28613D, 0x9CDEA9)
    static let warning = adaptive(0x785011, 0xF7CB84)
    static let failure = adaptive(0xA23330, 0xFFAA9D)
    static let terminal = adaptive(0xEBF0F4, 0x151D23)
    static let display = Font.system(.largeTitle, weight: .semibold)
    static let title = Font.system(.title2, weight: .semibold)
    static let headline = Font.headline
    static let body = Font.body
    static let footnote = Font.footnote
    static let mono = Font.footnote.monospaced()

    static func adaptive(_ light: UInt32, _ dark: UInt32) -> Color {
        Color(uiColor: UIColor { traits in
            let rgb = traits.userInterfaceStyle == .dark ? dark : light
            return UIColor(red: Double((rgb >> 16) & 255) / 255,
                           green: Double((rgb >> 8) & 255) / 255,
                           blue: Double(rgb & 255) / 255, alpha: 1)
        })
    }
}

extension View {
    func lensCard(_ fill: Color = Design.surface, radius: CGFloat = Design.panel) -> some View {
        background(fill, in: .rect(cornerRadius: radius, style: .continuous))
    }

    func lensComposerCard() -> some View {
        modifier(LensComposerCard())
    }

    func floatingGlass() -> some View {
        glassEffect(.regular.interactive(), in: Capsule())
    }

    func glassPanel(cornerRadius: CGFloat = Design.panel, interactive: Bool = false) -> some View {
        glassEffect(interactive ? .regular.interactive() : .regular, in: .rect(cornerRadius: cornerRadius))
    }

    func floatingBar(edge: VerticalEdge = .bottom, @ViewBuilder _ bar: () -> some View) -> some View {
        safeAreaBar(edge: edge, spacing: edge == .top ? 0 : nil, content: bar)
    }

    func softTopScrollEdge() -> some View {
        scrollEdgeEffectStyle(.soft, for: .top)
    }
}

private struct LensComposerCard: ViewModifier {
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    func body(content: Content) -> some View {
        content
            .lensCard(reduceTransparency ? Design.surface : .clear, radius: Design.largePanel)
            .glassEffect(reduceTransparency ? .identity : .regular.tint(Design.surface.opacity(0.72)),
                         in: .rect(cornerRadius: Design.largePanel))
    }
}

struct GlassGroup<Content: View>: View {
    var spacing: CGFloat
    @ViewBuilder var content: Content

    init(spacing: CGFloat = Design.gap, @ViewBuilder content: () -> Content) {
        self.spacing = spacing
        self.content = content()
    }

    var body: some View {
        GlassEffectContainer(spacing: spacing) { content }
    }
}
