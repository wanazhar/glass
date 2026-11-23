import Foundation

/// Readable Unicode for common LaTeX in chat. Keep unsupported commands and
/// script values legible instead of dropping parts of an equation.
enum MarbleMath {
    private static let inlineSpans = try! NSRegularExpression(pattern: #"""
        (?<![\\`])(`+)(?!`)[\s\S]*?(?<!`)\1(?!`)|\\\\|\\\$|\\\((.+?)\\\)|(?<!\$)\$(?![\s$])([^$\n]+?)(?<![\s\\])\$(?![\p{L}\p{N}$])
        """#)

    static func inline(in markdown: String) -> String {
        let source = markdown as NSString
        var output = markdown
        for match in inlineSpans.matches(in: markdown, range: NSRange(location: 0, length: source.length)).reversed() {
            let math = match.range(at: 2).location == NSNotFound ? match.range(at: 3) : match.range(at: 2)
            guard math.location != NSNotFound, let range = Range(match.range, in: output) else { continue }
            output.replaceSubrange(range, with: readable(source.substring(with: math)))
        }
        return output
    }

    private static let tokens = try! NSRegularExpression(pattern: #"\\(?:text|mathrm|operatorname)\s*\{([^{}]*)\}|\\(?:[dt]?frac)\s*(?:\{([^{}]+)\}|([\p{L}\p{N}]))\s*(?:\{([^{}]+)\}|([\p{L}\p{N}]))|([_^])(?:\{([^{}]+)\}|([^\s{}\\]))|\\([A-Za-z]+)|(?<![\d/])([123]/[234])(?![\d/])"#)
    private static let commands: [String: String] = [
        "alpha": "α", "beta": "β", "gamma": "γ", "delta": "δ", "epsilon": "ε",
        "varepsilon": "ε", "zeta": "ζ", "eta": "η", "theta": "θ", "vartheta": "ϑ",
        "iota": "ι", "kappa": "κ", "lambda": "λ", "mu": "μ", "nu": "ν", "xi": "ξ",
        "pi": "π", "rho": "ρ", "sigma": "σ", "tau": "τ", "upsilon": "υ",
        "phi": "φ", "varphi": "ϕ", "chi": "χ", "psi": "ψ", "omega": "ω",
        "Gamma": "Γ", "Delta": "Δ", "Theta": "Θ", "Lambda": "Λ", "Xi": "Ξ",
        "Pi": "Π", "Sigma": "Σ", "Phi": "Φ", "Psi": "Ψ", "Omega": "Ω",
        "times": "×", "cdot": "·", "pm": "±", "le": "≤", "leq": "≤",
        "ge": "≥", "geq": "≥", "ne": "≠", "neq": "≠", "approx": "≈",
        "infty": "∞", "sum": "∑", "int": "∫", "to": "→", "in": "∈",
        "sqrt": "√", "Re": "Re", "Im": "Im", "left": "", "right": "",
    ]
    private static let superscripts = Dictionary(uniqueKeysWithValues: zip("0123456789+-=()in", "⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁼⁽⁾ⁱⁿ"))
    private static let subscripts = Dictionary(uniqueKeysWithValues: zip("0123456789+-=()aeoxhklmnpst", "₀₁₂₃₄₅₆₇₈₉₊₋₌₍₎ₐₑₒₓₕₖₗₘₙₚₛₜ"))
    private static let fractions = ["1/2": "½", "1/3": "⅓", "2/3": "⅔", "1/4": "¼", "3/4": "¾"]

    static func readable(_ source: String) -> String {
        readable(source, depth: 0)
    }

    private static func readable(_ source: String, depth: Int) -> String {
        guard depth < 8 else { return source }
        let input = source as NSString
        var output = source
        for match in tokens.matches(in: source, range: NSRange(location: 0, length: input.length)).reversed() {
            func group(_ index: Int) -> String? {
                let range = match.range(at: index)
                return range.location == NSNotFound ? nil : input.substring(with: range)
            }
            let replacement: String
            if let label = group(1) {
                replacement = label
            } else if let numerator = group(2) ?? group(3), let denominator = group(4) ?? group(5) {
                let top = readable(numerator, depth: depth + 1)
                let bottom = readable(denominator, depth: depth + 1)
                replacement = fractions["\(top)/\(bottom)"] ?? "\(fractionTerm(top))/\(fractionTerm(bottom))"
            } else if let marker = group(6), let value = group(7) ?? group(8) {
                let text = readable(value, depth: depth + 1)
                let script = marker == "^" ? superscripts : subscripts
                let glyphs = text.compactMap { script[$0] }
                replacement = glyphs.count == text.count ? String(glyphs) : "\(marker)(\(text))"
            } else if let command = group(9), let symbol = commands[command] {
                replacement = symbol
            } else if let fraction = group(10), let symbol = fractions[fraction] {
                replacement = symbol
            } else {
                continue
            }
            if let range = Range(match.range, in: output) { output.replaceSubrange(range, with: replacement) }
        }
        let trimmed = output.trimmingCharacters(in: .whitespacesAndNewlines)
        return fractions[trimmed] ?? trimmed
    }

    private static func fractionTerm(_ text: String) -> String {
        text.allSatisfy { $0.isLetter || $0.isNumber } ? text : "(\(text))"
    }
}
