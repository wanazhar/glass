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
}
