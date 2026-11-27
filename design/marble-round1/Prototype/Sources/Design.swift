import SwiftUI

struct Palette {
    let dark: Bool
    var paper: Color { tone(0xF7F2EB, 0x201B1A) }
    var card: Color { tone(0xFFFBF6, 0x2D2523) }
    var wash: Color { tone(0xEEE3E0, 0x3C2C32) }
    var line: Color { tone(0xE2D6CB, 0x51443F) }
    var ink: Color { tone(0x282022, 0xFFF4E9) }
    var muted: Color { tone(0x695B59, 0xCBB9AD) }
    var fig: Color { tone(0x653E52, 0xE3B7CA) }
    var markClay: Color { tone(0xA05D43, 0xDEA28F) }
    var onFig: Color { tone(0xFFF8F2, 0x35212C) }
    var good: Color { tone(0x425A38, 0xBBD6A7) }
    var wait: Color { tone(0x76511F, 0xE9C284) }
    var bad: Color { tone(0x913B42, 0xF5ABAF) }

    func tone(_ light: UInt32, _ darkValue: UInt32) -> Color {
        Color(hex: dark ? darkValue : light)
    }
}

extension Color {
    init(hex: UInt32) {
        self.init(red: Double(hex >> 16) / 255,
                  green: Double((hex >> 8) & 255) / 255,
                  blue: Double(hex & 255) / 255)
    }
}

enum Form: String, CaseIterable {
    case circle, hex, shield, drop, star, square
    var label: String {
        switch self {
        case .hex: "Hexagon"
        case .drop: "Drop"
        default: rawValue.capitalized
        }
    }
}

struct FaceOutline: Shape {
    let form: Form
    func path(in rect: CGRect) -> Path {
        func p(_ x: CGFloat, _ y: CGFloat) -> CGPoint {
            CGPoint(x: rect.minX + x * rect.width, y: rect.minY + y * rect.height)
        }
        switch form {
        case .circle: return Path(ellipseIn: rect)
        case .square: return RoundedRectangle(cornerRadius: rect.width * 0.29, style: .continuous).path(in: rect)
        case .hex, .shield, .drop, .star:
            let vertices: [CGPoint]
            switch form {
            case .hex: vertices = [p(0.23, 0.05), p(0.77, 0.05), p(1, 0.5), p(0.77, 0.95), p(0.23, 0.95), p(0, 0.5)]
            case .shield: vertices = [p(0.5, 0.02), p(0.96, 0.16), p(0.89, 0.69), p(0.5, 1), p(0.11, 0.69), p(0.04, 0.16)]
            case .drop:
                var drop = Path()
                drop.move(to: p(0.5, 0))
                drop.addCurve(to: p(0.95, 0.60), control1: p(0.62, 0), control2: p(0.95, 0.42))
                drop.addCurve(to: p(0.5, 1), control1: p(0.95, 0.88), control2: p(0.76, 1))
                drop.addCurve(to: p(0.05, 0.60), control1: p(0.24, 1), control2: p(0.05, 0.88))
                drop.addCurve(to: p(0.5, 0), control1: p(0.05, 0.42), control2: p(0.38, 0))
                return drop
            default:
                vertices = (0..<10).map { i in
                    let angle = Double(i) * .pi / 5 - .pi / 2
                    let r = i.isMultiple(of: 2) ? 0.52 : 0.35
                    return p(0.5 + cos(angle) * r, 0.53 + sin(angle) * r)
                }
            }
            func interpolate(_ a: CGPoint, _ b: CGPoint) -> CGPoint {
                CGPoint(x: a.x * 0.78 + b.x * 0.22, y: a.y * 0.78 + b.y * 0.22)
            }
            var path = Path()
            for i in vertices.indices {
                let v = vertices[i]
                let start = interpolate(v, vertices[(i + vertices.count - 1) % vertices.count])
                let end = interpolate(v, vertices[(i + 1) % vertices.count])
                if i == 0 { path.move(to: start) } else { path.addLine(to: start) }
                path.addQuadCurve(to: end, control: v)
            }
            path.closeSubpath()
            return path
        }
    }
}

struct Hue: Identifiable, Hashable {
    let name: String
    let light: UInt32
    let dark: UInt32
    var id: String { name }
    static let all: [Hue] = [
        .init(name: "Clay", light: 0xEAB5A5, dark: 0xDEA28F),
        .init(name: "Moss", light: 0xC4D0A3, dark: 0xB3C48C),
        .init(name: "Plum", light: 0xD8BFDE, dark: 0xC9A8D1),
        .init(name: "Honey", light: 0xECC988, dark: 0xDDB570),
        .init(name: "Iris", light: 0xC5C9E9, dark: 0xADB4DD),
        .init(name: "Rose", light: 0xE9BDD0, dark: 0xDDA9C3)
    ]
}

