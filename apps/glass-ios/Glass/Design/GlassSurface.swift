import SwiftUI

extension View {
    /// Liquid Glass for small floating controls over content (iOS 26); the system's regular
    /// material on iOS 18–25. Never use it on content: cards, rows, banners or forms.
    @ViewBuilder
    func floatingGlass() -> some View {
        if #available(iOS 26.0, *) {
            glassEffect(.regular, in: Capsule())
        } else {
            background(.regularMaterial, in: Capsule())
        }
    }
}

extension View {
    /// A floating Liquid Glass panel over content, for controls that sit in a bar: the message box,
    /// a docked decision, a notice above it, the desktop's controls. iOS 18–25 keep the plain grouped
    /// fill they had, with no imitation of glass.
    @ViewBuilder
    func glassPanel(cornerRadius: CGFloat = 22, interactive: Bool = false) -> some View {
        if #available(iOS 26.0, *) {
            glassEffect(interactive ? .regular.interactive() : .regular, in: .rect(cornerRadius: cornerRadius, style: .continuous))
        } else {
            background(Color(.secondarySystemGroupedBackground), in: .rect(cornerRadius: cornerRadius, style: .continuous))
        }
    }
}

extension View {
    /// Floats glass controls along the bottom edge with no backing of their own. On iOS 26 this is a
    /// safe-area bar, so content runs edge to edge beneath it and gets the system scroll-edge effect.
    @ViewBuilder
    func floatingBar(@ViewBuilder _ bar: () -> some View) -> some View {
        if #available(iOS 26.0, *) {
            safeAreaBar(edge: .bottom, content: bar)
        } else {
            safeAreaInset(edge: .bottom, content: bar)
        }
    }
}

/// Groups adjacent floating glass controls so they blend and morph together on iOS 26.
struct GlassGroup<Content: View>: View {
    var spacing: CGFloat
    @ViewBuilder var content: Content

    init(spacing: CGFloat = 12, @ViewBuilder content: () -> Content) {
        self.spacing = spacing
        self.content = content()
    }

    var body: some View {
        if #available(iOS 26.0, *) {
            GlassEffectContainer(spacing: spacing) { content }
        } else {
            content
        }
    }
}
