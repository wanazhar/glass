import SwiftUI

struct DiffChip: View {
    var titleSample = false

    var body: some View {
        HStack(spacing: 7) {
            Image(systemName: "point.topleft.down.to.point.bottomright.curvepath")
            Text(titleSample ? "TitleView.swift" : "cache.swift").lineLimit(1)
            Text(titleSample ? "+1" : "+3").foregroundStyle(p.success)
            Text("−1").foregroundStyle(p.failure)
        }
        .font(.caption.monospaced())
        .padding(.horizontal, 9).padding(.vertical, 6)
        .background(p.inset, in: .capsule)
    }

    @Environment(\.palette) private var p
}

struct WorkLog: View {
    let running: Bool
    var expanded = false
    var titleSample = false
    var onDiff: () -> Void = {}
    @State private var details = false
    @State private var reasoning = false
    @Environment(\.palette) private var p

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Button {
                withAnimation(.snappy(duration: 0.24)) { details.toggle() }
            } label: {
                HStack(spacing: 10) {
                    if running {
                        ProgressView().controlSize(.small).tint(p.accent).frame(width: 20)
                    } else {
                        Image(systemName: titleSample ? "checkmark.circle" : "checkmark.circle.fill")
                            .foregroundStyle(titleSample ? p.accent : p.success).frame(width: 20)
                    }
                    Text(running ? "Working for 2m 14s" : "Worked for 2m 14s")
                        .font(.subheadline.weight(titleSample && !details ? .medium : .semibold))
                        .foregroundStyle(titleSample && !details ? p.muted : p.ink)
                    Spacer(minLength: 4)
                    Image(systemName: details ? "chevron.up" : "chevron.down").font(.caption2.weight(.semibold))
                }
                .foregroundStyle(p.ink)
                .frame(minHeight: titleSample && !details ? 44 : 28)
                .contentShape(.rect)
            }
            .buttonStyle(.plain)
            .accessibilityHint(details ? "Collapse work log" : "Expand work log")
            if details {
                VStack(alignment: .leading, spacing: 13) {
                    traceRow("01", titleSample ? "Read TitleView.swift" : "Read 3 files")
                    VStack(alignment: .leading, spacing: 6) {
                        Button {
                            withAnimation(.snappy(duration: 0.24)) { reasoning.toggle() }
                        } label: {
                            HStack(spacing: 5) {
                                Text("Reasoning summary")
                                Image(systemName: reasoning ? "chevron.up" : "chevron.down").font(.caption2)
                            }
                            .font(.subheadline.weight(.medium))
                            .foregroundStyle(p.accent)
                        }
                        .buttonStyle(.plain)
                        if reasoning {
                            Text(titleSample
                                 ? "Use the fallback only for missing titles so existing text stays intact."
                                 : "Compare cold and warm traffic separately; the cache only affects repeat requests.")
                                .font(.footnote)
                                .foregroundStyle(p.muted)
                        }
                    }
                    .padding(.leading, 29)
                    traceRow("02", titleSample ? "Title tests passed" : (running ? "Rerunning gateway benchmark" : "Gateway benchmark finished"))
                    Button(action: onDiff) { DiffChip(titleSample: titleSample) }
                        .buttonStyle(.plain)
                        .padding(.leading, 29)
                }
                .padding(.leading, 6)
                .overlay(alignment: .leading) { Rectangle().fill(p.accent.opacity(0.44)).frame(width: 1) }
            }
        }
        .padding(titleSample && !details ? 0 : 13)
        .background {
            if !titleSample || details { Color.clear.panel() }
        }
        .onAppear { details = expanded }
    }

    private func traceRow(_ index: String, _ title: String) -> some View {
        HStack(spacing: 10) {
            Text(index).font(.caption2.monospaced()).foregroundStyle(p.accent).frame(width: 19)
            Text(title).font(.subheadline)
        }
    }
}

struct FailedCommand: View {
    var onOutput: () -> Void = {}
    @Environment(\.palette) private var p

