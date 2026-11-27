import SwiftUI

extension EnvironmentValues {
    /// The iPhone Threads panel is open over this screen.
    @Entry var sidebarOpen = false
}

extension View {
    /// Chat chrome steps away while the Threads panel is open, so the panel is the only glass on screen.
    func hiddenUnderSidebar() -> some View { modifier(HiddenUnderSidebar()) }
}

private struct HiddenUnderSidebar: ViewModifier {
    @Environment(\.sidebarOpen) private var sidebarOpen

    func body(content: Content) -> some View {
        content
            .opacity(sidebarOpen ? 0 : 1)
            .allowsHitTesting(!sidebarOpen)
            .accessibilityHidden(sidebarOpen)
    }
}

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
        .background(alignment: .top) { HeaderFade() }
        .hiddenUnderSidebar()
    }

    private func control(_ symbol: String, label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            LensBarIcon(symbol: symbol)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }
}

/// The chat's own background behind the status bar and the controls row, fading out below them, so text
/// scrolls away before it reaches the title instead of running underneath it.
private struct HeaderFade: View {
    static let fade = 2 * (Design.spacious + Design.gap)

    var body: some View {
        VStack(spacing: 0) {
            Design.background
            // Blur first, then fade, so text passing under the title is softened rather than half-visible.
            ZStack {
                Rectangle().fill(.ultraThinMaterial)
                    .mask(LinearGradient(colors: [.black, .black.opacity(0)], startPoint: .top, endPoint: .bottom))
                LinearGradient(stops: [.init(color: Design.background, location: 0), .init(color: Design.background, location: 0.25),
                                       .init(color: Design.background.opacity(0), location: 0.7)],
                               startPoint: .top, endPoint: .bottom)
            }
            .frame(height: Self.fade)
        }
            .padding(.bottom, -Self.fade)
            .ignoresSafeArea(edges: .top)
            .allowsHitTesting(false)
            .accessibilityHidden(true)
    }
}
