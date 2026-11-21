import SwiftUI

/// The Prism protocol: several named marbles, working at once.
///
/// Grok Bot's roster taught the useful lesson here: a list of agents is only
/// scannable if each one is recognisable from its shape and if its state is
/// carried by the same object rather than by another row of badges. So a marble
/// is a shape with two eyes, its hue is its identity, and its motion is its
/// state.
struct PrismView: View {
    @EnvironmentObject private var store: MarbleStore
    @Environment(\.dismiss) private var dismiss
    @State private var prompt = ""
    @State private var name = ""
    @State private var role = ""
    @State private var workspace = ""
    @State private var glyph = "circle"
    @State private var hue = 210.0

    private let glyphs = ["circle", "hex", "shield", "drop", "star", "square"]

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Text(store.prism.summary)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                    // A sheet hides the chat's error strip, so a refused add or
                    // dispatch has to be reported here or not at all.
                    if let notice = store.prismNotice {
                        Text(notice)
                            .font(.footnote)
                            .foregroundStyle(.green)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    if let error = store.errorMessage {
                        Text(error)
                            .font(.footnote)
                            .foregroundStyle(.red)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                }

                Section("Dispatch") {
                    TextField("Hand the same question to every marble…", text: $prompt, axis: .vertical)
                        .lineLimit(1...4)
                    Button {
                        let text = prompt
                        prompt = ""
                        Task { await store.dispatchSwarm(text: text) }
                    } label: {
                        Label("Swarm all", systemImage: "hexagon.fill")
                    }
                    .disabled(prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }

                Section("Roster") {
                    if store.prism.marbles.isEmpty {
                        Text("No marbles yet.")
                            .foregroundStyle(.secondary)
                    }
                    ForEach(store.prism.marbles) { marble in
                        HStack(alignment: .top, spacing: 12) {
                            MarbleAvatar(marble: marble)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(marble.name)
                                Text(marble.role.isEmpty ? "no role yet" : marble.role)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                Text(meta(marble))
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                                if !marble.lastError.isEmpty {
                                    Text(marble.lastError)
                                        .font(.caption)
                                        .foregroundStyle(.red)
                                        .fixedSize(horizontal: false, vertical: true)
                                }
                            }
                            Spacer(minLength: 0)
                            Menu {
                                Button("Ask this marble") {
                                    let text = prompt
                                    Task { await store.dispatchSwarm(text: text, marbles: [marble.id]) }
                                }
                                .disabled(marble.isWorking || prompt.isEmpty)
                                Button("Open its conversation") {
                                    guard !marble.conversationId.isEmpty else { return }
                                    store.selectedConversationId = marble.conversationId
                                    dismiss()
                                }
                                .disabled(marble.conversationId.isEmpty)
                                Button("Remove from roster", role: .destructive) {
                                    Task { await store.removeMarble(marble) }
                                }
                            } label: {
                                Image(systemName: "ellipsis")
                                    .frame(width: 34, height: 34)
                            }
                            .accessibilityLabel("\(marble.name) actions")
                        }
                    }
                }

                Section("Add a marble") {
                    TextField("Name (Scout)", text: $name)
                    TextField("Role (reads the repo)", text: $role)
                    TextField("Workspace subfolder (optional)", text: $workspace)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    glyphPicker
                    HStack(spacing: 12) {
                        Slider(value: $hue, in: 0...359, step: 1)
                        Circle()
                            .fill(Color(hue: hue / 360.0, saturation: 0.62, brightness: 0.92))
                            .frame(width: 22, height: 22)
                    }
                    Button {
                        let draftName = name
                        let draftRole = role
                        let draftGlyph = glyph
                        let draftHue = Int(hue)
                        let draftWorkspace = workspace
                        name = ""
                        role = ""
                        workspace = ""
                        Task {
                            await store.addMarble(
                                name: draftName,
                                role: draftRole,
                                glyph: draftGlyph,
                                hue: draftHue,
                                workspace: draftWorkspace
                            )
                        }
                    } label: {
                        Label("Add marble", systemImage: "plus")
                    }
                    .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
            .navigationTitle("Prism")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") { dismiss() }
                }
            }
            .task { await store.refresh() }
        }
    }

    /// Choosing a shape by its name alone asks a person to imagine it six
    /// times. The picker shows the marble at the hue already chosen, which is
    /// the same object the roster draws.
    private var glyphPicker: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(glyphs, id: \.self) { option in
                    Button {
                        glyph = option
                    } label: {
                        MarbleGlyphMark(glyph: option, hue: Int(hue), size: 44)
                            .padding(6)
                            .overlay(
                                Circle().strokeBorder(
                                    option == glyph ? MarbleTheme.tint : Color.clear,
                                    lineWidth: 2
                                )
                            )
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("\(option) shape")
                    .accessibilityAddTraits(option == glyph ? [.isSelected] : [])
                }
                Spacer(minLength: 0)
            }
            .padding(.vertical, 2)
        }
    }

    private func meta(_ marble: MarblePrismMarble) -> String {
        "\(marble.stateLabel) · \(marble.runs) turn\(marble.runs == 1 ? "" : "s")"
    }

}

/// One marble's identity on its own: a shape, a hue, and two eyes. The roster
/// and the shape picker draw this same object, so choosing a shape shows what
/// the roster will show.
struct MarbleGlyphMark: View {
    var glyph: String
    var hue: Int
    var size: CGFloat = 42

