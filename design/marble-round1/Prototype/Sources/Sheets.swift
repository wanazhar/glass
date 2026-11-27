import SwiftUI

struct Sheets: View {
    let kind: Screen
    @Environment(\.colorScheme) private var scheme
    var p: Palette { Palette(dark: scheme == .dark) }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(eyebrow).font(.system(.caption2, design: .monospaced).weight(.semibold)).tracking(1.2)
                        .foregroundStyle(p.fig)
                    Text(heading).font(.largeTitle.weight(.semibold)).tracking(-0.8)
                }
                switch kind {
                case .identity: identity
                case .storyboard: storyboard
                case .system: system
                case .components: components
                default: controls
                }
            }
            .frame(maxWidth: 600).padding(.horizontal, 20).padding(.vertical, 14)
            .frame(maxWidth: .infinity)
        }
        .background(p.paper.ignoresSafeArea())
    }

    var eyebrow: String {
        switch kind {
        case .identity: "MARBLE / IDENTITY"
        case .storyboard: "MARBLE / SPLASH"
        case .system: "MARBLE / TOKENS"
        case .components: "MARBLE / COMPONENTS 1 OF 2"
        default: "MARBLE / COMPONENTS 2 OF 2"
        }
    }

    var heading: String {
        switch kind {
        case .identity: "Three marks"
        case .storyboard: "Coming together"
        case .system: "Paper & pigments"
        case .components: "From faces to words"
        default: "Decisions & controls"
        }
    }

    var identity: some View {
        VStack(alignment: .leading, spacing: 9) {
            ForEach(PebbleMark.Concept.allCases, id: \.self) { concept in
                VStack(alignment: .leading, spacing: 8) {
                    HStack {
                        Text(concept.rawValue).font(.headline)
                        Spacer()
                        if concept == .pocket { Text("Recommended").font(.caption.weight(.semibold)).foregroundStyle(p.fig) }
                    }
                    HStack(alignment: .top, spacing: 10) {
                        identitySample("Light") {
                            iconTile(concept, background: Color(hex: 0xF7F2EB), colors: [Color(hex: 0x653E52), Color(hex: 0xA05D43)])
                        }
                        identitySample("Dark") {
                            iconTile(concept, background: Color(hex: 0x2D2523), colors: [Color(hex: 0xE3B7CA), Color(hex: 0xDEA28F)])
                        }
                        identitySample("Tinted") {
                            iconTile(concept, background: Color(hex: 0x493743), colors: [Color(hex: 0xF6E2EC), Color(hex: 0xD3A4BC)])
                        }
                        identitySample("16 pt") {
                            PebbleMark(concept: concept, colors: [p.fig, p.markClay]).frame(width: 16, height: 16)
                                .frame(height: 48)
                        }
                        identitySample("Splash") {
                            VStack(spacing: 2) {
                                PebbleMark(concept: concept, colors: [p.fig, p.markClay]).frame(width: 28, height: 28)
                                Text("Marble").font(.caption.weight(.semibold)).tracking(-0.3)
                            }.frame(height: 48)
                        }
                    }
                }.surface(p, padding: 12, radius: 14)
            }
            Text("Two shapes with room between them, easy to recognise even without colour.")
                .font(.footnote).foregroundStyle(p.muted)
        }
    }

    func identitySample<V: View>(_ label: String, @ViewBuilder sample: () -> V) -> some View {
        VStack(spacing: 4) {
            sample()
            Text(label).font(.caption2).foregroundStyle(p.muted)
        }.frame(maxWidth: .infinity)
    }

    func iconTile(_ concept: PebbleMark.Concept, background: Color, colors: [Color]) -> some View {
        PebbleMark(concept: concept, colors: colors).frame(width: 31, height: 31)
            .frame(width: 48, height: 48)
            .background(background, in: RoundedRectangle(cornerRadius: 11))
            .overlay { RoundedRectangle(cornerRadius: 11).strokeBorder(p.line, lineWidth: 1) }
    }

    var storyboard: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("A small greeting, not a waiting screen.").foregroundStyle(p.muted)
            LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 10), count: 2), spacing: 10) {
                ForEach(Array(zip(["0.00 s", "0.24 s", "0.62 s", "1.55 s"], ["Arrive", "Draw near", "Settle", "Open home"]).enumerated()), id: \.offset) { index, frame in
                    VStack(alignment: .leading, spacing: 8) {
                        ZStack {
                            RoundedRectangle(cornerRadius: 13).fill(p.paper)
                            if index == 3 {
                                VStack(alignment: .leading, spacing: 5) {
                                    Text("Who’s on your mind?").font(.caption.weight(.semibold))
                                    ForEach(Person.all) { person in
                                        HStack { face(person, 16); Text(person.name).font(.caption2); Spacer() }
                                    }
                                }.padding(9)
                            } else {
                                VStack(spacing: 5) {
                                    SplashMark(progress: [0.0, 0.72, 1.0][index]).frame(width: 56, height: 56)
                                    Text("Marble").font(.subheadline.weight(.semibold))
                                }
                            }
                        }.frame(height: 108)
                        Text(frame.0).font(.caption.monospacedDigit()).foregroundStyle(p.fig)
                        Text(frame.1).font(.subheadline.weight(.semibold))
                    }.surface(p, padding: 10, radius: 14)
                }
            }
            Text("Skip is available immediately. Reduce Motion holds the settled mark and opens home without movement.")
                .font(.footnote).foregroundStyle(p.muted)
        }
    }

    var system: some View {
        VStack(alignment: .leading, spacing: 10) {
            swatches("Neutral + accent", [
                ("Paper", p.paper), ("Card", p.card), ("Wash", p.wash), ("Ink", p.ink), ("Fig", p.fig)
            ])
            swatches("Personality hues", Hue.all.map { ($0.name, p.tone($0.light, $0.dark)) })
            swatches("Status", [("Ready", p.good), ("Waiting", p.wait), ("Problem", p.bad)])
            VStack(alignment: .leading, spacing: 5) {
                Text("SF Pro").font(.largeTitle.weight(.semibold)).tracking(-0.7)
                Text("Title 34/semibold · Heading 20/semibold").font(.subheadline.weight(.semibold))
                Text("Body 17 · Supporting 15 · Caption 12").font(.footnote).foregroundStyle(p.muted)
            }.surface(p, padding: 12)
            VStack(alignment: .leading, spacing: 5) {
                Text("Space & shape").font(.headline)
                Text("4 · 8 · 12 · 16 · 24 · 32 pt   /   10 · 14 · 18 · 20 pt radii")
                Text("Solid paper for reading; glass for things you touch.")
                Text("Short, quiet motion. Tap feedback only after a deliberate action.")
            }.font(.footnote).surface(p, padding: 12)
        }
    }

    func swatches(_ name: String, _ colors: [(String, Color)]) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(name).font(.headline)
            HStack(spacing: 4) {
                ForEach(colors, id: \.0) { name, color in
                    VStack(spacing: 4) {
                        RoundedRectangle(cornerRadius: 7).fill(color).frame(height: 25)
                            .overlay { RoundedRectangle(cornerRadius: 7).strokeBorder(p.line, lineWidth: 1) }
                        Text(name).font(.caption2).foregroundStyle(p.muted)
                    }.frame(maxWidth: .infinity)
                }
            }
        }.surface(p, padding: 9, radius: 14)
    }

    var components: some View {
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 5) {
                Text("Avatar scale").font(.headline)
                HStack(alignment: .bottom, spacing: 5) {
                    ForEach([16, 24, 30, 32, 36, 50, 58, 88], id: \.self) { value in
                        if value != 88 {
                            VStack(spacing: 4) {
                                face(.ada, CGFloat(value), working: value == 32)
                                Text("\(value)").font(.caption2.monospacedDigit()).foregroundStyle(p.muted)
                            }
                            .frame(maxWidth: .infinity)
                        }
                    }
                }
                HStack(spacing: 14) {
                    face(.ada, 88)
                    VStack(alignment: .leading, spacing: 4) {
                        Text("88 · Profile").font(.subheadline.weight(.semibold))
                        Text("32 · Working").font(.footnote).foregroundStyle(p.wait)
                        Text("Keep the name beside the shape.").font(.caption).foregroundStyle(p.muted)
                    }
                }
            }.surface(p, padding: 10, radius: 14)
            UserBubble("**@Ada** Let’s leave the afternoon open.")
            Reply(person: .ada, text: "Absolutely. Breakfast, a short walk, then nothing on the calendar.")
            Folded("Ada asked Ben for a quiet route.", icon: "arrow.turn.down.right", disclosure: false)
            LineItem(title: "A quieter way to the river", detail: "2 replies · Today", icon: "bubble.left")
                .surface(p, padding: 8, radius: 14)
            Folded("Checked your Saturday note", icon: "checkmark")
            ErrorCard()
        }
    }

    var controls: some View {
        VStack(alignment: .leading, spacing: 10) {
            RequestCard(kind: .approval)
            RequestCard(kind: .memory)
            Composer(hint: "Message Everyone", demoMention: true)
            GlassEffectContainer(spacing: 8) {
                HStack(spacing: 8) {
                    Action(text: "Stop", icon: "stop.fill")
                    Action(text: "Steer", icon: "arrow.turn.up.right")
                }
            }
        }
    }
}

