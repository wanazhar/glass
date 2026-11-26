import SwiftUI

enum ComposerMode {
    case idle, typing, working, attachments
}

struct Composer: View {
    var mode: ComposerMode
    var draftText = ""
    var tapModel: () -> Void = {}
    @State private var draft = ""
    @Environment(\.palette) private var p

    var body: some View {
        GlassEffectContainer(spacing: 12) {
            VStack(spacing: 8) {
                if mode == .attachments {
                    HStack {
                        FileChip(name: "trace.json", detail: "24 KB")
                        Spacer()
                        circle("mic", label: "Dictate")
                    }
                        .padding(.horizontal, 8)
                }
                HStack(spacing: 6) {
                    Button(action: tapModel) {
                        HStack(spacing: 5) {
                            Text("Opus 4.7").font(.subheadline.weight(.semibold))
                            Image(systemName: "chevron.down").font(.system(size: 9, weight: .semibold))
                        }
                        .foregroundStyle(p.muted)
                        .frame(minHeight: 36)
                    }
                    .buttonStyle(.plain)
                    Spacer()
                    if mode == .working {
                        ProgressView().controlSize(.mini).tint(p.accent)
                        Text("Writing · 2m 14s").font(.caption.weight(.medium)).foregroundStyle(p.muted)
                    }
                }
                .padding(.horizontal, 12)
                HStack(spacing: 8) {
                    circle("plus", label: "Add attachment")
                    let placeholder = mode == .working ? "Steer this reply" : "Message Glass"
                    TextField(placeholder, text: $draft, prompt: Text(placeholder).foregroundStyle(p.muted), axis: .vertical)
                        .font(.body)
                        .lineLimit(1...4)
                        .accessibilityLabel("Message")
                    circle(mode == .working ? "stop.fill" : "arrow.up",
                           label: mode == .working ? "Stop" : "Send", primary: true)
                }
            }
            .padding(8)
            .glassEffect(.regular.tint(p.panel.opacity(0.72)), in: .rect(cornerRadius: 28))
        }
        .onAppear {
            draft = mode == .typing && draftText.isEmpty ? "Compare the cold run." : draftText
        }
    }

    private func circle(_ icon: String, label: String, primary: Bool = false) -> some View {
        Button {} label: {
            Image(systemName: icon)
                .font(.system(size: 17, weight: .semibold))
                .foregroundStyle(primary ? p.onAccent : p.ink)
                .frame(width: 44, height: 44, alignment: .center)
                .background(primary ? p.accent : p.inset, in: .circle)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }
}

struct ChatToolbar: View {
    var title = "Glass"
    var leadingAction = true
    var trailingAction = true
    var menu: () -> Void = {}
    var newChat: () -> Void = {}
    var info: () -> Void = {}
    @Environment(\.palette) private var p

    var body: some View {
        GlassEffectContainer(spacing: 12) {
            HStack(spacing: 6) {
                if leadingAction { control("line.3.horizontal", label: "Threads", action: menu) }
                Button(title, action: info)
                    .font(.headline)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, minHeight: 44)
                    .buttonStyle(.plain)
                    .accessibilityHint("Opens thread info")
                if trailingAction { control("square.and.pencil", label: "New chat", action: newChat) }
            }
            .padding(3)
            .foregroundStyle(p.ink)
            .glassEffect(.regular, in: .capsule)
        }
    }

    private func control(_ symbol: String, label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 16, weight: .semibold))
                .frame(width: 44, height: 44)
                .contentShape(.circle)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }
}

