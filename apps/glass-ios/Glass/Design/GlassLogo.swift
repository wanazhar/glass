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
                    ZStack {
                        wordmark.opacity(0.24)
                        wordmark.drawingGroup().visualEffect { view, _ in
                            view.layerEffect(
                                ShaderLibrary.liquidRipple(.float2(touch), .float(time)),
                                maxSampleOffset: CGSize(width: 12, height: 12)
                            )
                        }
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
    var handingOff: Bool
    var skip: () -> Void
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        ZStack(alignment: .bottom) {
            GlassLogo(launchRipple: true, font: .system(size: 66, weight: .medium, design: .rounded), sample: sample)
                .padding(.horizontal, Design.spacious)
                .padding(.vertical, Design.gap)
                .glassEffect(.regular.tint(Design.accent.opacity(0.23)), in: Capsule())
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .scaleEffect(handingOff && !reduceMotion ? 1.12 : 1)
                .opacity(handingOff ? (sample == nil ? 0 : 0.28) : 1)
            Button("Skip", action: skip)
                .font(Design.footnote.weight(.medium))
                .glassButtonStyle()
                .padding(.bottom, Design.spacious)
                .opacity(handingOff ? (sample == nil ? 0 : 0.5) : 1)
        }
        .background {
            ZStack {
                LinearGradient(colors: [Design.background, Design.accent.opacity(colorScheme == .dark ? 0.24 : 0.12), Design.background],
                               startPoint: .topLeading, endPoint: .bottomTrailing)
                Canvas { context, size in
                    let radius = min(size.width, size.height)
                    let center = CGPoint(x: size.width * 0.47, y: size.height * 0.48)
                    for index in 0..<8 {
                        let width = radius * (0.54 + Double(index) * 0.18)
                        let height = width * 0.73
                        let bounds = CGRect(x: center.x - width / 2, y: center.y - height / 2, width: width, height: height)
                        context.stroke(Path(ellipseIn: bounds),
                                       with: .linearGradient(Gradient(colors: [.white.opacity(0.5), Design.accent.opacity(0.18), .clear]),
                                                             startPoint: bounds.origin, endPoint: CGPoint(x: bounds.maxX, y: bounds.maxY)),
                                       lineWidth: 1)
                    }
                }
                .accessibilityHidden(true)
            }
            .background(Design.background)
            .ignoresSafeArea()
        }
        .contentShape(Rectangle())
        .onTapGesture(perform: skip)
        .accessibilityIdentifier("launch.splash")
    }
}

struct NewChatView: View {
    @Environment(AppModel.self) private var app
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var choose: (String) -> Void
    private var project: String { app.status?.workspaceName ?? "this workspace" }
    private var machine: String { app.endpoint?.host ?? "the connected machine" }
    private var suggestions: [(String, String, String, String)] {
        [
            ("Map the project", "Find the main modules and a starting point.", "Summarize \(project) and show me where to start.", "map"),
            ("Review recent changes", "See what changed and where to look closer.", "Review the latest changes in \(project) and flag anything worth a closer look.", "arrow.triangle.branch"),
            ("Check the workspace", "Find the commands for a local test run.", "Find the build and test commands for \(project) on \(machine).", "checklist")
        ]
    }

    var body: some View {
        VStack(spacing: Design.spacious) {
            VStack(spacing: Design.gap) {
                mark
                Text("What are we working on?")
                    .font(Design.display)
                    .multilineTextAlignment(.center)
                    .accessibilityAddTraits(.isHeader)
                if let project = app.status?.workspaceName, let host = app.endpoint?.host {
                    Label("\(project) on \(host)", systemImage: "desktopcomputer")
                        .font(.subheadline).foregroundStyle(.secondary)
                }
            }
            VStack(spacing: Design.gap) {
                ForEach(suggestions.indices, id: \.self) { index in
                    let (title, detail, prompt, icon) = suggestions[index]
                    Button { choose(prompt) } label: {
                        HStack(alignment: .top, spacing: Design.padding) {
                            Image(systemName: icon)
                                .font(Design.headline).foregroundStyle(Design.accent)
                                .frame(width: Design.spacious)
                                .accessibilityHidden(true)
                            VStack(alignment: .leading, spacing: Design.small) {
                                Text(title).font(Design.headline)
                                Text(detail).font(Design.footnote).foregroundStyle(.secondary)
                            }
                            Spacer(minLength: 0)
                        }
                        .multilineTextAlignment(.leading)
                        .padding(Design.padding)
                        .frame(maxWidth: .infinity, minHeight: 72, alignment: .leading)
                        .background(Design.surface, in: .rect(cornerRadius: Design.corner))
                        .contentShape(.rect(cornerRadius: Design.corner))
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .frame(maxWidth: 460)
        .frame(maxWidth: .infinity)
        .padding(.vertical, Design.spacious)
    }

    private var mark: some View {
        Image(.appGlyph)
            .resizable().scaledToFit()
            .frame(width: 56, height: 56)
            .overlay {
                if !reduceMotion {
                    TimelineView(.animation(minimumInterval: 1.0 / 24)) { frame in
                        let phase = min(1, frame.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 6) / 1.6)
                        LinearGradient(colors: [.clear, .white.opacity(0.30), .clear], startPoint: .leading, endPoint: .trailing)
                            .frame(width: 26, height: 86)
                            .rotationEffect(.degrees(25))
                            .offset(x: (phase - 0.5) * 120)
                    }
                }
            }
            .clipShape(.rect(cornerRadius: Design.corner))
            .padding(Design.padding)
            .glassEffect(.regular.tint(Design.accent.opacity(0.14)), in: .circle)
            .accessibilityHidden(true)
    }
}
