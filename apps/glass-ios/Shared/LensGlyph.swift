import SwiftUI

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

/// The Lens drawn from shapes, for places that can't use the app's asset catalog, like the Live Activity.
struct LensGlyph: View {
    var tint: Color

    var body: some View {
        GeometryReader { proxy in
            let size = min(proxy.size.width, proxy.size.height)
            let stroke = max(1.5, size / 12)
            ZStack {
                Circle().fill(tint.opacity(0.16))
                Circle().strokeBorder(tint, lineWidth: stroke)
                Horizon(bend: 0.16, fullWidth: true)
                    .stroke(tint, style: StrokeStyle(lineWidth: stroke * 0.8, lineCap: .round))
                    .padding(stroke * 0.5)
                    .clipShape(.circle)
                Capsule().fill(tint.opacity(0.9))
                    .frame(width: size * 0.22, height: max(1, stroke * 0.55))
                    .rotationEffect(.degrees(-40))
                    .position(x: size * 0.33, y: size * 0.27)
            }
            .frame(width: size, height: size)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .accessibilityHidden(true)
    }
}
