import SwiftUI
import Testing
import UIKit
@testable import Glass

@MainActor @Suite struct CommandShortcutTests {
    @Test func composerRegistersCommandK() {
        let view = ComposerTextView()
        #expect(!(view.keyCommands ?? []).contains { $0.modifierFlags == .command })
        var opened = 0
        view.onCommands = { opened += 1 }
        let shortcuts = (view.keyCommands ?? []).filter { $0.modifierFlags == .command }.compactMap(\.input)
        #expect(Set(shortcuts) == ["k", "/"])
        view.onCommands?()
        #expect(opened == 1)
    }

    @Test func appearancePreferencesMatchTheWebValues() {
        #expect(ThemePreference.allCases.map(\.rawValue) == ["system", "light", "dark"])
        #expect(ThemePreference.system.colorScheme == nil)
        #expect(ThemePreference.dark.colorScheme == .dark)
        #expect(ActivityAnimation.allCases.map(\.rawValue) == ["dots", "pulse", "breathe", "none"])
        #expect(ToolDensity.allCases.map(\.rawValue) == ["compact", "expanded"])
        #expect([ThemePreference.key, ActivityAnimation.key, ToolDensity.key] == ["glass.theme", "glass.animation", "glass.tools"])
    }

    @Test func localCommandsCoverTheWebActionsThatHaveANativeHome() {
        for name in ["help", "diagnostics", "clear", "tools", "editor", "animation", "theme", "settings", "conversations", "search"] {
            #expect(LocalCommand(rawValue: name) != nil, "\(name)")
        }
    }
}
