import SwiftUI

/// Hardware-keyboard shortcuts, in one table so screens that share a window can't collide.
enum KeyCommand: CaseIterable {
    case newConversation, searchConversations, commands, commandsAlternate, stop, settings

    enum Scope { case conversations, conversation, app }

    var key: Character {
        switch self {
        case .newConversation: "n"
        case .searchConversations, .commands: "k"
        case .commandsAlternate: "/"
        case .stop: "."
        case .settings: ","
        }
    }

    var modifiers: EventModifiers { .command }

    /// Shortcuts in the same scope are live at the same time; the conversation list and a conversation are not.
    var scope: Scope {
        switch self {
        case .newConversation, .searchConversations: .conversations
        case .commands, .commandsAlternate, .stop: .conversation
        case .settings: .app
        }
    }

    var shortcut: KeyboardShortcut { KeyboardShortcut(KeyEquivalent(key), modifiers: modifiers) }
}

extension View {
    func keyCommand(_ command: KeyCommand) -> some View { keyboardShortcut(command.shortcut) }
}
