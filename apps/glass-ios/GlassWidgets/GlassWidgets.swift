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
    /// Shown once Glass stopped renewing the activity, for example after it was closed without notice.
    static let closed = "Glass closed"

    var body: some WidgetConfiguration {
        ActivityConfiguration(for: ConnectionActivityAttributes.self) { context in
            HStack(spacing: 12) {
                Image(systemName: "dot.radiowaves.left.and.right")
                    .font(.title3)
                    .foregroundStyle(.green)
                VStack(alignment: .leading, spacing: 2) {
                    Text(context.isStale ? Self.closed : context.state.status).font(.headline)
                    HStack(spacing: 4) {
                        Text(context.attributes.host)
                        if !context.isStale {
                            Text("·")
                            Text(context.state.connectedSince, style: .timer).monospacedDigit()
                        }
                    }
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                }
                Spacer()
                if !context.isStale {
                    Button(intent: DisconnectIntent()) {
                        Label("Disconnect", systemImage: "xmark")
                    }
                    .buttonStyle(.bordered)
                    .tint(.red)
                }
            }
            .padding()
            .activityBackgroundTint(nil)
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    Label(context.isStale ? Self.closed : context.state.status, systemImage: "dot.radiowaves.left.and.right")
                        .font(.subheadline)
                        .foregroundStyle(context.isStale ? AnyShapeStyle(.secondary) : AnyShapeStyle(.green))
                }
                DynamicIslandExpandedRegion(.trailing) {
                    if !context.isStale {
                        Text(context.state.connectedSince, style: .timer)
                            .monospacedDigit()
                            .frame(maxWidth: 64, alignment: .trailing)
                    }
                }
                DynamicIslandExpandedRegion(.bottom) {
                    HStack {
                        Text(context.attributes.host).font(.footnote).foregroundStyle(.secondary)
                        Spacer()
                        if !context.isStale {
                            Button(intent: DisconnectIntent()) {
                                Label("Disconnect", systemImage: "xmark")
                            }
                            .tint(.red)
                        }
                    }
                }
            } compactLeading: {
                Image(systemName: "dot.radiowaves.left.and.right").foregroundStyle(.green)
            } compactTrailing: {
                if !context.isStale {
                    Text(context.state.connectedSince, style: .timer)
                        .monospacedDigit()
                        .frame(maxWidth: 44)
                }
            } minimal: {
                Image(systemName: "dot.radiowaves.left.and.right").foregroundStyle(.green)
            }
        }
    }
}
