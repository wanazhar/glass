import SwiftUI

struct MarbleSilhouette: Shape {
    let glyph: String

    func path(in rect: CGRect) -> Path {
        func point(_ x: Double, _ y: Double) -> CGPoint {
            CGPoint(x: rect.minX + x * rect.width, y: rect.minY + y * rect.height)
        }
        let vertices: [CGPoint]
        switch glyph {
        case "hex":
            vertices = [point(0.23, 0.05), point(0.77, 0.05), point(1, 0.5),
                        point(0.77, 0.95), point(0.23, 0.95), point(0, 0.5)]
        case "shield":
            vertices = [point(0.5, 0.02), point(0.96, 0.16), point(0.89, 0.69),
                        point(0.5, 1), point(0.11, 0.69), point(0.04, 0.16)]
        case "star":
            vertices = (0..<10).map { index in
                let angle = Double(index) * .pi / 5 - .pi / 2
                let radius = index.isMultiple(of: 2) ? 0.48 : 0.33
                return point(0.5 + cos(angle) * radius, 0.5 + sin(angle) * radius)
            }
        case "drop":
            var path = Path()
            path.move(to: point(0.5, 0))
            path.addCurve(to: point(0.95, 0.60), control1: point(0.62, 0), control2: point(0.95, 0.42))
            path.addCurve(to: point(0.5, 1), control1: point(0.95, 0.88), control2: point(0.76, 1))
            path.addCurve(to: point(0.05, 0.60), control1: point(0.24, 1), control2: point(0.05, 0.88))
            path.addCurve(to: point(0.5, 0), control1: point(0.05, 0.42), control2: point(0.38, 0))
            path.closeSubpath()
            return path
        case "square":
            return RoundedRectangle(cornerRadius: rect.width * 0.29).path(in: rect)
        default:
            return Path(ellipseIn: rect)
        }
        func inset(_ corner: CGPoint, toward adjacent: CGPoint) -> CGPoint {
            CGPoint(x: corner.x * 0.78 + adjacent.x * 0.22, y: corner.y * 0.78 + adjacent.y * 0.22)
        }
        var path = Path()
        for index in vertices.indices {
            let corner = vertices[index]
            let start = inset(corner, toward: vertices[(index + vertices.count - 1) % vertices.count])
            let end = inset(corner, toward: vertices[(index + 1) % vertices.count])
            if index == 0 { path.move(to: start) } else { path.addLine(to: start) }
            path.addQuadCurve(to: end, control: corner)
        }
        path.closeSubpath()
        return path
    }
}

struct PocketPair: View {
    var settled = true

    var body: some View {
        GeometryReader { geometry in
            let size = min(geometry.size.width, geometry.size.height)
            MarbleSilhouette(glyph: "hex")
                .fill(MarbleTheme.clay)
                .frame(width: 0.62 * size, height: 0.60 * size)
                .position(x: (settled ? 0.33 : 0.28) * size, y: (settled ? 0.41 : 0.35) * size)
            MarbleSilhouette(glyph: "drop")
                .fill(MarbleTheme.moss)
                .frame(width: 0.38 * size, height: 0.44 * size)
                .position(x: (settled ? 0.79 : 0.85) * size, y: (settled ? 0.67 : 0.73) * size)
        }
        .accessibilityHidden(true)
    }
}