struct TranscriptSurface<Content: View>: View {
    let title: String
    var mode: ComposerMode
    var nativeNavigation = false
    var anchor: UnitPoint = .top
    var connectionBanner: Bool?
    var connectionAction: () -> Void = {}
    var menu: () -> Void = {}
    var newChat: () -> Void = {}
    @ViewBuilder var content: () -> Content
    @State private var position = ScrollPosition()
    @State private var awayFromLatest = false
    @State private var showingInfo = false
    @State private var choosingModel = false
    @Environment(\.palette) private var p
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20, content: content)
                .frame(maxWidth: 560)
                .padding(.horizontal, 23).padding(.vertical, 16)
                .frame(maxWidth: .infinity)
        }
        .scrollPosition($position)
        .defaultScrollAnchor(anchor, for: .initialOffset)
        .defaultScrollAnchor(nativeNavigation ? .bottom : .top, for: .alignment)
        .onScrollGeometryChange(for: Bool.self) { geometry in
            let unobscuredBottom = geometry.contentOffset.y + geometry.contentInsets.top
                + geometry.containerSize.height
            return geometry.contentSize.height - unobscuredBottom > 32
        } action: { _, away in
            awayFromLatest = away
        }
        .safeAreaBar(edge: .top, spacing: 6) {
            if !nativeNavigation {
                ChatToolbar(title: title, menu: menu, newChat: newChat, info: { showingInfo = true })
                    .frame(maxWidth: 640)
                    .padding(.horizontal, 23)
            }
        }
        .safeAreaBar(edge: .bottom, spacing: 8) {
            VStack(spacing: 8) {
                JumpControl(visible: awayFromLatest) {
                    withAnimation(reduceMotion ? nil : .snappy(duration: 0.24)) {
                        position.scrollTo(edge: .bottom)
                    }
                }
                if let connectionBanner {
                    ConnectionBanner(offline: connectionBanner, takeover: connectionAction)
                } else {
                    Composer(mode: mode) { choosingModel = true }
                }
            }
            .frame(maxWidth: 560)
            .padding(.horizontal, 23)
        }
        .scrollEdgeEffectStyle(.hard, for: .top)
        .scrollEdgeEffectStyle(.soft, for: .bottom)
        .toolbar {
            if nativeNavigation {
                ToolbarItem(placement: .principal) {
                    Button(title) { showingInfo = true }
                        .font(.headline)
                        .foregroundStyle(p.ink)
                        .accessibilityHint("Opens thread info")
                }
            }
        }
        .sheet(isPresented: $showingInfo) {
            ThreadInfoPanel(title: title == "Glass" ? "Fix the nil crash in TitleView" : title)
                .padding(20)
                .frame(maxHeight: .infinity, alignment: .top)
                .presentationBackground(p.paper)
                .presentationDragIndicator(.visible)
                .presentationDetents([.medium, .large])
        }
        .sheet(isPresented: $choosingModel) {
            ModelPanel()
                .padding(20)
                .frame(maxHeight: .infinity, alignment: .top)
                .presentationBackground(p.paper)
                .presentationDragIndicator(.visible)
                .presentationDetents([.medium, .large])
        }
    }
}

struct FileChip: View {
    let name: String
    let detail: String
    @Environment(\.palette) private var p

    var body: some View {
        Label {
            let filename = Text(name).fontWeight(.semibold)
            let metadata = Text(detail).foregroundStyle(p.muted)
            Text("\(filename)   \(metadata)")
        } icon: {
            Image(systemName: "doc").foregroundStyle(p.accent)
        }
        .font(.caption)
        .padding(.horizontal, 11)
        .padding(.vertical, 8)
        .background(p.inset, in: .capsule)
        .fixedSize(horizontal: false, vertical: true)
    }
}

struct UserBubble: View {
    var message = "What changed in gateway latency after the cache patch?"
    @Environment(\.palette) private var p

    var body: some View {
        HStack {
            Spacer(minLength: 34)
            Text(message)
                .font(.body)
                .padding(.horizontal, 16)
                .padding(.vertical, 11)
                .background(p.inset, in: .rect(cornerRadius: 19))
        }
    }
}

struct AssistantLabel: View {
    @Environment(\.palette) private var p

    var body: some View {
        HStack(spacing: 8) {
            LensMark(compact: true).frame(width: 16, height: 16)
            Text("Glass").font(.subheadline.weight(.semibold)).foregroundStyle(p.muted)
        }
    }
}

struct CodeCard: View {
    var titleSample = false
    @Environment(\.palette) private var p

    private var colors: [Color] {
        let hex: [UInt32] = p.dark
            ? [0x60E4E8, 0xF1F6F8, 0xF1C581, 0x9DBDF8, 0xA9BAC4]
            : [0x6C33A1, 0x131C22, 0x9B392E, 0x125897, 0x4D5E69]
        return hex.map { Color(hex: $0) }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(titleSample ? "Swift" : "CachePolicy.swift")
                    .font(.caption.weight(.semibold))
                Spacer()
                Image(systemName: "document.on.document").accessibilityLabel("Copy")
            }
            .foregroundStyle(colors[4])
            .padding(.horizontal, 16).padding(.vertical, 11)
            Divider().overlay(p.line.opacity(0.65))
            VStack(alignment: .leading, spacing: 8) {
                let keyword = Text("let").foregroundStyle(colors[0])
                if titleSample {
                    let literal = Text("\"Untitled\"").foregroundStyle(colors[2])
                    let operation = Text("??").foregroundStyle(colors[3])
                    Text("\(keyword) title = item.title \(operation) \(literal)").foregroundStyle(colors[1])
                } else {
                    Text("// Keep disk reads off the hot path").foregroundStyle(colors[4])
                    let literal = Text("120").foregroundStyle(colors[2])
                    let symbol = Text(".store").foregroundStyle(colors[3])
                    Text("\(keyword) ttl = \(literal)").foregroundStyle(colors[1])
                    Text("cache\(symbol)(result, ttl: ttl)").foregroundStyle(colors[1])
                }
            }
            .font((titleSample ? Font.subheadline : Font.callout).monospaced())
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 16).padding(.vertical, 14)
        }
        .background(p.terminal, in: .rect(cornerRadius: 16))
        .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(p.line))
        .clipShape(.rect(cornerRadius: 16))
    }
}

