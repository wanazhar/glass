import SwiftMath
import SwiftUI

@MainActor
enum NativeMath {
    private static let cache: NSCache<NSString, UIImage> = {
        let cache = NSCache<NSString, UIImage>()
        cache.totalCostLimit = 8 * 1024 * 1024
        return cache
    }()

    static func image(_ source: String, size: CGFloat, display: Bool) -> UIImage? {
        guard !source.isEmpty, source.utf8.count <= 2_048 else { return nil }
        guard !source.matches(of: /\\(\p{L}+|[^\p{L}])/).contains(where: { $0.1 == "textcolor" }) else { return nil }
        let key = "\(size)|\(display)|\(source)" as NSString
        if let cached = cache.object(forKey: key) { return cached }
        var math = MathImage(latex: source, fontSize: size, textColor: .black,
                             labelMode: display ? .display : .text)
        let (error, output, layout) = math.asImage()
        guard error == nil, let output, let layout, let pixels = output.cgImage else { return nil }
        let image = output.withBaselineOffset(fromBottom: layout.descent)
        cache.setObject(image, forKey: key, cost: pixels.bytesPerRow * pixels.height)
        return image
    }
}

struct DisplayMath: View {
    var source: String
    @ScaledMetric(relativeTo: .title3) private var size = 21.0

    var body: some View {
        if let image = NativeMath.image(source, size: size, display: true) {
            ScrollView(.horizontal) {
                Image(uiImage: image).renderingMode(.template)
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
