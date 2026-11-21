import SwiftUI

/// First screen until pairing lands in M1: identity and version of this build.
struct HomeView: View {
    let info: AppInfo

    var body: some View {
        ZStack {
            Backdrop()
            VStack(spacing: 28) {
                Spacer()
                Image("AppGlyph")
                    .resizable()
                    .scaledToFit()
                    .frame(width: 112, height: 112)
                    .clipShape(.rect(cornerRadius: 26, style: .continuous))
                    .shadow(color: .black.opacity(0.18), radius: 18, y: 10)
                    .accessibilityHidden(true)
                VStack(spacing: 8) {
                    Text(info.displayName)
                        .font(.system(.largeTitle, design: .rounded, weight: .bold))
                        .accessibilityIdentifier("home.title")
                    Text("Your Glass host, native on iPhone.")
                        .font(.body)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                }
                Spacer()
                GlassGroup {
                    Label {
                        Text("Version \(info.versionLabel)")
                            .monospacedDigit()
                    } icon: {
                        Image(systemName: "sparkles")
                    }
                    .font(.footnote.weight(.medium))
                    .padding(.horizontal, 16)
                    .padding(.vertical, 10)
                    .glassSurface()
                    .accessibilityElement(children: .combine)
                    .accessibilityIdentifier("home.version")
                }
                .padding(.bottom, 24)
            }
            .padding(.horizontal, 32)
        }
    }
}

#Preview {
    HomeView(info: AppInfo(displayName: "Glass", bundleIdentifier: "com.wanazhar.glass", version: "0.1.0", build: "1"))
}
