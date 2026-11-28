import SwiftUI
import QuartzCore
import UIKit

struct MarbleAvatar: View {
    var glyph: String
    var hue: Int
    var size: CGFloat = 22
    /// A nil value keeps pickers and design previews static.
    var isWorking: Bool? = nil
    var settledAt: Date? = nil

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.scenePhase) private var scenePhase
    @State private var appeared = false
    @State private var inViewport = false

    var body: some View {
        let color = MarbleTheme.hues.first { $0.degrees == hue }?.fill
            ?? MarbleTheme.marble(hue: hue, brightness: 0.85)
        Group {
            if let isWorking, !reduceMotion {
                AvatarLayer(glyph: glyph, color: color, working: isWorking,
                            enabled: appeared && inViewport && scenePhase == .active,
                            settledAt: settledAt)
                    .onGeometryChange(for: Bool.self) { geometry in
                        guard let viewport = geometry.bounds(of: .scrollView) else { return true }
                        return viewport.intersects(CGRect(origin: .zero, size: geometry.size))
                    } action: { inViewport = $0 }
                    .onAppear { appeared = true }
                    .onDisappear { appeared = false }
            } else {
                MarbleSilhouette(glyph: glyph).fill(color)
                    .padding(isWorking == nil ? 0 : size * 0.075)
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}

/// Keep path drawing off the animation loop and let Core Animation drive the transform.
private struct AvatarLayer: UIViewRepresentable {
    var glyph: String
    var color: Color
    var working: Bool
    var enabled: Bool
    var settledAt: Date?

    func makeUIView(context: Context) -> AvatarLayerView { AvatarLayerView(frame: .zero) }

    func updateUIView(_ view: AvatarLayerView, context: Context) {
        view.update(glyph: glyph, color: UIColor(color), working: working,
                    enabled: enabled, settledAt: settledAt)
    }
}

private final class AvatarLayerView: UIView {
    private let shape = CAShapeLayer()
    private var glyph = ""
    private var color = UIColor.clear
    private var working = false
    private var enabled = false
    private var settledAt: Date?
    private var playingWork: Bool?

    override init(frame: CGRect) {
        super.init(frame: frame)
        isUserInteractionEnabled = false
        layer.addSublayer(shape)
        registerForTraitChanges([UITraitUserInterfaceStyle.self]) { (view: AvatarLayerView, _: UITraitCollection) in
            view.updateFill()
        }
    }

    required init?(coder: NSCoder) { nil }

    override func layoutSubviews() {
        super.layoutSubviews()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        // Bounds and position remain valid while the presentation transform runs.
        shape.bounds = CGRect(origin: .zero, size: bounds.size)
        shape.position = CGPoint(x: bounds.midX, y: bounds.midY)
        let inset = min(bounds.width, bounds.height) * 0.075
        shape.path = MarbleSilhouette(glyph: glyph).path(in: shape.bounds.insetBy(dx: inset, dy: inset)).cgPath
        CATransaction.commit()
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        reconcile()
    }

    func update(glyph: String, color: UIColor, working: Bool, enabled: Bool, settledAt: Date?) {
        if self.glyph != glyph {
            self.glyph = glyph
            setNeedsLayout()
        }
        if self.color != color {
            self.color = color
            updateFill()
        }
        self.working = working
        self.enabled = enabled
        self.settledAt = settledAt
        reconcile()
    }

    private func updateFill() {
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        shape.fillColor = color.resolvedColor(with: traitCollection).cgColor
        CATransaction.commit()
    }

    private func reconcile() {
        guard enabled, window != nil else {
            if playingWork != nil {
                shape.removeAllAnimations()
                playingWork = nil
            }
            return
        }
        let previous = playingWork
        guard previous != working else { return }
        let freshReply = settledAt.map { Date().timeIntervalSince($0) < 0.7 } == true
        let current = shape.presentation()?.transform ?? CATransform3DIdentity
        let startingPose = previous == nil && freshReply && !working
            ? pose(scale: 1.07, stretch: 0.03, degrees: 7) : current
        let settleTime = working ? 0.22 : 0.65
        let transitionTime = previous != nil || (freshReply && !working) ? settleTime : 0
        playingWork = working
        shape.removeAllAnimations()

        if transitionTime > 0 {
            let settle = CABasicAnimation(keyPath: "transform")
            settle.fromValue = NSValue(caTransform3D: startingPose)
            settle.toValue = NSValue(caTransform3D: CATransform3DIdentity)
            settle.duration = transitionTime
            settle.timingFunction = CAMediaTimingFunction(name: .easeOut)
            settle.preferredFrameRateRange = CAFrameRateRange(minimum: 60, maximum: 120, preferred: 60)
            shape.add(settle, forKey: "settle")
        }

        let cycle = CAKeyframeAnimation(keyPath: "transform")
        let expanded = pose(scale: working ? 1.08 : 1.025,
                            stretch: working ? 0.03 : 0.012, degrees: working ? 8 : 3)
        let contracted = pose(scale: working ? 0.96 : 0.985,
                              stretch: working ? -0.02 : -0.008, degrees: working ? -6 : -3)
        cycle.values = [CATransform3DIdentity, expanded, CATransform3DIdentity, contracted, CATransform3DIdentity]
            .map { NSValue(caTransform3D: $0) }
        cycle.timingFunctions = Array(repeating: CAMediaTimingFunction(name: .easeInEaseOut), count: 4)
        cycle.duration = working ? 1.55 : 5.6
        cycle.repeatCount = .infinity
        cycle.preferredFrameRateRange = working
            ? CAFrameRateRange(minimum: 60, maximum: 120, preferred: 120)
            : CAFrameRateRange(minimum: 24, maximum: 120, preferred: 30)
        if transitionTime > 0 {
            cycle.beginTime = shape.convertTime(CACurrentMediaTime(), from: nil) + transitionTime
        }
        shape.add(cycle, forKey: "breathe")
    }

    private func pose(scale: CGFloat, stretch: CGFloat, degrees: CGFloat) -> CATransform3D {
        let rotation = CATransform3DMakeRotation(degrees * .pi / 180, 0, 0, 1)
        return CATransform3DScale(rotation, scale + stretch, scale - stretch, 1)
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