struct SplashMark: View {
    var progress: CGFloat
    @Environment(\.colorScheme) private var scheme
    var body: some View {
        let p = Palette(dark: scheme == .dark)
        ZStack {
            PebbleMark(concept: .pocket, colors: [p.fig, .clear])
                .offset(x: -14 * (1 - progress), y: -6 * (1 - progress))
            PebbleMark(concept: .pocket, colors: [.clear, p.markClay])
                .offset(x: 14 * (1 - progress), y: 6 * (1 - progress))
        }
    }
}

struct SplashView: View {
    var finish: () -> Void
    @Environment(\.colorScheme) private var scheme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var progress: CGFloat = 0
    var body: some View {
        let p = Palette(dark: scheme == .dark)
        ZStack {
            p.paper.ignoresSafeArea()
            VStack(spacing: 14) {
                SplashMark(progress: reduceMotion ? 1 : progress).frame(width: 108, height: 108)
                Text("Marble").font(.largeTitle.weight(.semibold)).tracking(-0.7)
            }.accessibilityAddTraits(.isHeader)
        }
        .overlay(alignment: .bottomTrailing) {
            Action(text: "Skip", action: finish).padding(16)
        }
        .task {
            if !reduceMotion {
                withAnimation(.easeOut(duration: 0.62)) { progress = 1 }
            }
            do { try await Task.sleep(for: .seconds(1.55)) } catch { return }
            finish()
        }
    }
}
