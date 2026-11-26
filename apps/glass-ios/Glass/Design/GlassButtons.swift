import SwiftUI

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
