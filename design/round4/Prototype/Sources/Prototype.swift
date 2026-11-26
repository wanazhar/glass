import SwiftUI

@main
struct GlassRound4App: App {
    private var plate: String {
        let args = ProcessInfo.processInfo.arguments
        guard let index = args.firstIndex(of: "--plate"), args.indices.contains(index + 1) else { return "index" }
        return args[index + 1]
    }

    var body: some Scene {
        WindowGroup {
            PrototypeRoot(initialPlate: plate)
        }
    }
}

struct PrototypeRoot: View {
    let initialPlate: String
    @State private var selection: String?
    @Environment(\.palette) private var p

    private var plate: String { selection ?? initialPlate }
    private let routes = [
        ("system", "Visual system"), ("marks", "Logo concepts"), ("storyboard", "Splash storyboard"),
        ("splash", "Play splash"), ("splash-v1", "Alternate splash"),
        ("text", "Transcript and markdown"), ("rich", "Structured output"),
        ("rich-xl", "Structured output / XL"), ("work", "Work log"), ("failure", "Failure and command output"),
        ("diff", "Diff details"), ("composer", "Composer states"), ("panels", "Model and thread info"),
        ("connection-offline", "Disconnected"), ("connection-readonly", "Read-only"),
        ("edge", "Scroll-edge behavior"),
    ]

    var body: some View {
        Group {
            if plate == "splash" {
                SplashScene { selection = "splash-end" }
            } else if plate == "splash-v1" {
                SplashScene(frozen: 0.24, showsControls: false, striped: true)
                    .ignoresSafeArea()
            } else if plate == "splash-end" {
                VStack(alignment: .leading, spacing: 20) {
                    Eyebrow("GLASS / LENS")
                    Spacer()
                    LensMark(optical: true).frame(width: 82, height: 82)
                    Text("Glass").font(.system(.largeTitle, design: .rounded, weight: .semibold))
                    Spacer()
                    ActionPill(title: "Replay", icon: "arrow.clockwise") { selection = "splash" }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(24)
            } else if plate.hasPrefix("screen-"), let screen = Screen(rawValue: String(plate.dropFirst(7))) {
                WorkspacePreview(initialScreen: screen)
            } else if plate == "connection-offline" || plate == "connection-readonly" {
                ConnectionPreview(offline: plate == "connection-offline")
            } else if plate == "edge" {
                TranscriptSurface(title: "Glass", mode: .working, anchor: .bottom) {
                    UserBubble()
                    AssistantLabel()
                    Text("The cache patch helped both runs. Warm requests show the clearest improvement.")
                        .font(.body).lineSpacing(5)
                    BenchmarkTable()
                    BenchmarkChart()
                    CodeCard()
                    WorkLog(running: true)
                }
            } else {
                NavigationStack {
                    ScrollView {
                        Group {
                            switch plate {
                            case "system": SystemPlate()
                            case "marks": MarkPlate()
                            case "storyboard": StoryboardPlate()
                            case "index": index
                            default: ComponentPlate(plate: plate)
                            }
                        }
                        .frame(maxWidth: 570)
                        .padding(.horizontal, 23)
                        .padding(.vertical, 16)
                        .frame(maxWidth: .infinity)
                    }
                    .background(p.paper.ignoresSafeArea())
                    .scrollEdgeEffectStyle(.soft, for: [.top, .bottom])
                    .toolbar {
                        if plate != "index", initialPlate == "index" {
                            ToolbarItem(placement: .topBarLeading) {
                                Button("Index", systemImage: "chevron.left") { selection = "index" }
                            }
                        }
                    }
                    .toolbarVisibility(initialPlate == "index" && plate != "index" ? .visible : .hidden, for: .navigationBar)
                }
            }
        }
        .foregroundStyle(p.ink)
        .tint(p.accent)
        .background(p.paper.ignoresSafeArea())
        .preferredColorScheme(nil)
        .id(plate)
    }

    private var index: some View {
        VStack(alignment: .leading, spacing: 16) {
            PlateTitle(number: "GLASS / ROUND 04", title: "Design studio", summary: "Prototype plates")
            ForEach(Screen.allCases.map { ("screen-" + $0.rawValue, $0.title) } + routes, id: \.0) { key, title in
                Button {
                    selection = key
                } label: {
                    HStack {
                        Text(title).font(.body.weight(.medium))
                        Spacer()
                        Image(systemName: "arrow.up.right").font(.caption)
                    }
                    .padding(12)
                    .panel(10)
                }
                .buttonStyle(.plain)
            }
        }
    }
}

struct PlateTitle: View {
    let number: String
    let title: String
    let summary: String
    @Environment(\.palette) private var p

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Eyebrow(number)
            Text(title).font(.system(.largeTitle, design: .rounded, weight: .semibold))
                .tracking(-0.9)
            Text(summary).font(.subheadline).foregroundStyle(p.muted)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.bottom, 4)
    }
}

