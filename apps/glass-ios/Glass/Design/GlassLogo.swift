import SwiftUI

struct GlassLogo: View {
    var launchRipple = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var touch: CGPoint = .zero
    @State private var started = Date.distantPast
    @State private var active = false

    var body: some View {
        let touch = touch
        let started = started
        let shouldAnimate = active && !reduceMotion
        TimelineView(.animation(minimumInterval: 1.0 / 120, paused: !shouldAnimate)) { frame in
            let time = Float(frame.date.timeIntervalSince(started))
            Text("Glass")
                .font(.system(.largeTitle, design: .rounded, weight: .semibold))
                .padding(Design.padding)
                .visualEffect { view, _ in
                    view.layerEffect(
                        ShaderLibrary.liquidRipple(.float2(touch), .float(time)),
                        maxSampleOffset: CGSize(width: 12, height: 12),
                        isEnabled: shouldAnimate
                    )
                }
                .contentShape(Rectangle())
                .gesture(SpatialTapGesture().onEnded { ripple(at: $0.location) })
                .background {
                    if launchRipple {
                        GeometryReader { geometry in
                            Color.clear.onAppear {
                                ripple(at: CGPoint(x: geometry.size.width * 0.4, y: geometry.size.height * 0.55))
                            }
                        }
                    }
                }
        }
        .task(id: started) {
            guard active else { return }
            try? await Task.sleep(for: .milliseconds(1_300))
            if !Task.isCancelled { active = false }
        }
        .accessibilityAddTraits(.isHeader)
    }

    private func ripple(at point: CGPoint) {
        guard !reduceMotion else { return }
        touch = point
        started = .now
        active = true
    }
}

struct NewChatView: View {
    var launchRipple = false
    var choose: (String) -> Void
    private let suggestions = ["Summarize this project", "Help me plan a change", "Review the latest changes"]

    var body: some View {
        VStack(alignment: .leading, spacing: Design.padding) {
            GlassLogo(launchRipple: launchRipple)
                .padding(.leading, -Design.padding)
            Text("What are we working on?")
                .font(.title3)
            ForEach(suggestions, id: \.self) { suggestion in
                Button(suggestion) { choose(suggestion) }
                    .glassButtonStyle()
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, Design.spacious)
    }
}
