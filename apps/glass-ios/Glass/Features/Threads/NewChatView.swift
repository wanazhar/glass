import SwiftUI

struct NewChatView: View {
    var workspace: String? = nil
    var choose: (String) -> Void
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency

    static func directory(_ workspace: String?, hostDefault: String?) -> String? {
        if let workspace, !workspace.isEmpty { return workspace }
        return hostDefault
    }

    var body: some View {
        VStack(spacing: Design.spacious) {
            Horizon(fullWidth: true)
                .stroke(Design.muted.opacity(0.45), lineWidth: 0.8)
                .frame(height: 112)
                .overlay { LensMark(optical: true).frame(width: 112, height: 112) }
                .accessibilityHidden(true)
            Text("Start with a change")
                .font(Design.title)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 20)
                .accessibilityAddTraits(.isHeader)
            GlassEffectContainer(spacing: 12) {
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 12) { suggestions }
                    VStack(spacing: 10) { suggestions }
                }
            }
            .padding(.horizontal, 18)
        }
        .foregroundStyle(Design.ink)
        .frame(maxWidth: 620)
        .frame(maxWidth: .infinity)
        .padding(.vertical, Design.spacious)
    }

    @ViewBuilder private var suggestions: some View {
        suggestion("Review code", icon: "chevron.left.forwardslash.chevron.right",
                   task: "Review the changes in my current branch.")
        suggestion("Plan a project", icon: "list.bullet.clipboard",
                   task: "Help me plan the next milestone.")
    }

    private func suggestion(_ label: String, icon: String, task: String) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: .soft).impactOccurred(intensity: 0.55)
            choose(task)
        } label: {
            Label(label, systemImage: icon)
                .font(.subheadline.weight(.medium))
                .padding(.horizontal, 16)
                .frame(minHeight: Design.control)
                .contentShape(.capsule)
        }
        .buttonStyle(.plain)
        .background(reduceTransparency ? Design.surface : .clear, in: .capsule)
        .glassEffect(reduceTransparency ? .identity : .regular.interactive(), in: .capsule)
    }
}
