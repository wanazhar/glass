import SwiftUI

/// Appearance preferences stored on this iPhone, matching the web UI's theme, animation and tool density.
enum ThemePreference: String, CaseIterable, Identifiable {
    case system, light, dark
    static let key = "glass.theme"
    var id: String { rawValue }
    var title: String { rawValue.prefix(1).uppercased() + rawValue.dropFirst() }
    var colorScheme: ColorScheme? {
        switch self {
        case .system: nil
        case .light: .light
        case .dark: .dark
        }
    }
}

/// How the "working" indicator moves. Reduce Motion always shows it still.
enum ActivityAnimation: String, CaseIterable, Identifiable {
    case dots, pulse, breathe, none
    static let key = "glass.animation"
    var id: String { rawValue }
    var title: String {
        switch self {
        case .dots: "Spinner"
        case .pulse: "Pulse"
        case .breathe: "Breathe"
        case .none: "None"
        }
    }
}

enum ToolDensity: String, CaseIterable, Identifiable {
    case compact, expanded
    static let key = "glass.tools"
    var id: String { rawValue }
    var title: String { self == .compact ? "Compact" : "Expanded" }
}

/// The activity indicator in the conversation's bottom bar.
struct ActivityIndicator: View {
    @AppStorage(ActivityAnimation.key) private var animation: ActivityAnimation = .dots
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        switch reduceMotion ? .none : animation {
        case .dots:
            ProgressView().controlSize(.mini)
        case .pulse:
            Image(systemName: "circle.fill").font(.system(size: 7))
                .foregroundStyle(.tint)
                .symbolEffect(.pulse, options: .repeating)
        case .breathe:
            Image(systemName: "circle.fill").font(.system(size: 7))
                .foregroundStyle(.tint)
                .symbolEffect(.breathe, options: .repeating)
        case .none:
            Image(systemName: "circle.fill").font(.system(size: 7)).foregroundStyle(.tint)
        }
    }
}
