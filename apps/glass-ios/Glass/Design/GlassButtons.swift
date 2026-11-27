import SwiftUI

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