struct SystemPlate: View {
    @Environment(\.palette) private var p

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            PlateTitle(number: "DIRECTION / 01", title: "Lens system", summary: "Optics at the edges. A neutral reading surface.")
            VStack(alignment: .leading, spacing: 8) {
                Eyebrow("01 / COLOR")
                HStack(spacing: 5) {
                    swatch("Canvas", p.paper, p.dark ? "0B1013" : "F5F7FA")
                    swatch("Panel", p.panel, p.dark ? "161E23" : "FFFFFF")
                    swatch("Ink", p.ink, p.dark ? "F1F6F8" : "131C22")
                    swatch("Teal", p.accent, p.dark ? "60E4E8" : "08737D")
                }
                HStack(spacing: 5) {
                    swatch("Success", p.success, p.dark ? "9CDEA9" : "28613D")
                    swatch("Warning", p.warning, p.dark ? "F7CB84" : "785011")
                    swatch("Failure", p.failure, p.dark ? "FFAA9D" : "A23330")
                }
            }
            VStack(alignment: .leading, spacing: 7) {
                Eyebrow("02 / TYPE · BASE SIZE / LINE HEIGHT")
                Text("Lens").font(.system(.largeTitle, design: .rounded, weight: .semibold))
                Text("34/41 SF Rounded semibold · display only").font(.caption).foregroundStyle(p.muted)
                Text("A measured conversation").font(.title3.weight(.semibold))
                Text("SF: 28/34 · 22/28 · 20/25 headings, semibold\n17/25 body regular · 15/20 mono · 12/16 labels")
                    .font(.caption).foregroundStyle(p.muted).lineSpacing(3)
            }
            VStack(alignment: .leading, spacing: 6) {
                Eyebrow("03 / RHYTHM")
                Text("4 · 8 · 12 · 16 · 24 · 32 pt spacing\n8 · 12 · 16 · 24 · 28 pt corners · 44 pt targets")
                    .font(.footnote).lineSpacing(3)
            }
            VStack(alignment: .leading, spacing: 6) {
                Eyebrow("04 / OPTICS")
                Text("Glass: toolbar, composer, drawer shell and pinned actions.\nSolid: text, output, status and sheet bodies.")
                    .font(.footnote).lineSpacing(3)
            }
            VStack(alignment: .leading, spacing: 6) {
                Eyebrow("05 / MOTION & TOUCH")
                Text("240 ms disclosure · 1.55 s splash / tap to refract.\nSoft haptic on ripple; light confirmation on send.\nReduce Motion: no ripple; immediate transition.")
                    .font(.footnote).lineSpacing(3)
            }
        }
    }

    private func swatch(_ name: String, _ color: Color, _ hex: String) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            RoundedRectangle(cornerRadius: 6).fill(color).frame(height: 23)
                .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(p.line))
            Text(name).font(.caption2.weight(.semibold))
            Text(hex).font(.system(.caption2, design: .monospaced)).foregroundStyle(p.muted)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct MarkPlate: View {
    @Environment(\.palette) private var p

    var body: some View {
        VStack(alignment: .leading, spacing: 17) {
            PlateTitle(number: "IDENTITY / 01", title: "Lens family", summary: "An orb on a bent horizon; one teal rim.")
            ForEach(MarkKind.allCases, id: \.rawValue) { kind in
                VStack(alignment: .leading, spacing: 9) {
                    HStack {
                        Text(kind.name).font(.headline)
                        Spacer()
                        Eyebrow(kind == .lens ? "PRIMARY" : "ALTERNATE")
                    }
                    HStack(alignment: .top, spacing: 12) {
                        icon(kind, title: "Light", dark: false, field: 0xE5EDF4)
                        icon(kind, title: "Dark", dark: true, field: 0x142126)
                        icon(kind, title: "Tinted", dark: true, field: 0x1F3754, tint: Color(hex: 0x99C6FF))
                        Spacer(minLength: 0)
                        VStack(spacing: 5) {
                            LensMark(kind: kind, compact: true).frame(width: 16, height: 16)
                                .frame(width: 37, height: 67)
                            Text("16 pt").font(.caption2).foregroundStyle(p.muted)
                        }
                    }
                    SplashScene(kind: kind, frozen: 0.24, showsControls: false, striped: kind == .drop)
                        .frame(height: 86)
                        .clipShape(.rect(cornerRadius: 9))
                    Text(copy(kind)).font(.caption).foregroundStyle(p.muted)
                }
                .padding(13)
                .panel()
            }
        }
    }

    private func icon(_ kind: MarkKind, title: String, dark: Bool, field: UInt32, tint: Color? = nil) -> some View {
        VStack(spacing: 5) {
            Horizon(fullWidth: true)
                .stroke(Color.white.opacity(dark ? 0.44 : 0.90), lineWidth: 0.7)
                .overlay {
                    LensMark(kind: kind, fixedPalette: Palette(dark: dark), tint: tint)
                        .frame(width: 47, height: 47)
                }
                .frame(width: 67, height: 67)
                .background(Color(hex: field), in: .rect(cornerRadius: 16))
                .clipShape(.rect(cornerRadius: 16))
                .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(p.line.opacity(0.65)))
            Text(title).font(.caption2).foregroundStyle(p.muted)
        }
    }

    private func copy(_ kind: MarkKind) -> String {
        switch kind {
        case .lens: "Circular lens over one horizon. The simplest silhouette to keep across all sizes."
        case .drop: "An off-center lens on a striped field. Distinct at full size, busier at 16 pt."
        }
    }
}

