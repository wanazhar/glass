import ActivityKit
import SwiftUI
import WidgetKit

@main
struct MarbleLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: MarbleConnectionActivityAttributes.self) { context in
            HStack(spacing: 16) {
                VStack(alignment: .leading, spacing: 4) {
                    Label("Marble", systemImage: "bubble.left.and.text.bubble.right.fill")
                        .font(.headline)
                    Text(context.attributes.host)
                        .lineLimit(1)
                        .foregroundStyle(.secondary)
                    status(context)
                }
                Spacer(minLength: 0)
                Button(intent: MarbleDisconnectIntent()) {
                    Text("Disconnect")
                }
                .buttonStyle(.bordered)
            }
            .padding()
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    Label("Marble", systemImage: "bubble.left.and.text.bubble.right.fill")
                        .font(.headline)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    HStack {
                        status(context)
                        Spacer()
                        Button(intent: MarbleDisconnectIntent()) {
                            Text("Disconnect")
                        }
                        .buttonStyle(.bordered)
                    }
                }
            } compactLeading: {
                Image(systemName: "bubble.left.and.text.bubble.right.fill")
                    .accessibilityLabel("Marble")
            } compactTrailing: {
                Image(systemName: context.isStale ? "wifi.exclamationmark" : "wifi")
                    .accessibilityLabel(context.isStale ? "Connection unconfirmed" : "Connected")
            } minimal: {
                Image(systemName: context.isStale ? "wifi.exclamationmark" : "wifi")
                    .accessibilityLabel(context.isStale ? "Marble connection unconfirmed" : "Marble connected")
            }
        }
    }

    private func status(_ context: ActivityViewContext<MarbleConnectionActivityAttributes>) -> some View {
        HStack(spacing: 6) {
            if context.isStale {
                Text("Connection unconfirmed")
            } else {
                Text("Connected")
                Text(timerInterval: context.state.connectedAt...Date.distantFuture, countsDown: false)
                    .monospacedDigit()
                    .accessibilityLabel("Connection uptime")
            }
        }
        .font(.subheadline)
    }
}
