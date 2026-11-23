import Foundation

/// Plain-language rendering and parsing of the host's routine schedule specs
/// (`every:<N>m`, `every:<N>h`, `daily:HH:MM`), so the iPhone shows "Every 2
/// hours" and never shows the wire format.
public enum MarbleSchedule: Equatable, Sendable {
    case everyMinutes(Int)
    case everyHours(Int)
    case daily(hour: Int, minute: Int)

    /// Parse a host spec; nil when it is some other valid-but-unmapped form.
    public init?(spec: String) {
        let spec = spec.trimmingCharacters(in: .whitespacesAndNewlines)
        if spec.hasPrefix("every:") {
            let rest = String(spec.dropFirst(6))
            if rest.hasSuffix("m"), let n = Int(rest.dropLast()) {
                self = .everyMinutes(n); return
            }
            if rest.hasSuffix("h"), let n = Int(rest.dropLast()) {
                self = .everyHours(n); return
            }
            return nil
        }
        if spec.hasPrefix("daily:") {
            let parts = spec.dropFirst(6).split(separator: ":")
            guard parts.count == 2,
                  let hour = Int(parts[0]), let minute = Int(parts[1]),
                  (0..<24).contains(hour), (0..<60).contains(minute) else { return nil }
            self = .daily(hour: hour, minute: minute); return
        }
        return nil
    }

    /// The wire spec the host understands.
    public var spec: String {
        switch self {
        case .everyMinutes(let n): return "every:\(n)m"
        case .everyHours(let n): return "every:\(n)h"
        case .daily(let hour, let minute):
            return String(format: "daily:%02d:%02d", hour, minute)
        }
    }

    /// What the person reads in the Scheduled list.
    public var summary: String {
        switch self {
        case .everyMinutes(let n):
            return n == 1 ? "Every minute" : "Every \(n) minutes"
        case .everyHours(let n):
            return n == 1 ? "Every hour" : "Every \(n) hours"
        case .daily(let hour, let minute):
            return String(format: "Every day at %d:%02d", hour, minute)
        }
    }
}
