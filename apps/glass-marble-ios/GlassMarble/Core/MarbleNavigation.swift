import Foundation

/// Root-level navigation state for the iOS app: which tab is active and what
/// the Chats stack shows. Kept in Core as a pure value type so every rule —
/// "refresh never navigates", "launch lands on the list", "deleting pops
/// back" — is testable on Linux and impossible for a state refresh to violate.
public enum MarbleTab: String, Sendable, CaseIterable {
    case chats, activity, scheduled, settings
}

/// One destination inside the Chats tab's own NavigationStack.
public enum ChatRoute: Hashable, Sendable {
    /// A not-yet-created conversation. It gains a real id on the first send.
    case draft
    case conversation(String)
}

public struct MarbleNavigation: Sendable, Equatable {
    public private(set) var tab: MarbleTab = .chats
    /// The Chats stack's path; empty means the conversation list is on top.
    public internal(set) var chatPath: [ChatRoute] = []
    /// What the chat on screen should load; nil for a draft.
    public var viewingConversationId: String? {
        chatPath.last.flatMap { route in
            if case let .conversation(id) = route { return id }
            return nil
        }
    }
    /// Set by the scene when a notification launch arrives before the view
    /// exists; the Chats tab drains it on first appearance.
    public private(set) var pendingOpen: String?

    /// Cold start always lands on the chat list.
    public mutating func launch() {
        tab = .chats
        chatPath = []
    }

    public mutating func selectTab(_ newTab: MarbleTab) {
        tab = newTab
    }

    /// A list row or transcript-search hit; replaces any draft on top.
    public mutating func openConversation(_ id: String) {
        tab = .chats
        if case .draft = chatPath.last {
            chatPath[chatPath.count - 1] = .conversation(id)
        } else {
            chatPath.append(.conversation(id))
        }
    }

    /// The toolbar's compose button pushes a draft chat.
    public mutating func newChat() {
        tab = .chats
        if case .draft = chatPath.last { return }
        chatPath.append(.draft)
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
        tab = .chats
        chatPath = [.conversation(id)]
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
        tab = .chats
        chatPath = []
        pendingOpen = nil
    }
}

/// Whether the transcript should keep following new output. Once the person
/// scrolls away from the bottom the transcript must not yank them back; a
/// "Jump to latest" affordance takes over until they return near the end.
public struct MarbleScrollFollow: Sendable, Equatable {
    /// Distance from the bottom (points) that still counts as "at the end".
    public var snapThreshold: Double
    /// Minimum interval between follow-scrolls during streaming.
    public var throttleInterval: TimeInterval
    public private(set) var following = true
    private var lastFollow: TimeInterval = 0

    public init(snapThreshold: Double = 120, throttleInterval: TimeInterval = 0.15) {
        self.snapThreshold = snapThreshold
        self.throttleInterval = throttleInterval
    }

    /// The person moved the scroll position; only their movement re-arms or
    /// disarms following.
    public mutating func userMoved(distanceFromBottom: Double) {
        following = distanceFromBottom <= snapThreshold
    }

    /// New content arrived. Follows only when the person is at the end and
    /// the throttle allows it; returns whether a scroll should happen.
    public mutating func shouldFollow(now: TimeInterval) -> Bool {
        guard following, now - lastFollow >= throttleInterval else { return false }
        lastFollow = now
        return true
    }

    /// The first load jumps without animation or throttle.
    public mutating func initialFollow(now: TimeInterval) -> Bool {
        lastFollow = now
        return true
    }

    /// The "Jump to latest" button: shown when not following.
    public var showJumpButton: Bool { !following }

    /// Tapping the button re-arms following and scrolls.
    public mutating func jumpToLatest(now: TimeInterval) {
        following = true
        lastFollow = now
    }
}
