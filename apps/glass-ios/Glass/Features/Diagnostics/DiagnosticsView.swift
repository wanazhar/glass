import GlassModels
import SwiftUI

/// Client-side connection facts plus the host's read-only `GET /api/diagnostics` report.
struct DiagnosticsView: View {
    @Environment(AppModel.self) private var model
    @State private var report: JSONValue?
    @State private var error: String?
    @State private var loading = false

    var body: some View {
        List {
            Section("This iPhone") {
                row("App", "\(model.info.displayName) \(model.info.versionLabel)")
                row("Client protocol", "v\(GlassClientKitVersion.supportedProtocolVersion)")
                if let endpoint = model.endpoint {
                    row("Host", endpoint.origin).accessibilityIdentifier("diag.origin")
                    row("Transport", endpoint.transportLabel)
                }
                if let roundTrip = model.lastRoundTrip {
                    row("Status round trip", roundTrip.formatted(.units(allowed: [.milliseconds], width: .abbreviated)))
                }
                if let refreshed = model.lastRefreshed {
                    row("Last refreshed", refreshed.formatted(date: .omitted, time: .standard))
                }
            }
            if let status = model.status {
                Section("Gateway") {
                    row("Protocol", status.protocolVersion.map { "v\($0)" } ?? "Not advertised")
                    row("Connect command", status.connectCommand ?? "—")
                    row("Profile", status.profile ?? "—")
                }
            }
            if let error {
                Section {
                    Label(error, systemImage: "exclamationmark.triangle.fill").foregroundStyle(.red)
                }
            }
            if let report { reportSections(report) }
        }
        .overlay { if loading && report == nil { ProgressView() } }
        .navigationTitle("Diagnostics")
        .refreshable { await load() }
        .task { if report == nil { await load() } }
        .toolbar {
            if let report {
                ToolbarItem(placement: .primaryAction) {
                    ShareLink(item: report.prettyPrinted, preview: SharePreview("Glass diagnostics")) {
                        Label("Share report", systemImage: "square.and.arrow.up")
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func reportSections(_ report: JSONValue) -> some View {
        if let core = report["core"] {
            Section("Core \(core["version"]?.stringValue ?? "")") {
                ForEach(Array((core["checks"]?.arrayValue ?? []).enumerated()), id: \.offset) { _, check in
                    CheckRow(check: check)
                }
            }
            .accessibilityIdentifier("diag.core")
        }
        if let host = report["host"] {
            Section("Host machine") {
                row("System", [host["os"]?.stringValue, host["architecture"]?.stringValue].compactMap { $0 }.joined(separator: " · "))
                if let cpus = host["logicalCpus"]?.doubleValue { row("Logical CPUs", "\(Int(cpus))") }
                if let free = host["diskAvailableBytes"]?.doubleValue, let total = host["diskTotalBytes"]?.doubleValue {
                    row("Disk free", "\(bytes(free)) of \(bytes(total))")
                }
            }
        }
        let known: Set<String> = ["core", "host"]
        if case .object(let object) = report {
            ForEach(object.keys.sorted().filter { !known.contains($0) }, id: \.self) { key in
                if let value = object[key], value != .null {
                    Section(key.titleCased) { JSONRows(value: value) }
                }
            }
        }
    }

    private func row(_ title: String, _ value: String) -> some View {
        LabeledContent(title) {
            Text(value).textSelection(.enabled).multilineTextAlignment(.trailing)
        }
    }

    private func bytes(_ value: Double) -> String {
        ByteCountFormatter.string(fromByteCount: Int64(value), countStyle: .file)
    }

    private func load() async {
        loading = true
        defer { loading = false }
        do {
            report = try await model.diagnostics()
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}

private struct CheckRow: View {
    let check: JSONValue

    var body: some View {
        let status = check["status"]?.stringValue ?? ""
        Label {
            VStack(alignment: .leading, spacing: 2) {
                Text((check["area"]?.stringValue ?? "").capitalized).font(.subheadline.weight(.medium))
                Text(check["detail"]?.stringValue ?? "").font(.footnote).foregroundStyle(.secondary)
            }
        } icon: {
            Image(systemName: symbol(status)).foregroundStyle(color(status))
        }
        .accessibilityElement(children: .combine)
    }

    private func symbol(_ status: String) -> String {
        switch status {
        case "ok": "checkmark.circle.fill"
        case "warning", "warn": "exclamationmark.triangle.fill"
        case "error", "fail", "failed": "xmark.octagon.fill"
        default: "info.circle.fill"
        }
    }

    private func color(_ status: String) -> Color {
        switch status {
        case "ok": .green
        case "warning", "warn": .orange
        case "error", "fail", "failed": .red
        default: .blue
        }
    }
}

/// Generic fallback for report fields this build doesn't model explicitly.
private struct JSONRows: View {
    let value: JSONValue

    var body: some View {
        switch value {
        case .object(let object):
            ForEach(object.keys.sorted(), id: \.self) { key in
                if let child = object[key] {
                    if child.isScalar {
                        LabeledContent(key.titleCased, value: child.displayString)
                    } else {
                        NavigationLink(key.titleCased) {
                            List { JSONRows(value: child) }.navigationTitle(key.titleCased)
                        }
                    }
                }
            }
        case .array(let items) where items.isEmpty:
            Text("None").foregroundStyle(.secondary)
        case .array(let items):
            ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                if item.isScalar {
                    Text(item.displayString)
                } else {
                    NavigationLink("Item \(index + 1)") { List { JSONRows(value: item) }.navigationTitle("Item \(index + 1)") }
                }
            }
        default:
            Text(value.displayString)
        }
    }
}

extension JSONValue {
    var arrayValue: [JSONValue]? { if case .array(let items) = self { items } else { nil } }

    var isScalar: Bool {
        switch self {
        case .object, .array: false
        default: true
        }
    }

    var displayString: String {
        switch self {
        case .null: "—"
        case .bool(let b): b ? "Yes" : "No"
        case .number(let n): n.rounded() == n && abs(n) < 1e15 ? String(Int64(n)) : String(n)
        case .string(let s): s
        case .array(let items): "\(items.count) items"
        case .object(let o): "\(o.count) fields"
        }
    }

    var prettyPrinted: String {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]
        return (try? encoder.encode(self)).flatMap { String(data: $0, encoding: .utf8) } ?? ""
    }
}

extension String {
    /// `bridgeCount` → `Bridge Count`, `schema_version` → `Schema Version`.
    var titleCased: String {
        var words: [String] = []
        var current = ""
        for character in self {
            if character == "_" || character == "-" {
                if !current.isEmpty { words.append(current); current = "" }
            } else if character.isUppercase, !current.isEmpty, current.last?.isLowercase == true {
                words.append(current); current = String(character)
            } else {
                current.append(character)
            }
        }
        if !current.isEmpty { words.append(current) }
        return words.map { $0.prefix(1).uppercased() + $0.dropFirst() }.joined(separator: " ")
    }
}
