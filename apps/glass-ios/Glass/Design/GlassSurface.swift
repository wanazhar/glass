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
