import SwiftUI

struct LensSearch: UIViewRepresentable {
    var prompt: String
    @Binding var query: String

    func makeCoordinator() -> Coordinator { Coordinator(query: $query) }

    func makeUIView(context: Context) -> UISearchTextField {
        let field = UISearchTextField()
        field.accessibilityTraits.insert(.searchField)
        field.attributedPlaceholder = NSAttributedString(string: prompt, attributes: [.foregroundColor: UIColor(Design.muted)])
        field.font = .preferredFont(forTextStyle: .subheadline)
        field.adjustsFontForContentSizeCategory = true
        field.backgroundColor = UIColor(Design.inset)
        field.borderStyle = .none
        field.layer.cornerCurve = .continuous
        field.layer.cornerRadius = Design.control / 2
        field.clipsToBounds = true
        field.returnKeyType = .search
        field.delegate = context.coordinator
        field.addTarget(context.coordinator, action: #selector(Coordinator.changed(_:)), for: .editingChanged)
        return field
    }

    func updateUIView(_ field: UISearchTextField, context: Context) {
        context.coordinator.query = $query
        if field.text != query { field.text = query }
    }

    final class Coordinator: NSObject, UITextFieldDelegate {
        var query: Binding<String>
        init(query: Binding<String>) { self.query = query }
        @objc func changed(_ field: UITextField) { query.wrappedValue = field.text ?? "" }
        func textFieldShouldReturn(_ textField: UITextField) -> Bool {
            textField.resignFirstResponder()
            return false
        }
    }
}

struct LensBarGroup<Content: View>: View {
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @ViewBuilder var content: Content

    var body: some View {
        GlassGroup {
            HStack(spacing: 0) { content }
                .padding(.horizontal, Design.small)
                .frame(height: Design.control)
                .foregroundStyle(Design.ink)
                .background(reduceTransparency ? Design.surface : .clear, in: .capsule)
                .glassEffect(reduceTransparency ? .identity : .regular, in: .capsule)
        }
    }
}

struct LensBarIcon: View {
    let symbol: String

    var body: some View {
        Image(systemName: symbol)
            .font(.system(size: 16, weight: .semibold))
            .frame(width: Design.control, height: Design.control)
            .contentShape(.capsule)
    }
}

struct LensSheetBar: View {
    var title: String
    var closeLabel: String
    var close: () -> Void

    var body: some View {
        LensTitleBar {
            LensBarGroup {
                Button(action: close) { LensBarIcon(symbol: "xmark") }
                    .buttonStyle(.plain)
                    .accessibilityLabel(closeLabel)
            }
        } title: {
            Text(title).font(Design.headline).accessibilityAddTraits(.isHeader)
        } trailing: {
            EmptyView()
        }
    }
}

struct LensTitleBar<Leading: View, Title: View, Trailing: View>: View {
    var sideWidth: CGFloat = Design.control + 2 * Design.small
    @ViewBuilder var leading: Leading
    @ViewBuilder var title: Title
    @ViewBuilder var trailing: Trailing

    var body: some View {
        HStack(spacing: Design.small) {
            Color.clear.frame(width: sideWidth).overlay(alignment: .leading) { leading }
            title.lineLimit(1).frame(maxWidth: .infinity)
            Color.clear.frame(width: sideWidth).overlay(alignment: .trailing) { trailing }
        }
        .frame(height: Design.control)
        .padding(.horizontal, Design.padding)
    }
}

struct AccentAction: View {
    let title: String
    var icon: String?
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: Design.gap) {
                if let icon { Image(systemName: icon) }
                Text(title)
            }
            .font(.subheadline.weight(.semibold))
            .padding(.horizontal, Design.padding)
            .frame(minHeight: Design.control)
            .foregroundStyle(Design.onAccent)
            .background(Design.accent, in: .capsule)
            .contentShape(.capsule)
        }
        .buttonStyle(.plain)
    }
}

func workElapsed(_ duration: Duration) -> String {
    let seconds = max(0, duration.components.seconds)
    if seconds >= 3600 { return "\(seconds / 3600)h \(seconds / 60 % 60)m" }
    if seconds >= 60 { return "\(seconds / 60)m \(seconds % 60)s" }
    return "\(seconds)s"
}

extension View {
    @ViewBuilder
    func glassButtonStyle(prominent: Bool = false) -> some View {
        if prominent { buttonStyle(.glassProminent) } else { buttonStyle(.glass) }
    }
}

/// A status word with a small colored dot, for list rows and toolbars.
struct StatusLabel: View {
    var title: String
    var color: Color

    var body: some View {
        HStack(spacing: 6) {
            Circle().fill(color).frame(width: 7, height: 7).accessibilityHidden(true)
            Text(title)
        }
    }
}
