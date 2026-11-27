import SwiftUI

struct LaunchView: View {
    static let duration = 1.55

    var sample: Double?
    var skip: () -> Void
    @Environment(\.colorScheme) private var scheme
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @State private var started = Date.now

    var body: some View {
        GeometryReader { proxy in
            let size = proxy.size
            let center = CGPoint(x: size.width / 2, y: size.height * 0.46)
            let diameter = min(size.width * 0.31, size.height * 0.54, 144)
            TimelineView(.animation(paused: sample != nil)) { context in
                let elapsed = sample ?? context.date.timeIntervalSince(started)
                field(center: center, size: size)
                    .drawingGroup()
                    .layerEffect(
                        ShaderLibrary.refraction(
                            .float2(center), .float(elapsed), .float(Self.duration), .float2(size.width, size.height)
                        ),
                        maxSampleOffset: CGSize(width: 25, height: 25),
                        isEnabled: elapsed >= 0 && elapsed < Self.duration
                    )
                    .overlay {
                        LensMark(optical: true)
                            .frame(width: diameter, height: diameter)
                            .position(center)
                            .allowsHitTesting(false)
                    }
            }
        }
        .ignoresSafeArea()
        .contentShape(.rect)
        .onTapGesture(perform: skip)
        .overlay(alignment: .topTrailing) {
            Button("Skip", action: skip)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(Design.ink)
                .padding(.horizontal, 18)
                .frame(height: Design.control)
                .background(reduceTransparency ? Design.surface : .clear, in: .capsule)
                .glassEffect(reduceTransparency ? .identity : .regular.interactive(), in: .capsule)
                .padding(.trailing, Design.spacious)
        }
        .accessibilityIdentifier("launch.splash")
    }

    private func field(center: CGPoint, size: CGSize) -> some View {
        let dark = scheme == .dark
        let line = Color(lensHex: dark ? 0x8EBDD0 : 0x4C6977)
        return Canvas { context, _ in
            for offset: CGFloat in [-34, 0, 34] {
                let y = center.y + offset
                let path = Horizon(bend: offset == 0 ? 0.10 : 0.04, fullWidth: true)
                    .path(in: CGRect(x: 0, y: y - 75, width: size.width, height: 150))
                context.stroke(path, with: .color(line.opacity(0.39)), lineWidth: offset == 0 ? 1 : 0.7)
            }
        }
        .background(Color(lensHex: dark ? 0x101B23 : 0xE5EDF4))
        .accessibilityHidden(true)
    }
}
