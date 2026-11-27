import SwiftUI

extension EnvironmentValues {
    /// The iPhone Threads panel is open over this screen.
    @Entry var sidebarOpen = false
}

struct ChatToolbar<Title: View>: View {
    @Environment(\.sidebarOpen) private var sidebarOpen
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
            .opacity(sidebarOpen ? 0 : 1)
            .allowsHitTesting(!sidebarOpen)
            .accessibilityHidden(sidebarOpen)
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
