import SwiftMath
import SwiftUI

@MainActor
enum NativeMath {
    private static let cache: NSCache<NSString, UIImage> = {
        let cache = NSCache<NSString, UIImage>()
        cache.totalCostLimit = 8 * 1024 * 1024
        return cache
    }()

    static func image(_ source: String, size: CGFloat, display: Bool, colorScheme: ColorScheme = .light) -> UIImage? {
        guard !source.isEmpty, source.utf8.count <= 2_048,
              let latex = normalizedLaTeX(source) else { return nil }
        let commands = latex.matches(of: /\\(\p{L}+|[^\p{L}])/).map(\.1)
        guard !commands.contains("textcolor") else { return nil }
        let usesColor = commands.contains("color") || commands.contains("colorbox")
        let lightInk = usesColor && colorScheme == .dark
        let equation = commands.contains("color") ? adaptiveColor(in: latex, lightInk: lightInk) : latex
        let key = "\(size)|\(display)|\(lightInk)|\(equation)" as NSString
        if let cached = cache.object(forKey: key) { return cached }
        var math = MathImage(latex: equation, fontSize: size, textColor: lightInk ? .white : .black,
                             labelMode: display ? .display : .text)
        let (error, output, layout) = math.asImage()
        guard error == nil, let output, let layout, let pixels = output.cgImage else { return nil }
        let image = output.withBaselineOffset(fromBottom: layout.descent)
            .withRenderingMode(usesColor ? .alwaysOriginal : .alwaysTemplate)
        cache.setObject(image, forKey: key, cost: pixels.bytesPerRow * pixels.height)
        return image
    }

    static func artwork(_ image: UIImage) -> Image {
        Image(uiImage: image).renderingMode(image.renderingMode == .alwaysOriginal ? .original : .template)
    }

    private static func adaptiveColor(in latex: String, lightInk: Bool) -> String {
        let ink = lightInk ? "#FFFFFF" : "#000000"
        return latex.replacing(/\\color\s*\{\s*((?i:black|white|#000000|#ffffff))\s*\}|\\(?:\p{L}+|[^\p{L}])/) { match in
            match.1 == nil ? String(match.0) : #"\color{"# + ink + "}"
        }
    }

    static func normalizedLaTeX(_ source: String) -> String? {
        var latex = ""
        var primes = 0
        let hasText = source.matches(of: /\\(\p{L}+|[^\p{L}])/).contains { $0.1 == "text" }
        func flushPrimes() {
            guard primes > 0 else { return }
            latex += "^{" + String(repeating: #"\prime"#, count: primes) + "}"
            primes = 0
        }
        for character in source {
            switch character {
            case "'" where !hasText: primes += 1; continue
            case "′": primes += 1; continue
            case "″": primes += 2; continue
            case "‴": primes += 3; continue
            default: flushPrimes()
            }
            switch character {
            case "≤": latex += #"\le "#
            case "≥": latex += #"\ge "#
            case "≠": latex += #"\ne "#
            case "−": latex += "-"
            case _ where character.isASCII || character.isWhitespace:
                latex += character.isASCII ? String(character) : " "
            default:
                guard character.unicodeScalars.count == 1,
                      MTMathAtomFactory.atom(forCharacter: character) != nil else { return nil }
                latex.append(character)
            }
        }
        flushPrimes()
        return latex
    }
}

struct DisplayMath: View {
    var source: String
    @Environment(\.colorScheme) private var colorScheme
    @ScaledMetric(relativeTo: .title3) private var size = 21.0

    var body: some View {
        if let image = NativeMath.image(source, size: size, display: true, colorScheme: colorScheme) {
            ScrollView(.horizontal) {
                NativeMath.artwork(image)
                    .padding(Design.gap)
                    .containerRelativeFrame(.horizontal) { width, _ in max(width, image.size.width + 2 * Design.gap) }
            }
            .scrollBounceBehavior(.basedOnSize)
            .accessibilityLabel("Equation")
            .accessibilityValue(source)
            .accessibilityIdentifier("timeline.rich.math")
        } else {
            CodeBlock(code: source, language: "LaTeX")
        }
    }
}