struct Person: Identifiable, Hashable {
    let name: String
    let note: String
    let form: Form
    let hue: Hue
    var id: String { name }
    static let ada = Person(name: "Ada", note: "Makes a plan feel doable", form: .hex, hue: Hue.all[0])
    static let ben = Person(name: "Ben", note: "Finds the quiet spots", form: .drop, hue: Hue.all[1])
    static let mina = Person(name: "Mina", note: "Always brings another idea", form: .square, hue: Hue.all[2])
    static let all = [ada, ben, mina]
}

struct Avatar: View {
    let form: Form
    let hue: Hue
    let size: CGFloat
    var working = false
    @Environment(\.colorScheme) private var scheme
    var body: some View {
        let p = Palette(dark: scheme == .dark)
        ZStack(alignment: .bottomTrailing) {
            FaceOutline(form: form)
                .fill(p.tone(hue.light, hue.dark))
                .overlay {
                    FaceOutline(form: form).stroke(p.tone(0x8F736F, 0x51443F), lineWidth: 1)
                }
                .overlay {
                    HStack(spacing: size * 0.13) {
                        Capsule().frame(width: size * 0.076, height: size * 0.115)
                        Capsule().frame(width: size * 0.076, height: size * 0.115)
                    }
                    .foregroundStyle(Color(hex: 0x282022))
                    .offset(y: size * (form == .drop ? 0.10 : 0.025))
                }
            if working {
                Circle().fill(p.wait).strokeBorder(p.paper, lineWidth: 2)
                    .frame(width: max(9, size * 0.24), height: max(9, size * 0.24))
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}

struct PebbleMark: View {
    enum Concept: String, CaseIterable {
        case pocket = "Pocket Pair"
        case seam = "Keepsake"
        case cluster = "Patchwork"
    }
    let concept: Concept
    let colors: [Color]

    var body: some View {
        Canvas { context, size in
            let s = min(size.width, size.height)
            func piece(_ form: Form, _ x: Double, _ y: Double, _ w: Double, _ h: Double, _ color: Int) {
                let rect = CGRect(x: x * s, y: y * s, width: w * s, height: h * s)
                context.fill(FaceOutline(form: form).path(in: rect), with: .color(colors[color]))
            }
            switch concept {
            case .pocket:
                piece(.hex, 0.02, 0.11, 0.62, 0.60, 0)
                piece(.drop, 0.60, 0.45, 0.38, 0.44, 1)
            case .seam:
                var top = Path()
                top.move(to: CGPoint(x: 0.12*s, y: 0.53*s))
                top.addQuadCurve(to: CGPoint(x: 0.50*s, y: 0.09*s), control: CGPoint(x: 0.11*s, y: 0.09*s))
                top.addQuadCurve(to: CGPoint(x: 0.88*s, y: 0.38*s), control: CGPoint(x: 0.80*s, y: 0.09*s))
                top.addQuadCurve(to: CGPoint(x: 0.12*s, y: 0.53*s), control: CGPoint(x: 0.63*s, y: 0.70*s))
                top.closeSubpath()
                context.fill(top, with: .color(colors[0]))
                var base = Path()
                base.move(to: CGPoint(x: 0.89*s, y: 0.47*s))
                base.addQuadCurve(to: CGPoint(x: 0.56*s, y: 0.91*s), control: CGPoint(x: 0.92*s, y: 0.88*s))
                base.addQuadCurve(to: CGPoint(x: 0.13*s, y: 0.62*s), control: CGPoint(x: 0.20*s, y: 0.94*s))
                base.addQuadCurve(to: CGPoint(x: 0.89*s, y: 0.47*s), control: CGPoint(x: 0.67*s, y: 0.75*s))
                base.closeSubpath()
                context.fill(base, with: .color(colors[1]))
            case .cluster:
                piece(.square, 0.09, 0.09, 0.36, 0.36, 0)
                piece(.circle, 0.55, 0.09, 0.36, 0.36, 1)
                piece(.drop, 0.09, 0.55, 0.36, 0.36, 1)
                piece(.hex, 0.55, 0.55, 0.36, 0.36, 0)
            }
        }
        .accessibilityHidden(true)
    }
}
