import SwiftUI

enum Design {
    static let small: CGFloat = 4
    static let gap: CGFloat = 8
    static let padding: CGFloat = 16
    static let spacious: CGFloat = 24
    static let control: CGFloat = 44
    static let corner: CGFloat = 14
    static let panel: CGFloat = 22
    static let readingWidth: CGFloat = 760
    static let surface = Color(uiColor: .secondarySystemBackground)
    static let background = Color(uiColor: .systemBackground)
    static let accent = Color.accentColor
    static let failure = Color.red
    static let display = Font.largeTitle.weight(.semibold)
    static let title = Font.title2.weight(.semibold)
    static let headline = Font.headline
    static let body = Font.body
    static let footnote = Font.footnote
    static let mono = Font.footnote.monospaced()
}

extension View {
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