struct StoryboardPlate: View {
    private let frames: [(Double, String)] = [
        (0, "Rest"), (0.18, "Contact"), (0.38, "Refraction"), (0.65, "Travel"), (1.55, "Settle"),
    ]
    @Environment(\.palette) private var p

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            PlateTitle(number: "MOTION / V4", title: "Horizon ripple", summary: "One expanding wave bends the lines behind the lens.")
            LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 12) {
                ForEach(frames, id: \.0) { time, label in
                    VStack(alignment: .leading, spacing: 6) {
                        SplashScene(frozen: time, showsControls: false)
                            .frame(height: 150)
                            .clipShape(.rect(cornerRadius: 12))
                            .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(p.line))
                        Eyebrow(String(format: "%.2fs / %@", time, label))
                    }
                }
            }
            Text("A touch moves the origin to the finger. Skip interrupts immediately.")
                .font(.footnote).foregroundStyle(p.muted)
        }
    }
}

struct ComponentPlate: View {
    let plate: String
    @Environment(\.palette) private var p

    var body: some View {
        VStack(alignment: .leading, spacing: plate == "composer" ? 10 : 16) {
            switch plate {
            case "text":
                PlateTitle(number: "COMPONENTS / 01", title: "Transcript", summary: "The reply is a reading surface, not a card grid.")
                ChatToolbar(title: "Gateway latency baseline")
                UserBubble()
                AssistantLabel()
                Text("The cache patch helped both runs. Warm requests show the clearest improvement.")
                    .font(.body).lineSpacing(5)
                Text("What changed").font(.title.weight(.semibold))
                Text("Warm traffic").font(.title2.weight(.semibold))
                Text("Repeat requests").font(.title3.weight(.semibold))
                VStack(alignment: .leading, spacing: 8) {
                    Text("1.   Cache lookups happen before disk reads.")
                    Text("•    Cold requests still take the miss path.")
                }
                .font(.body)
                Text("Keep the cold path measurable.")
                    .font(.body.italic())
                    .padding(.leading, 12)
                    .overlay(alignment: .leading) { Rectangle().fill(p.accent).frame(width: 2) }
                HStack {
                    Text("cache.store").font(.callout.monospaced()).padding(5).background(p.inset, in: .rect(cornerRadius: 5))
                    Text("Benchmark notes").underline().font(.subheadline.weight(.medium)).foregroundStyle(p.accent)
                        .accessibilityAddTraits(.isLink)
                }
                FileChip(name: "benchmark.csv", detail: "8 KB")
            case "rich", "rich-xl":
                PlateTitle(number: "COMPONENTS / 02", title: "Rich output",
                           summary: plate == "rich-xl" ? "Dynamic Type XL · fixed-size columns scroll." : "Readable code, data and notation.")
                Group {
                    BenchmarkTable()
                    CodeCard()
                    BenchmarkChart()
                    MathSample()
                }
                .environment(\.dynamicTypeSize, plate == "rich-xl" ? .xLarge : .large)
            case "work":
                PlateTitle(number: "COMPONENTS / 03", title: "Work log", summary: "Execution trace, folded until it matters.")
                Eyebrow("LIVE / COLLAPSED")
                WorkLog(running: true)
                Eyebrow("LIVE / EXPANDED")
                WorkLog(running: true, expanded: true)
                Eyebrow("DONE / COLLAPSED")
                WorkLog(running: false)
                Eyebrow("DONE / EXPANDED")
                WorkLog(running: false, expanded: true)
            case "failure":
                PlateTitle(number: "COMPONENTS / 04", title: "Run recovery", summary: "Explain what stopped before asking to retry.")
                FailureCard()
                OutputDetail()
            case "diff":
                PlateTitle(number: "COMPONENTS / 05", title: "Changes", summary: "Two file gutters. Count edits, not patch rows.")
                DiffDetail()
                Text("Patch transport headers are not displayed as edits.")
                    .font(.footnote).foregroundStyle(p.muted)
            case "composer":
                PlateTitle(number: "COMPONENTS / 06", title: "Composer", summary: "One control surface, not a stack of banners.")
                Eyebrow("IDLE")
                Composer(mode: .idle)
                Eyebrow("TYPING")
                Composer(mode: .typing)
                HStack { Eyebrow("WORKING / AWAY FROM LATEST"); Spacer() }
                JumpControl(visible: true)
                Composer(mode: .working)
                Eyebrow("ATTACHMENTS")
                Composer(mode: .attachments)
            case "panels":
                PlateTitle(number: "COMPONENTS / 07", title: "Control panels", summary: "Solid reading surfaces over a live conversation.")
                ModelPanel()
                ThreadInfoPanel()
            default:
                ContentUnavailableView("Unknown plate", systemImage: "rectangle.slash")
            }
        }
    }
}
