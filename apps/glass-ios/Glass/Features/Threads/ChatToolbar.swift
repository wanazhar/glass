import SwiftUI

struct ChatToolbar<Title: View>: View {
    let menu: () -> Void
    let newChat: () -> Void
    @ViewBuilder let title: () -> Title

    var body: some View {
        LensTitleBar {
            LensBarGroup {
                control("line.3.horizontal", label: "Threads", action: menu)
                    .accessibilityIdentifier("conversation.back")
            }
        } title: {
            title()
        } trailing: {
            LensBarGroup {
                control("square.and.pencil", label: "New chat", action: newChat)
                    .accessibilityIdentifier("conversation.new")
            }
        }
        .frame(maxWidth: Design.readingWidth)
        .frame(maxWidth: .infinity)
    }

    private func control(_ symbol: String, label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            LensBarIcon(symbol: symbol)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }
}
