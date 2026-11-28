import ActivityKit
import SwiftUI
import WidgetKit

struct MarbleWorkWidget: Widget {
    private typealias Context = ActivityViewContext<MarbleWorkActivityAttributes>

    var body: some WidgetConfiguration {
        ActivityConfiguration(for: MarbleWorkActivityAttributes.self) { context in
            HStack(spacing: 12) {
                avatar(context, size: 36).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 4) {
                    Text(context.state.agentName)
                        .font(.headline)
                        .lineLimit(1)
                    status(context)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                    elapsed(context)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 0)
                stop(context)
            }
            .padding(16)
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    HStack(spacing: 8) {
                        avatar(context, size: 24).accessibilityHidden(true)
                        Text(context.state.agentName)
                            .font(.headline)
                            .lineLimit(1)
                    }
                }
                DynamicIslandExpandedRegion(.trailing) {
                    elapsed(context)
                        .font(.subheadline)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    HStack {
                        status(context)
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                        Spacer(minLength: 8)
                        stop(context)
                    }
                }
            } compactLeading: {
                HStack(spacing: 4) {
                    avatar(context, size: 14)
                    Text(context.state.agentName)
                        .font(.caption2)
                        .lineLimit(1)
                        .minimumScaleFactor(0.75)
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(context.state.agentName)
                .accessibilityValue(status(context))
            } compactTrailing: {
                if context.isStale {
                    Text("—").accessibilityLabel("Status unavailable")
                } else {
                    elapsed(context)
                        .font(.caption2)
                        .frame(maxWidth: 46)
                }
            } minimal: {
                avatar(context, size: 17)
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(context.state.agentName)
                    .accessibilityValue(status(context))
            }
            .keylineTint(MarbleAvatarAppearance.fill(for: context.state.hue))
        }
    }

    private func avatar(_ context: Context, size: CGFloat) -> some View {
        MarbleWorkAvatar(glyph: context.state.glyph, hue: context.state.hue,
                         phase: context.state.phase, stale: context.isStale)
            .frame(width: size, height: size)
    }

    private func status(_ context: Context) -> Text {
        let phase: LocalizedStringKey
        if context.isStale {
            phase = "Status unavailable"
        } else {
            switch context.state.phase {
            case .thinking: phase = "Thinking"
            case .tool: phase = "Running a tool"
            case .replying: phase = "Replying"
            case .stopping: phase = "Stopping"
            }
        }
        return context.state.isEveryone ? Text("Everyone · \(Text(phase))") : Text(phase)
    }

    @ViewBuilder
    private func elapsed(_ context: Context) -> some View {
        if !context.isStale {
            Text(context.state.startedAt, style: .timer)
                .monospacedDigit()
                .multilineTextAlignment(.trailing)
                .lineLimit(1)
                .accessibilityLabel("Elapsed time")
        }
    }

    private func stop(_ context: Context) -> some View {
        Button(intent: MarbleStopTurnIntent(taskID: context.state.taskID, hostID: context.attributes.hostID)) {
            Text("Stop")
                .font(.subheadline.weight(.semibold))
                .frame(minWidth: 44, minHeight: 44)
        }
        .buttonStyle(.borderedProminent)
        .tint(.red)
        .disabled(context.state.phase == .stopping && !context.isStale)
    }
}

private struct MarbleWorkAvatar: View {
    var glyph: String
    var hue: Int
    var phase: MarbleWorkPhase
    var stale: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let moving = !reduceMotion && !stale
        MarbleSilhouette(glyph: glyph)
            .fill(MarbleAvatarAppearance.fill(for: hue))
            .padding(2)
            .scaleEffect(moving && phase == .replying ? 1.1 : 1)
            .rotationEffect(.degrees(moving ? rotation : 0))
            .opacity(stale ? 0.55 : 1)
            .id(moving ? phase : .thinking)
            .transition(moving ? .scale(scale: 0.94).combined(with: .opacity) : .identity)
            .animation(moving ? .easeInOut(duration: 0.35) : nil, value: phase)
    }

    private var rotation: Double {
        switch phase {
        case .thinking: 0
        case .tool: -9
        case .replying: 9
        case .stopping: 0
        }
    }
}
