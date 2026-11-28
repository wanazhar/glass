import ActivityKit
import SwiftUI
import WidgetKit

@main
struct MarbleLiveActivities: WidgetBundle {
    var body: some Widget {
        MarbleWorkWidget()
        MarbleConnectionWidget()
    }
}

struct MarbleConnectionWidget: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: MarbleConnectionActivityAttributes.self) { context in
            atExpiry(context) { date in
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: 12) {
                        connectionDetails(context, at: date)
                        Spacer(minLength: 0)
                        disconnectButton(context, at: date)
                    }
                    VStack(alignment: .leading, spacing: 8) {
                        connectionDetails(context, at: date)
                        disconnectButton(context, at: date)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(16)
            }
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    Label("Marble", systemImage: "bubble.left.and.text.bubble.right.fill")
                        .font(.subheadline.weight(.semibold))
                }
                DynamicIslandExpandedRegion(.bottom) {
                    atExpiry(context) { date in
                        ViewThatFits(in: .horizontal) {
                            HStack(spacing: 8) {
                                status(context, at: date)
                                Spacer(minLength: 0)
                                disconnectButton(context, at: date)
                            }
                            VStack(alignment: .leading, spacing: 4) {
                                status(context, at: date)
                                disconnectButton(context, at: date)
                            }
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

    private func connectionDetails(
        _ context: ActivityViewContext<MarbleConnectionActivityAttributes>, at date: Date
    ) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Label("Marble", systemImage: "bubble.left.and.text.bubble.right.fill")
                .font(.subheadline.weight(.semibold))
            Text(context.attributes.host)
                .font(.footnote)
                .foregroundStyle(.secondary)
            status(context, at: date)
        }
        .fixedSize(horizontal: false, vertical: true)
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
            Button(intent: MarbleDisconnectIntent()) {
                Text("Disconnect")
                    .font(.footnote)
                    .frame(minHeight: 36)
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
    }

    private func status(_ context: ActivityViewContext<MarbleConnectionActivityAttributes>, at date: Date) -> some View {
        VStack(alignment: .leading, spacing: 2) {
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
        .font(.footnote)
    }
}
