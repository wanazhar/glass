import SwiftUI

struct GlassLogo: View {
    var launchRipple = false
    var font: Font = .system(.largeTitle, design: .rounded, weight: .semibold)
    var sample: Float?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var touch: CGPoint = .zero
    @State private var started = Date.distantPast
    @State private var active = false

    var body: some View {
        let touch = touch
        let started = started
        let shouldAnimate = (sample.map { $0 > 0 } ?? active) && !reduceMotion
        TimelineView(.animation(minimumInterval: 1.0 / 120, paused: !shouldAnimate)) { frame in
            let time = sample ?? Float(frame.date.timeIntervalSince(started))
            Group {
                if shouldAnimate {
                    wordmark.visualEffect { view, _ in
                        view.layerEffect(
                            ShaderLibrary.liquidRipple(.float2(touch), .float(time)),
                            maxSampleOffset: CGSize(width: 12, height: 12)
                        )
                    }
                } else {
                    wordmark
                }
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

    private var wordmark: some View {
        Text("Glass").font(font).foregroundStyle(.primary).padding(Design.padding)
    }

    private func ripple(at point: CGPoint) {
        guard !reduceMotion else { return }
        touch = point
        started = .now
        active = true
    }
}

struct LaunchView: View {
    var sample: Float?
    var skip: () -> Void

    var body: some View {
        VStack(spacing: Design.spacious) {
            GlassLogo(launchRipple: true, font: .system(size: 64, weight: .semibold, design: .rounded), sample: sample)
                .padding(.horizontal, Design.spacious)
                .padding(.vertical, Design.padding)
                .background {
                    Color.clear
                        .glassEffect(.regular.tint(Design.accent.opacity(0.18)), in: .rect(cornerRadius: 40))
                }
            Button("Skip", action: skip)
                .font(.footnote.weight(.medium))
                .glassButtonStyle()
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background {
            Design.background
                .overlay { Design.accent.opacity(0.07) }
                .ignoresSafeArea()
        }
        .accessibilityIdentifier("launch.splash")
    }
}

struct NewChatView: View {
    @Environment(AppModel.self) private var app
    var choose: (String) -> Void
    private let suggestions = [
        ("Map the project", "Summarize the project and show me where to start.", "map"),
        ("Review recent changes", "Review the latest changes and flag anything worth a closer look.", "arrow.triangle.branch"),
        ("Plan a change", "Help me outline and test my next change.", "list.bullet.rectangle"),
    ]

    var body: some View {
        VStack(spacing: Design.spacious) {
            VStack(spacing: Design.gap) {
                Image(.appGlyph)
                    .resizable().scaledToFit()
                    .frame(width: 50, height: 50)
                    .clipShape(.rect(cornerRadius: Design.corner))
                    .padding(Design.padding)
                    .glassEffect(.regular.tint(Design.accent.opacity(0.13)).interactive(), in: .circle)
                    .accessibilityHidden(true)
                Text("What are we working on?")
                    .font(.largeTitle.weight(.semibold))
                    .multilineTextAlignment(.center)
                    .accessibilityAddTraits(.isHeader)
                if let project = app.status?.workspaceName, let host = app.endpoint?.host {
                    Label("\(project) on \(host)", systemImage: "desktopcomputer")
                        .font(.subheadline).foregroundStyle(.secondary)
                }
            }
            VStack(spacing: Design.gap) {
                ForEach(suggestions.indices, id: \.self) { index in
                    let (title, prompt, icon) = suggestions[index]
                    Button { choose(prompt) } label: {
                        Label(title, systemImage: icon)
                            .font(.subheadline.weight(.medium))
                            .frame(maxWidth: .infinity, minHeight: Design.control, alignment: .leading)
                            .padding(.horizontal, Design.padding)
                            .background(Design.surface, in: .rect(cornerRadius: Design.corner))
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .frame(maxWidth: 460)
        .frame(maxWidth: .infinity)
        .padding(.vertical, Design.spacious)
    }
}
