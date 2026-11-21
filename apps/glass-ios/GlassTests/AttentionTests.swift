import SwiftUI
import Testing
import UIKit
@testable import Glass

@Suite struct AttentionTests {
    @Test func alertsOnlyWhenOnAndOffScreen() {
        #expect(Attention.shouldNotify(enabled: true, appState: .background))
        #expect(Attention.shouldNotify(enabled: true, appState: .inactive))
        #expect(!Attention.shouldNotify(enabled: true, appState: .active))
        #expect(!Attention.shouldNotify(enabled: false, appState: .background))
    }

    @Test func keyCommandsDontCollide() {
        for command in KeyCommand.allCases {
            let clashes = KeyCommand.allCases.filter {
                $0 != command && $0.key == command.key && $0.modifiers == command.modifiers
                    && ($0.scope == command.scope || $0.scope == .app || command.scope == .app)
            }
            #expect(clashes.isEmpty, "\(command) clashes with \(clashes)")
        }
    }
}
