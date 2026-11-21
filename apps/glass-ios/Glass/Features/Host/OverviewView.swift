import GlassModels
import SwiftUI

struct OverviewView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        Group {
            switch model.phase {
            case .connected(let status):
                List { sections(status) }
                    .refreshable { await model.refresh() }
            case .unreachable(let message):
                ContentUnavailableView {
                    Label("Host unreachable", systemImage: "wifi.exclamationmark")
                } description: {
                    Text(message)
                } actions: {
                    Button("Try Again") { Task { await model.refresh() } }
                        .glassButtonStyle(prominent: true)
                }
                .accessibilityIdentifier("host.unreachable")
            case .connecting, .unpaired:
                ProgressView("Connecting")
                    .accessibilityIdentifier("host.connecting")
            }
        }
        .navigationTitle("Host")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button("Refresh", systemImage: "arrow.clockwise") { Task { await model.refresh() } }
                    .accessibilityIdentifier("host.refresh")
            }
        }
    }

    @ViewBuilder
    private func sections(_ status: GatewayStatus) -> some View {
        if case .appUpdateRequired(let version) = status.compatibility {
            Section {
                Label {
                    Text("This host is newer than this app. Update Glass to use every feature.")
                } icon: {
                    Image(systemName: "arrow.down.app").foregroundStyle(.orange)
                }
                .accessibilityIdentifier("host.updateRequired")
            }
        }

        Section {
            LabeledContent("Status") {
                StatusLabel(title: "Connected", color: .green)
                    .accessibilityElement(children: .combine)
                    .accessibilityIdentifier("host.connected")
            }
            LabeledContent("Workspace") {
                Text(status.workspaceName).accessibilityIdentifier("host.workspace")
            }
            VStack(alignment: .leading, spacing: 4) {
                Text("Path")
                Text(status.workspace)
                    .font(.system(.footnote, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
            .accessibilityElement(children: .combine)
        }

        if let endpoint = model.endpoint {
            Section("Connection") {
                LabeledContent("Transport") {
                    Text(endpoint.transportLabel)
                        .foregroundStyle(endpoint.isSecure ? Color.secondary : Color.orange)
                }
                LabeledContent("Address") {
                    Text(endpoint.origin)
                        .textSelection(.enabled)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .accessibilityIdentifier("host.origin")
                }
                LabeledContent("Mode", value: status.remote ? "Remote" : "Local")
                LabeledContent("Secret entry", value: model.secretEntryUnlocked ? "Unlocked" : "Locked")
            }
        }

        Section {
            LabeledContent("Profile", value: status.profile?.capitalized ?? "Unknown")
            LabeledContent("Compatibility") {
                Text(protocolLabel(status)).accessibilityIdentifier("host.protocol")
            }
        } header: {
            Text("Host")
        }
    }

    private func protocolLabel(_ status: GatewayStatus) -> String {
        switch status.compatibility {
        case .compatible: "Up to date"
        case .legacyGateway: "Older host"
        case .appUpdateRequired: "Update Glass"
        }
    }
}
