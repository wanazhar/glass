import XCTest
@testable import MarbleCore

final class MarbleMathTests: XCTestCase {
    func testBothInlineDelimitersAndReadableMath() {
        XCTAssertEqual(
            MarbleMath.inline(in: #"See $\zeta(s)$ and \(10^{13}\). $\text{Re}(s) = 1/2$; $x_{12} + \frac{a}{b}$."#),
            "See ζ(s) and 10¹³. Re(s) = ½; x₁₂ + a/b."
        )
    }

    func testCurrencyEscapesAndCodeRemainLiteral() {
        XCTAssertEqual(
            MarbleMath.inline(in: #"Costs $5 or $10. \$5. `$x^2$` and $x^2$."#),
            #"Costs $5 or $10. \$5. `$x^2$` and x²."#
        )
    }

    func testBothDisplayDelimitersAndCodeFences() {
        let markdown = #"""
        $$\frac12$$
        \[
          \alpha + x_2
        \]
        ```
        $$x^2$$
        ```
        """#
        XCTAssertEqual(MarbleMarkdown.parse(markdown), [
            .math("½"), .math("α + x₂"), .code(language: "", code: "$$x^2$$"),
        ])
    }
}