    var body: some View {
        ZStack {
            marbleShape(glyph, size: size)
                .fill(tint)
            HStack(spacing: max(3, size * 0.12)) {
                eye
                eye
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }

    private var eye: some View {
        Capsule()
            .fill(Color.black.opacity(0.75))
            .frame(width: max(3, size * 0.095), height: max(4, size * 0.12))
    }

    private var tint: Color {
        Color(
            hue: Double(((hue % 360) + 360) % 360) / 360.0,
            saturation: 0.62,
            brightness: 0.92
        )
    }
}

/// The outline a marble's glyph names, sized for the space it is drawn in.
func marbleShape(_ glyph: String, size: CGFloat) -> AnyShape {
    switch glyph {
    case "hex":
        return AnyShape(MarbleHexagon())
    case "shield":
        return AnyShape(MarbleShield())
    case "drop":
        return AnyShape(MarbleDrop())
    case "star":
        return AnyShape(MarbleStar())
    case "square":
        return AnyShape(RoundedRectangle(cornerRadius: size * 0.26, style: .continuous))
    default:
        return AnyShape(Circle())
    }
}

/// A marble drawn the way the roster needs it: the mark, plus state carried by
/// motion instead of by another badge.
struct MarbleAvatar: View {
    var marble: MarblePrismMarble
    var size: CGFloat = 42
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var breathing = false

    var body: some View {
        MarbleGlyphMark(glyph: marble.glyph, hue: marble.hue, size: size)
        .overlay {
            if marble.isWorking {
                marbleShape(marble.glyph, size: size)
                    .stroke(tint.opacity(0.75), lineWidth: 2)
                    .scaleEffect(breathing ? 1.16 : 1.0)
                    .opacity(breathing ? 0.25 : 0.9)
            }
        }
        .overlay(alignment: .bottomTrailing) {
            if marble.state == "blocked" {
                Circle().fill(.red).frame(width: 9, height: 9)
            } else if marble.state == "done" {
                Circle().fill(.green).frame(width: 9, height: 9)
            }
        }
        // Motion only while the marble is actually working, and none at all
        // when the person asked the system to reduce it.
        .task(id: marble.state) { breathe() }
        .accessibilityLabel("\(marble.name), \(marble.stateLabel)")
    }

    private var tint: Color {
        Color(
            hue: Double(((marble.hue % 360) + 360) % 360) / 360.0,
            saturation: 0.62,
            brightness: 0.92
        )
    }

    private func breathe() {
        guard !reduceMotion, marble.isWorking else {
            withAnimation(.easeOut(duration: 0.2)) { breathing = false }
            return
        }
        withAnimation(.easeInOut(duration: 1.3).repeatForever(autoreverses: true)) {
            breathing = true
        }
    }
}

struct MarbleHexagon: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        let width = rect.width
        let height = rect.height
        path.move(to: CGPoint(x: width * 0.25, y: 0))
        path.addLine(to: CGPoint(x: width * 0.75, y: 0))
        path.addLine(to: CGPoint(x: width, y: height * 0.5))
        path.addLine(to: CGPoint(x: width * 0.75, y: height))
        path.addLine(to: CGPoint(x: width * 0.25, y: height))
        path.addLine(to: CGPoint(x: 0, y: height * 0.5))
        path.closeSubpath()
        return path
    }
}

/// A shield needs its own silhouette: a rounded box reads as a square at
/// roster size.
struct MarbleShield: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        let width = rect.width
        let height = rect.height
        path.move(to: CGPoint(x: width * 0.5, y: 0))
        path.addLine(to: CGPoint(x: width, y: height * 0.16))
        path.addLine(to: CGPoint(x: width * 0.94, y: height * 0.58))
        path.addCurve(
            to: CGPoint(x: width * 0.5, y: height),
            control1: CGPoint(x: width * 0.84, y: height * 0.8),
            control2: CGPoint(x: width * 0.62, y: height * 0.93)
        )
        path.addCurve(
            to: CGPoint(x: width * 0.06, y: height * 0.58),
            control1: CGPoint(x: width * 0.38, y: height * 0.93),
            control2: CGPoint(x: width * 0.16, y: height * 0.8)
        )
        path.addLine(to: CGPoint(x: 0, y: height * 0.16))
        path.closeSubpath()
        return path
    }
}

struct MarbleDrop: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        let width = rect.width
        let height = rect.height
        path.move(to: CGPoint(x: width * 0.5, y: 0))
        path.addCurve(
            to: CGPoint(x: width, y: height * 0.62),
            control1: CGPoint(x: width * 0.78, y: height * 0.28),
            control2: CGPoint(x: width, y: height * 0.4)
        )
        path.addCurve(
            to: CGPoint(x: width * 0.5, y: height),
            control1: CGPoint(x: width, y: height * 0.84),
            control2: CGPoint(x: width * 0.78, y: height)
        )
        path.addCurve(
            to: CGPoint(x: 0, y: height * 0.62),
            control1: CGPoint(x: width * 0.22, y: height),
            control2: CGPoint(x: 0, y: height * 0.84)
        )
        path.addCurve(
            to: CGPoint(x: width * 0.5, y: 0),
            control1: CGPoint(x: 0, y: height * 0.4),
            control2: CGPoint(x: width * 0.22, y: height * 0.28)
        )
        path.closeSubpath()
        return path
    }
}

struct MarbleStar: Shape {
    func path(in rect: CGRect) -> Path {
        var path = Path()
        let center = CGPoint(x: rect.midX, y: rect.midY)
        let outer = min(rect.width, rect.height) / 2
        let inner = outer * 0.44
        for index in 0..<10 {
            let radius = index.isMultiple(of: 2) ? outer : inner
            let angle = -CGFloat.pi / 2 + CGFloat(index) * CGFloat.pi / 5
            let point = CGPoint(
                x: center.x + cos(angle) * radius,
                y: center.y + sin(angle) * radius
            )
            if index == 0 {
                path.move(to: point)
            } else {
                path.addLine(to: point)
            }
        }
        path.closeSubpath()
        return path
    }
}