struct BenchmarkTable: View {
    var titleSample = false
    @Environment(\.palette) private var p

    var body: some View {
        if titleSample {
            VStack(spacing: 0) {
                caseRow("Case", "Result", header: true)
                Divider()
                caseRow("Nil", "Untitled")
                Divider()
                caseRow("Set", "Original")
            }
            .clipShape(.rect(cornerRadius: 13))
            .panel(13)
        } else {
            benchmarkGrid
        }
    }

    private func caseRow(_ left: String, _ right: String, header: Bool = false) -> some View {
        HStack(spacing: 14) {
            Text(left).frame(maxWidth: .infinity, alignment: .leading)
            Text(right).frame(maxWidth: .infinity, alignment: .leading)
        }
        .font(header ? .subheadline.weight(.semibold) : .subheadline)
        .padding(.horizontal, 14).padding(.vertical, 10)
        .background(header ? p.inset.opacity(0.65) : p.panel)
    }

    private var benchmarkGrid: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Request latency").font(.subheadline.weight(.semibold))
                Spacer()
                Image(systemName: "arrow.left.and.right").font(.caption).foregroundStyle(p.muted)
                    .accessibilityLabel("Scroll horizontally for all columns")
            }
            .padding(14)
            ScrollView(.horizontal) {
                Grid(alignment: .leading, horizontalSpacing: 22, verticalSpacing: 0) {
                    cells("Scenario", "Before", "After", "Change", header: true)
                    cells("Cold cache", "164 ms", "121 ms", "−26%")
                    cells("Warm cache", "126 ms", "84 ms", "−33%")
                }
                .fixedSize(horizontal: true, vertical: false)
                .padding(.horizontal, 14)
            }
            .defaultScrollAnchor(.leading)
        }
        .padding(.bottom, 6)
        .panel()
    }

    private func cells(_ label: String, _ before: String, _ after: String, _ change: String, header: Bool = false) -> some View {
        GridRow {
            Text(label)
            Text(before).monospacedDigit()
            Text(after).monospacedDigit()
            Text(change).monospacedDigit().foregroundStyle(header ? p.muted : p.success)
        }
        .font(header ? .caption.weight(.semibold) : .subheadline)
        .foregroundStyle(header ? p.muted : p.ink)
        .padding(.vertical, 8)
    }
}

struct BenchmarkChart: View {
    @Environment(\.palette) private var p

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Warm cache · p95").font(.subheadline.weight(.semibold))
            bar("Before", "126 ms", portion: 1, color: p.muted)
            bar("After", "84 ms", portion: 0.667, color: p.accent)
        }
        .padding(14)
        .panel()
    }

    private func bar(_ label: String, _ value: String, portion: CGFloat, color: Color) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                Text(label)
                Spacer()
                Text(value).monospacedDigit()
            }
            .font(.caption)
            GeometryReader { proxy in
                Capsule().fill(color).frame(width: proxy.size.width * portion)
            }
            .frame(height: 5)
        }
    }
}

struct MathSample: View {
    @ScaledMetric(relativeTo: .title3) private var mathSize = 24.0
    @Environment(\.palette) private var p

    var body: some View {
        let median = Text("m").font(.system(.body, design: .serif).italic())
        VStack(alignment: .leading, spacing: 10) {
            Text("The median \(median) stays within tolerance.")
                .font(.body)
            HStack(spacing: 8) {
                Text("Δ =")
                VStack(spacing: mathSize * 0.24) {
                    HStack(alignment: .firstTextBaseline, spacing: mathSize * 0.26) {
                        term("1")
                        Text("−")
                        term("0")
                    }
                    Rectangle().frame(height: 1)
                    term("0")
                }
                .fixedSize()
                Text("× 100")
            }
            .font(.system(size: mathSize, design: .serif))
            .accessibilityElement(children: .ignore)
            .accessibilityLabel("Delta equals t one minus t zero, divided by t zero, times one hundred.")
            .frame(maxWidth: .infinity)
            .padding(14).panel()
        }
    }

    private func term(_ index: String) -> Text {
        let variable = Text("t").italic()
        let subscriptDigit = Text(index)
            .font(.system(size: mathSize * 0.67, design: .serif))
            .baselineOffset(-mathSize * 0.20)
        return Text("\(variable)\(subscriptDigit)")
    }
}

struct JumpControl: View {
    let visible: Bool
    var action: () -> Void = {}
    @Environment(\.palette) private var p

    var body: some View {
        if visible {
            Button(action: action) {
                Label("Jump to latest", systemImage: "arrow.down")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(p.ink)
                    .padding(.horizontal, 15)
                    .frame(minHeight: 44)
                    .background(p.panel, in: .capsule)
                    .overlay(Capsule().strokeBorder(p.line, lineWidth: 1))
            }
            .buttonStyle(.plain)
            .accessibilityHint("Returns to the newest message")
        }
    }
}
