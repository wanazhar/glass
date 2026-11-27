import Testing
@testable import Glass

@Test func lensPatchUsesFileCoordinatesAndCountsOnlyHunkChanges() {
    let patch = LensPatch("""
    diff --git a/TitleView.swift b/TitleView.swift
    --- a/TitleView.swift
    +++ b/TitleView.swift
    @@ -24,2 +31,3 @@
    -old title
    +new title
    +fallback
     shared
    @@ -40 +50,0 @@
    -removed
    """)

    #expect(patch.added == 2)
    #expect(patch.removed == 2)
    #expect(patch.rows.map(\.kind) == [.hunk, .removal, .addition, .addition, .context, .hunk, .removal])
    #expect(patch.rows.map(\.old) == [nil, 24, nil, nil, 25, nil, 40])
    #expect(patch.rows.map(\.new) == [nil, nil, 31, 32, 33, nil, nil])
}
