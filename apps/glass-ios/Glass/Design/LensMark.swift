import SwiftUI

struct LensMark: View {
    var optical = false
    var tint: Color?
    @Environment(\.colorScheme) private var scheme
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    var body: some View {
        GeometryReader { proxy in
            let size = min(proxy.size.width, proxy.size.height)
            let stroke = max(0.8, size / 74)
            let dark = scheme == .dark
            let teal = tint ?? Color(lensHex: dark ? 0x51F5F5 : 0x078995)
            let shade = Color(lensHex: 0x596D78)
            ZStack {
                Circle()
                    .stroke(teal.opacity(dark ? 0.42 : 0.22), lineWidth: stroke * 2.6)
                    .blur(radius: size * 0.042)
                    .offset(x: size * 0.025, y: size * 0.055)
                #if os(iOS)
                if optical && !reduceTransparency {
                    Color.clear.glassEffect(.clear, in: .circle)
                }
                #endif
                Circle().fill(LinearGradient(colors: [
                    .white.opacity(dark ? 0.13 : 0.76),
                    Color(lensHex: 0x698794).opacity(dark ? 0.10 : 0.14),
                    teal.opacity(dark ? 0.19 : 0.13)
                ], startPoint: .topLeading, endPoint: .bottomTrailing))
                Circle().fill(RadialGradient(colors: [
                    .clear, Color(lensHex: 0x203C47).opacity(dark ? 0.34 : 0.15),
                    teal.opacity(dark ? 0.20 : 0.07)
                ], center: UnitPoint(x: 0.34, y: 0.24), startRadius: size * 0.28, endRadius: size))
                Circle().stroke(AngularGradient(gradient: Gradient(stops: [
                    .init(color: teal, location: 0),
                    .init(color: teal.opacity(0.85), location: 0.17),
                    .init(color: .white.opacity(0.95), location: 0.31),
                    .init(color: shade, location: 0.51),
                    .init(color: .white.opacity(0.95), location: 0.69),
                    .init(color: shade, location: 0.84),
                    .init(color: teal, location: 1)
                ]), center: .center), lineWidth: stroke)
                Circle().stroke(.white.opacity(dark ? 0.36 : 0.75), lineWidth: stroke * 0.45)
                    .padding(stroke * 1.35)
                Capsule().fill(.white.opacity(0.95))
                    .frame(width: size * 0.27, height: size * 0.045)
                    .rotationEffect(.degrees(-40))
                    .position(x: size * 0.27, y: size * 0.19)
                Horizon(bend: 0.14).stroke(teal.opacity(0.95), lineWidth: stroke)
                    .clipShape(.circle)
            }
            .frame(width: size, height: size)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .accessibilityHidden(true)
    }
}
