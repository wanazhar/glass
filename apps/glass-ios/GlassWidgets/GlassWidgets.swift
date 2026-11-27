import ActivityKit
import AppIntents
import SwiftUI
import WidgetKit

@main
struct GlassWidgets: WidgetBundle {
    var body: some Widget {
        ConnectionLiveActivity()
    }
}

struct ConnectionLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: LensConnectionAttributes.self) { context in
            HStack(spacing: 8) {
                mark(context, size: 36)
                connection(context)
            }
            .padding(16)
            .activityBackgroundTint(nil)
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    mark(context, size: 28, onIsland: true)
                        .padding(.leading, 6)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    connection(context)
                        .padding(.horizontal, 6)
                }
            } compactLeading: {
                mark(context, size: 22, onIsland: true)
            } compactTrailing: {
                let count = context.isStale ? nil : context.state.workingAgents
                Text(count.map { "\($0) \($0 == 1 ? "agent" : "agents")" } ?? (context.isStale ? "Closed" : "Live"))
                    .font(.caption.weight(.semibold))
            } minimal: {
                mark(context, size: 22, onIsland: true)
            }
        }
    }

    private func mark(_ context: ActivityViewContext<LensConnectionAttributes>, size: CGFloat,
                      onIsland: Bool = false) -> some View {
        ActivityLensMark(stale: context.isStale, pulse: context.state.pulse, onIsland: onIsland)
            .frame(width: size, height: size)
    }

    private func connection(_ context: ActivityViewContext<LensConnectionAttributes>) -> some View {
        HStack(spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(context.isStale ? "Closed" : "Connected").font(.headline)
                Text(context.attributes.host).font(.subheadline).foregroundStyle(.secondary)
                if !context.isStale, let count = context.state.workingAgents {
                    HStack(spacing: 4) {
                        Text("\(count) \(count == 1 ? "agent" : "agents") working")
                            .fixedSize()
                        if let oldest = context.state.longestAgentStart {
                            Text("· Longest").fixedSize()
                            Text(timerInterval: oldest ... .distantFuture, countsDown: false)
                                .monospacedDigit()
                                .frame(width: 54, alignment: .leading)
                        }
                    }
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.secondary)
                }
            }
            .lineLimit(1)
            Spacer(minLength: 4)
            if !context.isStale {
                Button(intent: DisconnectIntent()) {
                    HStack(spacing: 5) {
                        Image(systemName: "xmark").accessibilityHidden(true)
                        Text("Disconnect")
                    }
                    .font(.footnote.weight(.semibold))
                    .fixedSize(horizontal: true, vertical: false)
                }
                .buttonStyle(.bordered)
                .buttonBorderShape(.capsule)
                .tint(.red)
                .layoutPriority(1)
            }
        }
    }
}

private struct ActivityLensMark: View {
    var stale: Bool
    var pulse: Bool
    var onIsland: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        let teal = Color(lensHex: onIsland || scheme == .dark ? LensPalette.darkTeal : LensPalette.lightTeal)
        LensGlyph(tint: stale ? .secondary : teal)
            .opacity(!stale && !reduceMotion && pulse ? 0.72 : 1)
            .animation(reduceMotion ? nil : .easeInOut(duration: 0.6), value: pulse)
            .accessibilityHidden(true)
    }
}
