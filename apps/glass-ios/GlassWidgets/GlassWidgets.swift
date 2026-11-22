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
        ActivityConfiguration(for: ConnectionActivityAttributes.self) { context in
            HStack(spacing: 12) {
                Image(systemName: "dot.radiowaves.left.and.right")
                    .font(.title3)
                    .foregroundStyle(.green)
                VStack(alignment: .leading, spacing: 2) {
                    Text(context.state.status).font(.headline)
                    HStack(spacing: 4) {
                        Text(context.attributes.host)
                        Text("·")
                        Text(context.state.connectedSince, style: .timer).monospacedDigit()
                    }
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                }
                Spacer()
                Button(intent: DisconnectIntent()) {
                    Label("Disconnect", systemImage: "xmark")
                }
                .buttonStyle(.bordered)
                .tint(.red)
            }
            .padding()
            .activityBackgroundTint(nil)
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    Label(context.state.status, systemImage: "dot.radiowaves.left.and.right")
                        .font(.subheadline)
                        .foregroundStyle(.green)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    Text(context.state.connectedSince, style: .timer)
                        .monospacedDigit()
                        .frame(maxWidth: 64, alignment: .trailing)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    HStack {
                        Text(context.attributes.host).font(.footnote).foregroundStyle(.secondary)
                        Spacer()
                        Button(intent: DisconnectIntent()) {
                            Label("Disconnect", systemImage: "xmark")
                        }
                        .tint(.red)
                    }
                }
            } compactLeading: {
                Image(systemName: "dot.radiowaves.left.and.right").foregroundStyle(.green)
            } compactTrailing: {
                Text(context.state.connectedSince, style: .timer)
                    .monospacedDigit()
                    .frame(maxWidth: 44)
            } minimal: {
                Image(systemName: "dot.radiowaves.left.and.right").foregroundStyle(.green)
            }
        }
    }
}
