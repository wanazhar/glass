import Foundation

/// Composer actions for mid-turn corrections.
enum MarbleSteer {
    enum Action: Equatable {
        /// Nothing is running: the arrow starts a new turn.
        case send
        /// A turn is running and the host can take a correction.
        case steer
        /// A turn is running and corrections are not possible: Stop only.
        case stopOnly
    }

    static let unsupportedNote = "Corrections aren't available for this reply — Stop and resend."

    static func action(isWorking: Bool, canSteer: Bool) -> Action {
        guard isWorking else { return .send }
        return canSteer ? .steer : .stopOnly
    }
}

enum MarbleDraft {
    /// Keep the text unless the host accepted this exact submission.
    static func draft(after sent: String, current: String, accepted: Bool) -> String {
        accepted && current == sent ? "" : current
    }
}
