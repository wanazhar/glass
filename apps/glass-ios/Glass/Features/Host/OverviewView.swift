import GlassModels
import SwiftUI

struct OverviewView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        ZStack {
            Backdrop()
            ScrollView {
                GlassGroup(spacing: 14) {
                    VStack(spacing: 14) {
                        switch model.phase {
                        case .connected(let status):
                            if case .appUpdateRequired(let version) = status.compatibility { updateBanner(version) }
                            hero(status)
                            facts(status)
                            if let capabilities = status.capabilities, !capabilities.isEmpty { capabilityCard(capabilities) }
                        case .unreachable(let message):
                            unreachable(message)
                        case .connecting, .unpaired:
                            ProgressView("Connecting…")
                                .padding(40)
                                .accessibilityIdentifier("host.connecting")
                        }
                    }
                }
                .padding(20)
                .frame(maxWidth: 640)
                .frame(maxWidth: .infinity)
            }
            .refreshable { await model.refresh() }
        }
        .navigationTitle(model.status?.workspaceName ?? "Glass")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button("Refresh", systemImage: "arrow.clockwise") { Task { await model.refresh() } }
                    .accessibilityIdentifier("host.refresh")
            }
        }
    }

    private func hero(_ status: GatewayStatus) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Pill(title: "Connected", symbol: "circle.fill", tint: .green)
                    .accessibilityIdentifier("host.connected")
                Spacer()
                if let endpoint = model.endpoint {
                    Pill(title: endpoint.transportLabel, symbol: endpoint.transportSymbol, tint: endpoint.isSecure ? .blue : .orange)
                }
            }
            Text(status.workspaceName)
                .font(.system(.largeTitle, design: .rounded, weight: .bold))
                .lineLimit(1)
                .minimumScaleFactor(0.6)
                .accessibilityIdentifier("host.workspace")
            Text(status.workspace)
                .font(.system(.footnote, design: .monospaced))
                .foregroundStyle(.secondary)
                .lineLimit(2)
                .truncationMode(.middle)
            if let origin = model.endpoint?.origin {
                Text(origin)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .accessibilityIdentifier("host.origin")
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(20)
        .glassSurface(in: .rect(cornerRadius: 26, style: .continuous))
    }

    private func facts(_ status: GatewayStatus) -> some View {
        LazyVGrid(columns: [GridItem(.flexible(), spacing: 14), GridItem(.flexible(), spacing: 14)], spacing: 14) {
            fact("Profile", value: status.profile?.capitalized ?? "—", symbol: "shield.lefthalf.filled")
            fact("Mode", value: status.remote ? "Remote" : "Local", symbol: "point.3.connected.trianglepath.dotted")
            fact("Protocol", value: protocolLabel(status), symbol: "number")
                .accessibilityIdentifier("host.protocol")
            fact("Secret entry", value: model.secretEntryUnlocked ? "Unlocked" : "Locked",
                 symbol: model.secretEntryUnlocked ? "lock.open.fill" : "lock.fill")
        }
    }

    private func fact(_ title: String, value: String, symbol: String) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Image(systemName: symbol).foregroundStyle(.tint).imageScale(.large)
            Text(value).font(.headline).lineLimit(1).minimumScaleFactor(0.7)
            Text(title).font(.caption).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
        .glassSurface(in: .rect(cornerRadius: 20, style: .continuous))
        .accessibilityElement(children: .combine)
    }

    private func capabilityCard(_ capabilities: [String]) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Host features").font(.subheadline.weight(.semibold))
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 120), spacing: 8, alignment: .leading)], alignment: .leading, spacing: 8) {
                ForEach(capabilities, id: \.self) { Pill(title: $0, tint: .secondary) }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(18)
        .glassSurface(in: .rect(cornerRadius: 22, style: .continuous))
    }

    private func updateBanner(_ version: Int) -> some View {
        Label("This host speaks protocol \(version). Update Glass to keep everything working.", systemImage: "arrow.down.app.fill")
            .font(.subheadline)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(16)
            .glassSurface(in: .rect(cornerRadius: 18, style: .continuous), tint: .orange)
            .accessibilityIdentifier("host.updateRequired")
    }

    private func unreachable(_ message: String) -> some View {
        ContentUnavailableView {
            Label("Host unreachable", systemImage: "wifi.exclamationmark")
        } description: {
            Text(message)
        } actions: {
            Button("Try Again") { Task { await model.refresh() } }
                .glassButtonStyle(prominent: true)
        }
        .padding(.vertical, 24)
        .glassSurface(in: .rect(cornerRadius: 26, style: .continuous))
        .accessibilityIdentifier("host.unreachable")
    }

    private func protocolLabel(_ status: GatewayStatus) -> String {
        switch status.compatibility {
        case .compatible: "v\(status.protocolVersion ?? 0)"
        case .legacyGateway: "Legacy"
        case .appUpdateRequired(let version): "v\(version) · update"
        }
    }
}
