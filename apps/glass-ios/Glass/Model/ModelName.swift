import Foundation

/// Readable model names for the sidebar, from provider IDs such as "deepseek/deepseek-v4.1-flash-1017-free".
enum ModelName {
    private static let brands = ["deepseek": "DeepSeek", "gpt": "GPT", "openai": "OpenAI", "claude": "Claude",
                                 "gemini": "Gemini", "gemma": "Gemma", "llama": "Llama", "qwen": "Qwen",
                                 "mistral": "Mistral", "mixtral": "Mixtral", "grok": "Grok", "glm": "GLM",
                                 "kimi": "Kimi", "phi": "Phi", "codestral": "Codestral", "minimax": "MiniMax"]
    private static let dropped: Set<String> = ["free", "latest", "preview-latest", "beta", "instruct-free"]

    static func friendly(_ id: String) -> String? {
        let trimmed = id.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        let last = trimmed.split(separator: "/").last.map(String.init) ?? trimmed
        let base = last.split(separator: ":").first.map(String.init) ?? last
        var words: [String] = []
        var previousShortNumber = false
        for raw in base.split(whereSeparator: { $0 == "-" || $0 == "_" || $0 == " " }) {
            let token = raw.lowercased()
            if dropped.contains(token) { continue }
            if token.count >= 4, token.allSatisfy(\.isNumber) { continue }
            let shortNumber = token.count <= 2 && token.allSatisfy(\.isNumber)
            if shortNumber, previousShortNumber, let last = words.popLast() {
                words.append(last + "." + token)
            } else if let brand = brands[token] {
                words.append(brand)
            } else if token.first == "v", token.dropFirst().first?.isNumber == true {
                words.append("V" + token.dropFirst())
            } else if token.first?.isNumber == true {
                words.append(token)
            } else {
                words.append(token.prefix(1).uppercased() + token.dropFirst())
            }
            previousShortNumber = shortNumber
        }
        return words.isEmpty ? nil : words.joined(separator: " ")
    }
}
