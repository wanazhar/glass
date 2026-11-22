import Foundation

/// Root-level navigation state for the iOS app: which tab is active and what
/// the Chats stack shows. Kept in Core as a pure value type so every rule —
/// "refresh never navigates", "launch lands on the list", "deleting pops
/// back" — is testable on Linux and impossible for a state refresh to violate.
public enum MarbleTab: String, Sendable, CaseIterable {
    /// The shared chat where every marble can answer; launch lands here.
    case everyone
    /// The roster: every marble, its chat, and its info.
    case marbles
    /// Connection, Marbles and models, Activity, Scheduled, and the rest.
    case settings
}

/// One destination inside a tab's own NavigationStack. The Marbles stack
/// holds marble screens and the default assistant's chats; the Everyone
/// stack only needs a marble's info card when an avatar is tapped.
public enum ChatRoute: Hashable, Sendable {
    /// The shared chat, pushed from the Everyone inbox.
    case everyoneChat
    case threadInbox(String)
    case thread(String)
    case searchHit(conversationId: String, threadId: String?, entryId: String?)
    /// A not-yet-created conversation. It gains a real id on the first send.
    case draft
    case conversation(String)
    /// One marble's dedicated chat (`marble:<id>` keeps its own engine chat).
    case marbleChat(String)
    /// The default assistant's chats list — its "chat" is every conversation.
    case marbleChats
    /// A marble's info screen: personality, look, memory, what it can do.
    case marbleInfo(String)
}

public struct MarbleNavigation: Sendable, Equatable {
    public private(set) var tab: MarbleTab = .everyone
    /// The Marbles stack's path; empty means the roster is on top.
    public internal(set) var chatPath: [ChatRoute] = []
    /// The Everyone stack's path; only avatar taps push onto it.
    public internal(set) var everyonePath: [ChatRoute] = []
    /// What the chat on screen should load; nil for a draft or marble screen.
    public var viewingConversationId: String? {
        chatPath.last.flatMap { route in
            if case let .conversation(id) = route { return id }
            return nil
        }
    }

    /// Visible chat for foreground notifications. Inactive tabs can retain
    /// their paths even while another screen is on top.
    func visibleChat(namedConversation: (String) -> String?) -> String? {
        switch activeChatRoute {
        case .everyoneChat: return everyoneConversationId
        case .marbleChat(let id): return namedConversation(id)
        case .conversation(let id), .searchHit(let id, nil, _): return id
        default: return nil
        }
    }

    var visibleThreadId: String? {
        switch activeChatRoute {
        case .thread(let id), .searchHit(_, let id?, _): return id
        default: return nil
        }
    }

    private var activeChatRoute: ChatRoute? {
        switch tab {
        case .everyone: return everyonePath.last
        case .marbles: return chatPath.last
        case .settings: return nil
        }
    }
    /// Set by the scene when a notification launch arrives before the view
    /// exists; the Chats tab drains it on first appearance.
    public private(set) var pendingOpen: String?

    /// Cold start always lands on Everyone.
    public mutating func launch() {
        tab = .everyone
        chatPath = []
        everyonePath = []
    }

    public mutating func selectTab(_ newTab: MarbleTab) {
        tab = newTab
    }

    mutating func openThread(_ id: String) {
        if tab == .everyone {
            everyonePath.append(.thread(id))
        } else {
            chatPath.append(.thread(id))
        }
    }

    /// A list row or transcript-search hit; replaces any draft on top.
    public mutating func openConversation(_ id: String) {
        tab = .marbles
        if case .draft = chatPath.last {
            chatPath[chatPath.count - 1] = .conversation(id)
        } else {
            chatPath.append(.conversation(id))
        }
    }

    /// The toolbar's compose button pushes a draft chat.
    public mutating func newChat() {
        tab = .marbles
        if case .draft = chatPath.last { return }
        chatPath.append(.draft)
    }

    /// A roster row: the default assistant opens its chats list, a named
    /// marble opens its own chat.
    public mutating func openMarble(_ id: String) {
        tab = .marbles
        chatPath.append(id == "marble" ? .marbleChats : .marbleChat(id))
    }

    /// A marble's info screen, from the roster or an Everyone avatar.
    public mutating func openMarbleInfo(_ id: String, fromEveryone: Bool = false) {
        if fromEveryone {
            everyonePath.append(.marbleInfo(id))
        } else {
            tab = .marbles
            chatPath.append(.marbleInfo(id))
        }
    }

    /// The first send resolved the draft into a real conversation.
    public mutating func draftResolved(_ id: String) {
        guard case .draft = chatPath.last else { return }
        chatPath[chatPath.count - 1] = .conversation(id)
    }

    public mutating func back() {
        if !chatPath.isEmpty { chatPath.removeLast() }
    }

    /// Deleting the chat on screen pops back to the list.
    public mutating func deleted(_ id: String) {
        chatPath.removeAll { route in
            if case let .conversation(existing) = route { return existing == id }
            return false
        }
    }

    /// A notification tap takes the person straight to that chat.
    public mutating func notificationTapped(_ id: String) {
        if id == everyoneConversationId {
            tab = .everyone
            everyonePath = [.everyoneChat]
            return
        }
        tab = .marbles
        chatPath = [.marbleChats, .conversation(id)]
    }

    /// A launch-time notification target, queued until the view exists.
    public mutating func queuePendingOpen(_ id: String) {
        pendingOpen = id
    }

    public mutating func drainPendingOpen() -> String? {
        defer { pendingOpen = nil }
        return pendingOpen
    }

    /// State refreshes are data, not navigation. Explicitly a no-op so a poll
    /// can never re-push a chat the person just left.
    public mutating func refreshed() {}

    public mutating func signOut() {
        tab = .everyone
        chatPath = []
        everyonePath = []
        pendingOpen = nil
    }
}

/// Whether the transcript should keep following new output. Once the person
/// scrolls away from the bottom the transcript must not yank them back; a
/// "Jump to latest" affordance takes over until they return near the end.
public struct MarbleScrollFollow: Sendable, Equatable {
    /// Distance from the bottom (points) that still counts as "at the end".
    public var snapThreshold: Double
    public private(set) var following = true

    public init(snapThreshold: Double = 120) {
        self.snapThreshold = snapThreshold
    }

    /// The person moved the scroll position; only their movement re-arms or
    /// disarms following.
    public mutating func userMoved(distanceFromBottom: Double) {
        following = distanceFromBottom <= snapThreshold
    }

    /// The "Jump to latest" button: shown when not following.
    public var showJumpButton: Bool { !following }

    mutating func pauseForSearchJump() { following = false }

    /// Tapping the button re-arms following and scrolls.
    public mutating func jumpToLatest() {
        following = true
    }
}
