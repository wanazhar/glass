import SwiftUI
import UIKit

struct LensField: View {
    var striped = false
    var centerFraction = 0.46
    var dark: Bool

    var body: some View {
        let background = dark ? 0x101B23 : 0xE5EDF4
        let line = Color(hex: dark ? 0x8EBDD0 : 0x4C6977)
        ZStack {
            Color(hex: UInt32(background))
            Canvas { context, size in
                let horizon = size.height * centerFraction
                let offsets = striped ? Array(stride(from: -size.height, through: size.height, by: 7)) : [-34, 0, 34]
                for offset in offsets {
                    let y = horizon + offset
                    let path = Horizon(bend: offset == 0 ? 0.10 : 0.04, fullWidth: true)
                        .path(in: CGRect(x: 0, y: y - 75, width: size.width, height: 150))
                    context.stroke(path, with: .color(line.opacity(striped ? 0.19 : 0.39)),
                                   lineWidth: offset == 0 ? 1 : 0.7)
                }
            }
        }
        .accessibilityHidden(true)
    }
}

struct SplashScene: View {
    var kind: MarkKind = .lens
    var frozen: Double?
    var showsControls = true
    var striped = false
    var finish: () -> Void = {}
    @Environment(\.palette) private var p
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var started = Date.now
    @State private var origin: CGPoint?
    @State private var run = 0

    private let duration = 1.55

    var body: some View {
        GeometryReader { proxy in
            let center = CGPoint(x: proxy.size.width / 2, y: proxy.size.height * 0.46)
            let diameter = min(proxy.size.width * 0.31, proxy.size.height * 0.54, 144)
            TimelineView(.animation(paused: frozen != nil || reduceMotion)) { context in
                let elapsed = frozen ?? context.date.timeIntervalSince(started)
                LensField(striped: striped, dark: striped || p.dark)
                    .drawingGroup()
                    .layerEffect(
                        ShaderLibrary.refraction(
                            .float2(origin ?? center), .float(elapsed), .float(duration),
                            .float2(proxy.size.width, proxy.size.height)
                        ),
                        maxSampleOffset: CGSize(width: 25, height: 25),
                        isEnabled: !reduceMotion && elapsed >= 0 && elapsed < duration
                    )
                    .overlay {
                        LensMark(kind: kind, optical: true, fixedPalette: Palette(dark: striped || p.dark))
                            .frame(width: diameter, height: diameter)
                            .position(center)
                            .allowsHitTesting(false)
                    }
                .contentShape(.rect)
                .onTapGesture(coordinateSpace: .local) { point in
                    guard showsControls else { return }
                    origin = point
                    started = .now
                    run += 1
                    UIImpactFeedbackGenerator(style: .soft).impactOccurred(intensity: 0.55)
                }
            }
            if showsControls {
                VStack(alignment: .leading) {
                    HStack {
                        Eyebrow("GLASS / LENS")
                        Spacer()
                        Button("Skip", action: finish)
                            .font(.subheadline.weight(.semibold))
                            .padding(.horizontal, 18)
                            .frame(height: 44)
                            .glassEffect(.regular.interactive(), in: .capsule)
                    }
                    Spacer()
                }
                .padding(24)
            }
        }
        .environment(\.colorScheme, striped ? .dark : (p.dark ? .dark : .light))
        .task(id: run) {
            guard showsControls, frozen == nil else { return }
            if !reduceMotion { try? await Task.sleep(for: .seconds(duration)) }
            if !Task.isCancelled { finish() }
        }
    }
}
