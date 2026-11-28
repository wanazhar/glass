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

struct MarbleAvatarChoices {
    static let builtIns: [String: (glyph: String, hue: Int)] = [
        "marble": ("circle", 210), "lead": ("hex", 42), "engineer": ("square", 152),
        "designer": ("drop", 280), "researcher": ("star", 329), "reviewer": ("shield", 0),
        "finance": ("diamond", 105), "shopper": ("triangle", 180)
    ]
    private static let shapes = MarbleMentions.glyphs
        + ["diamond", "triangle", "capsule", "petal", "cross", "moon", "clover"]
        + (1...32).map { "roster-\($0)" }

    private var usedGlyphs = Set(MarbleAvatarChoices.builtIns.values.map { $0.glyph })
    private var usedHues = MarbleAvatarChoices.builtIns.values.map { $0.hue }

    var glyphs: [String] { Self.shapes.filter { !usedGlyphs.contains($0) } }

    var hues: [Int] {
        let named = MarbleTheme.hues.map(\.degrees).filter { distance($0) >= 8 }
        return named.isEmpty ? [furthestHue] : named
    }

    private var furthestHue: Int {
        (0..<360).max { lhs, rhs in
            let lhsDistance = distance(lhs)
            let rhsDistance = distance(rhs)
            return lhsDistance == rhsDistance ? lhs < rhs : lhsDistance < rhsDistance
        } ?? 0
    }

    private func distance(_ hue: Int) -> Int {
        usedHues.map {
            let delta = abs($0 - hue)
            return min(delta, 360 - delta)
        }.min() ?? 180
    }

    func available(glyph: String, hue: Int) -> (glyph: String, hue: Int) {
        let hue = (hue % 360 + 360) % 360
        return (glyphs.contains(glyph) ? glyph : glyphs.first ?? "petal",
                distance(hue) >= 8 ? hue : furthestHue)
    }

    mutating func assign(_ agent: MarblePrismMarble) -> (glyph: String, hue: Int) {
        if let fixed = Self.builtIns[agent.id] { return fixed }
        let value = available(glyph: agent.glyph, hue: agent.hue)
        usedGlyphs.insert(value.glyph)
        usedHues.append(value.hue)
        return value
    }
}

extension MarbleStore {
    func rosterStyle(for agent: MarblePrismMarble) -> MarbleSpeakerStyle {
        var choices = MarbleAvatarChoices()
        for member in prism.marbles {
            let look = choices.assign(member)
            if member.id == agent.id { return .init(name: agent.name, hue: look.hue, glyph: look.glyph) }
        }
        let look = choices.assign(agent)
        return .init(name: agent.name, hue: look.hue, glyph: look.glyph)
    }

    func avatarChoices(excluding id: String? = nil) -> MarbleAvatarChoices {
        var all = MarbleAvatarChoices()
        var choices = MarbleAvatarChoices()
        for var member in prism.marbles {
            let look = all.assign(member)
            if member.id != id {
                (member.glyph, member.hue) = look
                _ = choices.assign(member)
            }
        }
        return choices
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
