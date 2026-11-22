import Foundation

/// Mid-turn corrections. The host only steers ordinary chats; a named
/// Marble's turn runs on its own engine, which takes no corrections, so the
/// composer offers Stop alone there and says why.
enum MarbleSteer {
    enum Action: Equatable {
        /// Nothing is running: the arrow starts a new turn.
        case send
        /// A turn is running and the host can take a correction.
        case steer
        /// A turn is running and corrections are not possible: Stop only.
        case stopOnly
    }

    static let unsupportedNote = "Corrections aren't supported for Marbles yet — Stop and resend."

    static func action(isWorking: Bool, canSteer: Bool) -> Action {
        guard isWorking else { return .send }
        return canSteer ? .steer : .stopOnly
    }

    /// The draft after a correction was sent: cleared only once the host
    /// accepted it, and kept if the person typed more in the meantime.
    static func draft(after sent: String, current: String, accepted: Bool) -> String {
        accepted && current == sent ? "" : current
    }
}
