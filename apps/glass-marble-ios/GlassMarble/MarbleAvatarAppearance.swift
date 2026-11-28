import SwiftUI
import UIKit

enum MarbleAvatarAppearance {
    static let hues: [(degrees: Int, name: String, fill: Color)] = [
        (0, "Red", adaptive(0xDC6469, 0xF48388)),
        (105, "Green", adaptive(0x88B35D, 0xA6CB76)),
        (180, "Teal", adaptive(0x59ABAB, 0x7AC5C4)),
        (210, "Blue", adaptive(0x639AD1, 0x82B4E4)),
        (15, "Clay", adaptive(0xEAB5A5, 0xDEA28F)), (27, "Peach", adaptive(0xF6C8AE, 0xE9AD92)),
        (85, "Moss", adaptive(0xC4D0A3, 0xB3C48C)), (280, "Plum", adaptive(0xA98ACC, 0xBA9CDD)),
        (42, "Honey", adaptive(0xD7AB54, 0xECC275)), (230, "Iris", adaptive(0xC5C9E9, 0xADB4DD)),
        (329, "Rose", adaptive(0xDA81AE, 0xEBA0C6)),
        (152, "Mint", adaptive(0x6ABF9C, 0x8AD2AF)),
        (196, "Sky", adaptive(0xA7D6E7, 0x94C9DF)),
        (56, "Lemon", adaptive(0xEDE3A0, 0xD9CE87))
    ]

    static func fill(for hue: Int) -> Color {
        hues.first { $0.degrees == hue }?.fill
            ?? Color(hue: Double((hue % 360 + 360) % 360) / 360, saturation: 0.55, brightness: 0.85)
    }

    private static func adaptive(_ light: UInt32, _ dark: UInt32) -> Color {
        Color(uiColor: UIColor { traits in
            let hex = traits.userInterfaceStyle == .dark ? dark : light
            return UIColor(red: Double(hex >> 16) / 255, green: Double(hex >> 8 & 255) / 255,
                           blue: Double(hex & 255) / 255, alpha: 1)
        })
    }
}

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
        case "diamond":
            vertices = [point(0.5, 0), point(1, 0.5), point(0.5, 1), point(0, 0.5)]
        case "triangle":
            vertices = [point(0.5, 0), point(1, 0.92), point(0, 0.92)]
        case "capsule":
            return Capsule().path(in: rect.insetBy(dx: rect.width * 0.24, dy: 0))
        case "petal":
            vertices = [point(0.04, 0.2), point(0.61, 0), point(0.98, 0.4),
                        point(0.8, 0.96), point(0.28, 0.89)]
        case "cross":
            vertices = [point(0.3, 0), point(0.7, 0), point(0.7, 0.3), point(1, 0.3),
                        point(1, 0.7), point(0.7, 0.7), point(0.7, 1), point(0.3, 1),
                        point(0.3, 0.7), point(0, 0.7), point(0, 0.3), point(0.3, 0.3)]
        case "moon":
            var path = Path()
            path.move(to: point(0.85, 0.04))
            path.addQuadCurve(to: point(0.84, 0.94), control: point(0.19, 0.51))
            path.addCurve(to: point(0.85, 0.04), control1: point(-0.24, 1.28), control2: point(-0.23, -0.24))
            return path
        case "clover":
            var path = Path()
            for (x, y) in [(0.0, 0.25), (0.5, 0.25), (0.25, 0.0), (0.25, 0.5), (0.25, 0.25)] {
                path.addEllipse(in: CGRect(origin: point(x, y), size: CGSize(width: rect.width / 2, height: rect.height / 2)))
            }
            return path
        default:
            if let index = Int(glyph.dropFirst("roster-".count)), glyph.hasPrefix("roster-"),
               (1...32).contains(index) {
                let count = 5 + (index - 1) % 8
                vertices = (0..<count * 2).map { vertex in
                    let angle = Double(vertex) * .pi / Double(count) - .pi / 2
                    let radius = vertex.isMultiple(of: 2) ? 0.49 : 0.37 - 0.06 * Double((index - 1) / 8)
                    return point(0.5 + cos(angle) * radius, 0.5 + sin(angle) * radius)
                }
                break
            }
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
