#if DEBUG
import Foundation
import GlassModels

enum LensPreview: String, Sendable {
    case splash, empty, active, working, workLog = "work-log-expanded", failure = "failed-run"
    case disconnected, readOnly = "read-only", diff, modelPicker = "model-picker", threadInfo = "thread-info"
    case drawer, settings, dynamicType = "dynamic-type-xl", ipad

    static var current: Self? {
        guard ProcessInfo.processInfo.arguments.contains("-GlassDemo") else { return nil }
        return UserDefaults.standard.string(forKey: "GlassLensPreview").flatMap(Self.init(rawValue:))
    }

    var threadID: String? {
        switch self {
        case .active, .workLog, .diff, .modelPicker, .threadInfo, .drawer, .ipad: "harbor-0"
        case .working: "lens-working"
        case .failure: "lens-failure"
        case .disconnected: "harbor-1"
        case .readOnly: "lens-readonly"
        case .dynamicType: "lens-rich"
        case .empty, .splash, .settings: nil
        }
    }

    static func firstDiff(in timeline: Timeline) -> ActivityDetail? {
        timeline.entries.compactMap { entry -> ActivityDetail? in
            guard case .tool(let id, _, _, let output, _) = entry,
                  output.hasPrefix("diff --git ") else { return nil }
            let path = output.split(separator: "\n").first?.split(separator: " ").last
            return ActivityDetail(id: id, title: path.map { (String($0) as NSString).lastPathComponent } ?? "Changes",
                                  text: output, monospaced: true, isDiff: true)
        }.first
    }
}
#endif