    var body: some View {
        Button(action: onOutput) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Image(systemName: "terminal").foregroundStyle(p.failure)
                VStack(alignment: .leading, spacing: 5) {
                    Text("swift test --filter TitleViewTests").font(.subheadline.monospaced().weight(.medium))
                        .fixedSize(horizontal: false, vertical: true)
                    Text("Exit 1 · View output").font(.caption).foregroundStyle(p.failure)
                }
                Spacer(minLength: 0)
                Image(systemName: "chevron.right").font(.caption2.weight(.semibold)).foregroundStyle(p.muted)
            }
            .foregroundStyle(p.ink)
            .padding(12)
            .background(p.inset, in: .rect(cornerRadius: 10))
        }
        .buttonStyle(.plain)
    }
}

struct FailureCard: View {
    var output: () -> Void = {}
    @Environment(\.palette) private var p

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 8) {
                Image(systemName: "exclamationmark.octagon.fill").foregroundStyle(p.failure)
                Text("1 command failed").font(.subheadline.weight(.semibold)).foregroundStyle(p.failure)
            }
            Text("Title fixture not found")
                .font(.title2.weight(.semibold))
                .fixedSize(horizontal: false, vertical: true)
            Text("The title tests couldn’t open blank-title.json. Your changes are saved.")
                .font(.body).foregroundStyle(p.muted)
                .fixedSize(horizontal: false, vertical: true)
            FailedCommand(onOutput: output)
            ActionPill(title: "Continue", icon: "arrow.right")
        }
        .padding(18)
        .panel()
        .overlay(alignment: .leading) {
            UnevenRoundedRectangle(topLeadingRadius: 2, bottomLeadingRadius: 2)
                .fill(p.failure).frame(width: 3).padding(.vertical, 17)
        }
    }
}

struct OutputDetail: View {
    @Environment(\.palette) private var p

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Command · exit 1").font(.subheadline.weight(.semibold)).foregroundStyle(p.muted)
            Text("swift test --filter TitleViewTests").font(.headline.monospaced())
                .fixedSize(horizontal: false, vertical: true)
            VStack(alignment: .leading, spacing: 9) {
                Label("Standard error", systemImage: "exclamationmark.circle")
                    .font(.subheadline.weight(.semibold))
                Text("error: blank-title.json: No such file or directory")
                    .font(.callout.monospaced())
                    .fixedSize(horizontal: false, vertical: true)
            }
            .foregroundStyle(Color(hex: 0xFFB3A3))
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(14)
            .background(Color(hex: 0x3A2522), in: .rect(cornerRadius: 12))
            HStack {
                Text("Standard output").font(.subheadline.weight(.semibold))
                Spacer()
                Text("No output").font(.caption).foregroundStyle(p.muted)
            }
        }
        .padding(16)
        .panel()
    }
}

struct DiffDetail: View {
    var titleSample = false
    @Environment(\.palette) private var p

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text(titleSample ? "TitleView.swift" : "cache.swift").font(.headline.monospaced())
                Spacer()
                Text(titleSample ? "+1" : "+3").foregroundStyle(p.success)
                Text("−1").foregroundStyle(p.failure)
            }
            .font(.subheadline.monospacedDigit().weight(.medium))
            HStack(spacing: 8) {
                Text("Old").frame(width: 30, alignment: .trailing)
                Text("New").frame(width: 30, alignment: .trailing)
                Text(titleSample ? "@@ −24,2 +24,2 @@" : "@@ −38,3 +38,5 @@").foregroundStyle(p.muted)
            }
            .font(.caption)
            .padding(.horizontal, 8)
            VStack(alignment: .leading, spacing: 1) {
                if titleSample {
                    row("24", "", "− let title = item.title!", change: -1)
                    row("", "24", "+ let title = item.title ?? \"Untitled\"", change: 1)
                    row("25", "25", "  Text(title)", change: 0)
                } else {
                    row("38", "38", "  func resolve() {", change: 0)
                    row("39", "", "−   let ttl = 30", change: -1)
                    row("", "39", "+   let ttl = 120", change: 1)
                    row("", "40", "+   let key = cacheKey()", change: 1)
                    row("", "41", "+   cache.store(key, ttl: ttl)", change: 1)
                    row("40", "42", "  }", change: 0)
                }
            }
        }
        .padding(14)
        .panel()
    }

    private func row(_ before: String, _ after: String, _ code: String, change: Int) -> some View {
        let ink = change == 0 ? p.ink : change == 1 ? p.success : p.failure
        return HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(before).frame(width: 30, alignment: .trailing).foregroundStyle(p.muted)
            Text(after).frame(width: 30, alignment: .trailing).foregroundStyle(p.muted)
            Text(code).foregroundStyle(ink)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .font(.caption.monospaced())
        .padding(.vertical, 5).padding(.horizontal, 8)
        .background(ink.opacity(change == 0 ? 0 : 0.10))
    }
}

