/// The lock screen uses phases and keeps prompts, reasoning and tool arguments in the app.
enum MarbleWorkPhase: String, Codable, Hashable {
    case thinking, tool, replying, stopping
}
