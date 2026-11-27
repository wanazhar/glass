import SwiftUI

struct ChatToolbar<Title: View>: View {
    let menu: () -> Void
    let newChat: () -> Void
    @ViewBuilder let title: () -> Title
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    var body: some View {
        GlassGroup(spacing: 12) {
            HStack(spacing: 6) {
                control("line.3.horizontal", label: "Threads", action: menu)
                    .accessibilityIdentifier("conversation.back")
                title()
                control("square.and.pencil", label: "New chat", action: newChat)
                    .accessibilityIdentifier("conversation.new")
            }
            .padding(3)
            .foregroundStyle(Design.ink)
            .background(reduceTransparency ? Design.surface : .clear, in: .capsule)
            .glassEffect(reduceTransparency ? .identity : .regular, in: .capsule)
        }
        .frame(maxWidth: 640)
        .padding(.horizontal, Design.padding)
        .frame(maxWidth: .infinity)
    }

    private func control(_ symbol: String, label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 16, weight: .semibold))
                .frame(width: Design.control, height: Design.control)
                .contentShape(.circle)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }
}
