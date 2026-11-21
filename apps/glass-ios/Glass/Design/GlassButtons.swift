import SwiftUI

extension View {
    /// `.glassProminent` / `.glass` on iOS 26; bordered styles on iOS 18–25.
    @ViewBuilder
    func glassButtonStyle(prominent: Bool = false) -> some View {
        if #available(iOS 26.0, *) {
            if prominent { buttonStyle(.glassProminent) } else { buttonStyle(.glass) }
        } else {
            if prominent { buttonStyle(.borderedProminent) } else { buttonStyle(.bordered) }
        }
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
