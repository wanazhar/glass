import ActivityKit
import SwiftUI
import WidgetKit

@main
struct MarbleLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: MarbleConnectionActivityAttributes.self) { context in
            atExpiry(context) { date in
                HStack(spacing: 16) {
                    VStack(alignment: .leading, spacing: 4) {
                        Label("Marble", systemImage: "bubble.left.and.text.bubble.right.fill")
                            .font(.headline)
                        Text(context.attributes.host)
                            .lineLimit(1)
                            .foregroundStyle(.secondary)
                        status(context, at: date)
                    }
                    Spacer(minLength: 0)
                    disconnectButton(context, at: date)
                }
                .padding()
            }
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    Label("Marble", systemImage: "bubble.left.and.text.bubble.right.fill")
                        .font(.headline)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    atExpiry(context) { date in
                        HStack {
                            status(context, at: date)
                            Spacer()
                            disconnectButton(context, at: date)
                        }
                    }
                }
            } compactLeading: {
                Image(systemName: "bubble.left.and.text.bubble.right.fill")
                    .accessibilityLabel("Marble")
            } compactTrailing: {
                atExpiry(context) { connectionIcon(context, at: $0) }
            } minimal: {
                atExpiry(context) { connectionIcon(context, at: $0) }
            }
        }
    }

    private func atExpiry<Content: View>(
        _ context: ActivityViewContext<MarbleConnectionActivityAttributes>,
        @ViewBuilder content: @escaping (Date) -> Content
    ) -> some View {
        let now = Date.now
        let next = max(now, context.state.expiresAt ?? now)
        return TimelineView(.explicit([now, next])) { entry in
            content(entry.date)
        }
    }

    private func isExpired(_ context: ActivityViewContext<MarbleConnectionActivityAttributes>, at date: Date) -> Bool {
        context.isStale || context.state.expiresAt.map { $0 <= date } == true
    }

    private func connectionIcon(_ context: ActivityViewContext<MarbleConnectionActivityAttributes>, at date: Date) -> some View {
        let connected = context.state.connectedAt != nil
        let expired = isExpired(context, at: date)
        return Image(systemName: expired ? "wifi.slash" : connected ? "wifi" : "wifi.exclamationmark")
            .accessibilityLabel(expired ? "Marble disconnected" : connected ? "Marble connection check" : "Marble reconnecting")
    }

    @ViewBuilder
    private func disconnectButton(_ context: ActivityViewContext<MarbleConnectionActivityAttributes>, at date: Date) -> some View {
        if !isExpired(context, at: date) {
            Button("Disconnect", intent: MarbleDisconnectIntent())
                .buttonStyle(.bordered)
        }
    }

    private func status(_ context: ActivityViewContext<MarbleConnectionActivityAttributes>, at date: Date) -> some View {
        HStack(spacing: 6) {
            if isExpired(context, at: date) {
                Text("Disconnected")
            } else if context.state.connectedAt != nil, let expiresAt = context.state.expiresAt {
                // The system timer reaches zero even if ActivityKit delivers
                // the stale state late after the application has exited.
                Text("Check expires in")
                Text(timerInterval: date...max(date, expiresAt), countsDown: true, showsHours: false)
                    .monospacedDigit()
            } else {
                Text("Reconnecting")
            }
        }
        .font(.subheadline)
    }
}
