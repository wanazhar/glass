import SwiftUI

/// The soft chromatic field Glass content floats over.
struct Backdrop: View {
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        ZStack {
            (colorScheme == .dark ? Color(red: 0.03, green: 0.04, blue: 0.08) : Color(red: 0.95, green: 0.96, blue: 0.99))
            orb(Color(red: 0.36, green: 0.62, blue: 1.0), x: -0.35, y: -0.30, size: 1.1)
            orb(Color(red: 0.55, green: 0.32, blue: 0.95), x: 0.45, y: 0.10, size: 0.9)
            orb(Color(red: 0.20, green: 0.82, blue: 0.86), x: -0.15, y: 0.55, size: 0.8)
        }
        .ignoresSafeArea()
        .accessibilityHidden(true)
    }

    private func orb(_ color: Color, x: CGFloat, y: CGFloat, size: CGFloat) -> some View {
        GeometryReader { proxy in
            let side = max(proxy.size.width, proxy.size.height) * size
            Circle()
                .fill(color.opacity(colorScheme == .dark ? 0.35 : 0.30))
                .frame(width: side, height: side)
                .blur(radius: side * 0.22)
                .position(
                    x: proxy.size.width * (0.5 + x),
                    y: proxy.size.height * (0.5 + y)
                )
        }
    }
}