struct ConnectionBanner: View {
    var offline: Bool
    var takeover: () -> Void = {}
    @Environment(\.palette) private var p

    var body: some View {
        VStack(alignment: .leading, spacing: 9) {
            Label(offline ? "Disconnected" : "Read-only on this device",
                  systemImage: offline ? "wifi.slash" : "lock")
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(p.warning)
            Text(offline ? "Showing the last synced history. Reconnect to send." :
                 "Studio Mac is typing. Move typing control here to reply; the conversation stays open there.")
                .font(.footnote).foregroundStyle(p.muted)
                .fixedSize(horizontal: false, vertical: true)
            ActionPill(title: offline ? "Reconnect" : "Take over here", action: takeover)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14).panel()
    }
}

struct ModelPanel: View {
    @State private var customID = ""
    @State private var selected = "Opus 4.7"
    @Environment(\.palette) private var p

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Choose model").font(.title2.weight(.semibold))
            group("Anthropic", models: [("Opus 4.7", "Deep reasoning"), ("Sonnet 4.6", "Balanced")])
            group("OpenAI", models: [("GPT-5.4", "General purpose")])
            Divider()
            DisclosureGroup("Custom model ID") {
                TextField("provider/model", text: $customID)
                    .font(.subheadline.monospaced()).padding(10)
                    .background(p.inset, in: .rect(cornerRadius: 8))
            }
            .font(.subheadline.weight(.medium)).tint(p.accent)
        }
        .padding(17).panel()
    }

    private func group(_ provider: String, models: [(String, String)]) -> some View {
        VStack(alignment: .leading, spacing: 7) {
            Text(provider).font(.subheadline.weight(.semibold)).foregroundStyle(p.muted)
            ForEach(models, id: \.0) { name, detail in
                Button {
                    selected = name
                } label: {
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(name).font(.subheadline.weight(.semibold)).foregroundStyle(p.ink)
                            Text(detail).font(.caption2).foregroundStyle(p.muted)
                        }
                        Spacer()
                        if selected == name {
                            Image(systemName: "checkmark.circle.fill").foregroundStyle(p.accent)
                        }
                    }
                    .padding(8)
                    .frame(minHeight: 44)
                    .background(selected == name ? p.inset : .clear, in: .rect(cornerRadius: 8))
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(selected == name ? .isSelected : [])
            }
        }
    }
}

struct ThreadInfoPanel: View {
    var title = "Gateway latency baseline"
    @Environment(\.palette) private var p

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(title).font(.title2.weight(.semibold))
            HStack(alignment: .top, spacing: 22) {
                metric("Context used", "38K / 200K")
                metric("Estimated cost", "$0.18")
            }
            Divider()
            LabeledContent("Machine", value: "Studio Mac").font(.subheadline)
            LabeledContent("Model", value: "Opus 4.7").font(.subheadline)
            Button("Switch typing to another device", systemImage: "keyboard") {}
                .font(.subheadline.weight(.semibold)).tint(p.accent)
        }
        .padding(17).panel()
    }

    private func metric(_ label: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label).font(.subheadline).foregroundStyle(p.muted)
            Text(value).font(.title3.weight(.semibold)).monospacedDigit()
        }
    }
}
