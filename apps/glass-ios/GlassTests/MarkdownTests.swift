import Testing
@testable import Glass

@Test func commandOutputSeparatesFailureAndStderr() {
    let failed = CommandActivity(command: "swift test",
                                 output: "[exit 1]\nExecuted 8 tests\nstderr:\nExpected: page-004",
                                 duration: .milliseconds(1360), failed: false)
    #expect(failed.exitCode == 1)
    #expect(failed.failed)
    #expect(failed.duration == .milliseconds(1360))
    #expect(failed.stdout == "Executed 8 tests")
    #expect(failed.stderr == "Expected: page-004")
}

@MainActor @Suite struct MarkdownTests {
    @Test func truncatedPreviewsDoNotRenderIncompleteCode() {
        for (fence, content) in [("```", "let first = 1"), ("~~~", "let first = 1"), ("````", "```\n````suffix")] {
            let partial = "Summary.\n\n\(fence)swift\n\(content)\n\nlet second"
            #expect(MarkdownView.blocks(partial, completeFencesOnly: true) == [.prose("Summary.")])
            #expect(MarkdownView.blocks(partial + " = 2\n\(fence)", completeFencesOnly: true) == [
                .prose("Summary."), .code("\(content)\n\nlet second = 2", language: "swift")
            ])
        }
    }
    @Test func tablesBecomeTablesWithAlignment() {
        let blocks = MarkdownView.blocks("| Name | Role | Status |\n|------|:----:|-------:|\n| Alice | Admin | Active |\n| Bob | `a|b` | Off |")
        #expect(blocks == [.table(header: ["Name", "Role", "Status"], alignments: [.leading, .center, .trailing],
                                  rows: [["Alice", "Admin", "Active"], ["Bob", "`a|b`", "Off"]])])
    }

    @Test func taskListsNestedListsAndContinuations() {
        let blocks = MarkdownView.blocks("- [x] Set up\n- [ ] Deploy\n1. First\n   - Nested\n     more words\n2. Second")
        #expect(blocks == [
            .item("Set up", marker: .task(done: true), depth: 0),
            .item("Deploy", marker: .task(done: false), depth: 0),
            .item("First", marker: .number("1."), depth: 0),
            .item("Nested more words", marker: .bullet, depth: 1),
            .item("Second", marker: .number("2."), depth: 0),
        ])
    }

    @Test func wideTableStaysBetweenTheSurroundingParagraphs() {
        let blocks = MarkdownView.blocks("""
            Before the table.

            | Capability | Status | Notes | Owner |
            | --- | :--- | :---: | ---: |
            | Compile SwiftiOS from source | Includes a long description that needs to wrap. | A \\| B | Glass |
            | Sign the build | Pending |

            After the table.
            """)
        #expect(blocks == [
            .prose("Before the table."),
            .table(header: ["Capability", "Status", "Notes", "Owner"],
                   alignments: [.leading, .leading, .center, .trailing],
                   rows: [["Compile SwiftiOS from source", "Includes a long description that needs to wrap.", "A | B", "Glass"],
                          ["Sign the build", "Pending", "", ""]]),
            .prose("After the table."),
        ])
    }

    @Test func quotesNestAndDetailsCollapse() {
        #expect(MarkdownView.blocks("> One\n> > Two") == [.quote("One\n> Two")])
        #expect(MarkdownView.blocks("> One\n> > Two").count == 1)
        let details = MarkdownView.blocks("<details>\n<summary>Full report</summary>\n\n| A | B |\n|---|---|\n| 1 | 2 |\n</details>\nAfter")
        #expect(details == [.details(summary: "Full report", body: "| A | B |\n|---|---|\n| 1 | 2 |"), .prose("After")])
    }

    @Test func imagesAndMath() {
        #expect(MarkdownView.blocks("![Image](sample.png)") == [.image(alt: "Image", url: "sample.png")])
        #expect(MarkdownView.stripImages("See ![Image](a.png) here") == "See  here")
        #expect(MarkdownView.blocks("$$E = mc^2$$") == [.math("E = mc^2")])
        #expect(MarkdownView.blocks("\\[\na+b\n\\]") == [.math("a+b")])
    }

    @Test func fencesCloseInCRLFDocuments() {
        let text = "```swift\r\nlet x = 1\r\n```\r\n\r\nAfter the fence."
        let expected: [MarkdownView.Block] = [.code("let x = 1", language: "swift"), .prose("After the fence.")]
        #expect(MarkdownView.blocks(text) == expected)
        #expect(MarkdownView.blocks(text, completeFencesOnly: true) == expected)
    }

    @Test func inlineMathIsFoundButMoneyIsNot() {
        #expect(InlineMath.split("costs $5 and $10 today") == [.text("costs $5 and $10 today")])
        #expect(InlineMath.split("so $x^2$ grows") == [.text("so "), .math("x^2"), .text(" grows")])
        #expect(InlineMath.split("and \\(a+b\\).") == [.text("and "), .math("a+b"), .text(".")])
        #expect(InlineMath.split("`$x$` stays code") == [.text("`$x$` stays code")])
    }

    @Test func inlineMathRendersAsUnicode() {
        #expect(InlineMath.render("\\sum_{i=1}^{n} i = \\frac{n(n+1)}{2}") == "∑ᵢ₌₁ⁿ i = n(n+1)⁄2")
        #expect(InlineMath.render("E = mc^2") == "E = mc²")
        #expect(InlineMath.render("\\alpha \\leq \\sqrt{x}") == "α ≤ √x")
        #expect(InlineMath.render("\\mathbb{R}^n") == "ℝⁿ")
    }

    @Test func nativeMathTypesetsFractionsAndRejectsInvalidCommands() throws {
        let fraction = #"\frac{1}{1+\sqrt{x^2}}"#
        let inline = try #require(NativeMath.image(fraction, size: 17, display: false))
        let display = try #require(NativeMath.image(fraction, size: 21, display: true))
        #expect((inline.baselineOffsetFromBottom ?? 0) > 0)
        #expect(display.size.height > inline.size.height)
        #expect(NativeMath.image(#"\unsupportedglassequation{x}"#, size: 17, display: false) == nil)
    }

    @Test func nativeMathRejectsUnsafeTextColor() {
        let source = #"a + \textcolor{red}{\frac{1}{2}}"#
        #expect(NativeMath.image(source, size: 17, display: false) == nil)
        #expect(NativeMath.image(source, size: 21, display: true) == nil)
    }

    @Test func nativeMathPreservesUnicodeRelationsAndPrimes() throws {
        for (source, latex) in [
            ("x ≤ 1", #"x \le  1"#),
            ("x ≥ 1 ≠ y", #"x \ge  1 \ne  y"#),
            ("f'(x)", #"f^{\prime}(x)"#),
            ("f″(x)", #"f^{\prime\prime}(x)"#)
        ] {
            #expect(NativeMath.normalizedLaTeX(source) == latex)
            _ = try #require(NativeMath.image(source, size: 17, display: false))
        }
        #expect(NativeMath.image("x 🜁 1", size: 17, display: false) == nil)
    }

    @Test func nativeMathPreservesOpaqueColorBoxes() throws {
        let source = #"a + \colorbox{#3F8066}{x^2}"#
        for display in [false, true] {
            let light = try #require(NativeMath.image(source, size: 21, display: display))
            let dark = try #require(NativeMath.image(source, size: 21, display: display, colorScheme: .dark))
            #expect(light.renderingMode == .alwaysOriginal)
            #expect(dark.renderingMode == .alwaysOriginal)
            #expect(light.pngData() != dark.pngData())
            #expect(NativeMath.image(#"\color{#FF0000}{x}"#, size: 21, display: display)?.renderingMode == .alwaysOriginal)
            #expect(NativeMath.image("x^2", size: 21, display: display)?.renderingMode == .alwaysTemplate)
        }
    }

    @Test func nativeMathAdaptsExplicitBlackAndWhiteInk() throws {
        let black = #"\color{#000000}{x+1}"#
        for display in [false, true] {
            let light = try #require(NativeMath.image(black, size: 21, display: display)?.pngData())
            let dark = try #require(NativeMath.image(black, size: 21, display: display, colorScheme: .dark)?.pngData())
            #expect(light != dark)
            for source in [#"\color{black}{x+1}"#, #"\color{white}{x+1}"#, #"\color{#ffffff}{x+1}"#] {
                #expect(NativeMath.image(source, size: 21, display: display)?.pngData() == light)
                #expect(NativeMath.image(source, size: 21, display: display, colorScheme: .dark)?.pngData() == dark)
            }
            let red = #"\color{#FF0000}{x+1}"#
            let redLight = try #require(NativeMath.image(red, size: 21, display: display)?.pngData())
            #expect(NativeMath.image(red, size: 21, display: display, colorScheme: .dark)?.pngData() == redLight)
            #expect(redLight != light && redLight != dark)
        }
    }

    @Test func pieSelectionUsesCumulativeValues() {
        let points = [
            ChartView.Point(id: 1, series: "", x: "A", xValue: nil, y: 1.25),
            ChartView.Point(id: 2, series: "", x: "B", xValue: nil, y: 2.75)
        ]
        #expect(ChartView.slice(at: 0, in: points)?.y == 1.25)
        #expect(ChartView.slice(at: 1.25, in: points)?.y == 2.75)
        #expect(ChartView.slice(at: 4, in: points)?.id == 2)
        #expect(ChartView.slice(at: 4.01, in: points) == nil)
    }
}
