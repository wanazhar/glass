import SwiftUI

/// Liquid Glass on iOS 26 and later; a material surface with a hairline edge on iOS 18–25.
struct GlassSurface<S: InsettableShape>: ViewModifier {
    var shape: S
    var tint: Color?
    var interactive: Bool

    func body(content: Content) -> some View {
        if #available(iOS 26.0, *) {
            content.glassEffect(glass, in: shape)
        } else {
            content
                .background {
                    shape.fill(.ultraThinMaterial)
                    if let tint { shape.fill(tint.opacity(0.18)) }
                }
                .overlay { shape.strokeBorder(.white.opacity(0.22), lineWidth: 0.5) }
                .shadow(color: .black.opacity(0.10), radius: 12, y: 6)
        }
    }

    @available(iOS 26.0, *)
    private var glass: Glass {
        var glass = Glass.regular
        if let tint { glass = glass.tint(tint) }
        if interactive { glass = glass.interactive() }
        return glass
    }
}

extension View {
    func glassSurface<S: InsettableShape>(in shape: S, tint: Color? = nil, interactive: Bool = false) -> some View {
        modifier(GlassSurface(shape: shape, tint: tint, interactive: interactive))
    }

    func glassSurface(tint: Color? = nil, interactive: Bool = false) -> some View {
        modifier(GlassSurface(shape: Capsule(), tint: tint, interactive: interactive))
    }
}

/// Groups glass surfaces so they blend and morph together on iOS 26; a plain stack otherwise.
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
