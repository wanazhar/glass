import SwiftUI
import Testing
import UIKit
@testable import Glass

@Suite struct AttentionTests {
    @Test func alertsOnlyWhenOnAndNotLookingAtThatConversation() {
        #expect(Attention.shouldNotify(enabled: true, appState: .background, looking: true))
        #expect(Attention.shouldNotify(enabled: true, appState: .inactive, looking: true))
        #expect(Attention.shouldNotify(enabled: true, appState: .active, looking: false))
        #expect(!Attention.shouldNotify(enabled: true, appState: .active, looking: true))
        #expect(!Attention.shouldNotify(enabled: false, appState: .background, looking: false))
    }

    @Test func ledgerFiresOncePerEventAndThread() {
        let store = UserDefaults(suiteName: "glass.tests.ledger.\(UUID().uuidString)")!
        let ledger = NotifiedLedger(store: store, key: "notified")
        #expect(ledger.record("7:40", for: "a"))
        #expect(!ledger.record("7:40", for: "a"))
        #expect(!ledger.record("7:12", for: "a"))
        #expect(ledger.record("7:41", for: "a"))
        #expect(ledger.record("7:12", for: "b"))
        #expect(ledger.record("8:1", for: "a"), "a host restart starts a new epoch")
        let reopened = NotifiedLedger(store: store, key: "notified")
        #expect(reopened.last(for: "a") == "8:1")
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
