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
    @State private var appeared = false
    @State private var inViewport = false

    var body: some View {
        let color = MarbleAvatarAppearance.fill(for: hue)
        Group {
            if let isWorking, !reduceMotion {
                AvatarLayer(glyph: glyph, color: color, working: isWorking,
                            enabled: appeared && inViewport,
                            settledAt: settledAt)
            } else {
                MarbleSilhouette(glyph: glyph).fill(color)
                    .padding(isWorking == nil ? 0 : size * 0.075)
            }
        }
        .frame(width: size, height: size)
        .onGeometryChange(for: Bool.self) { geometry in
            guard let viewport = geometry.bounds(of: .scrollView) else { return true }
            return CGRect(origin: .zero, size: viewport.size).intersects(geometry.frame(in: .scrollView))
        } action: { inViewport = $0 }
        .onAppear { appeared = true }
        .onDisappear { appeared = false }
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
    private var sceneActive = false
    private var settledAt: Date?
    private var playingWork: Bool?

    override init(frame: CGRect) {
        super.init(frame: frame)
        isUserInteractionEnabled = false
        layer.addSublayer(shape)
        NotificationCenter.default.addObserver(self, selector: #selector(sceneActivated(_:)),
                                               name: UIScene.didActivateNotification, object: nil)
        NotificationCenter.default.addObserver(self, selector: #selector(sceneDeactivating(_:)),
                                               name: UIScene.willDeactivateNotification, object: nil)
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
        reconcile()
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        sceneActive = window?.windowScene?.activationState == .foregroundActive
        reconcile()
    }

    @objc private func sceneActivated(_ notification: Notification) {
        guard let scene = window?.windowScene, notification.object as? UIScene === scene else { return }
        sceneActive = true
        reconcile()
    }

    @objc private func sceneDeactivating(_ notification: Notification) {
        guard let scene = window?.windowScene, notification.object as? UIScene === scene else { return }
        sceneActive = false
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
        guard enabled, sceneActive, !bounds.isEmpty else {
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
