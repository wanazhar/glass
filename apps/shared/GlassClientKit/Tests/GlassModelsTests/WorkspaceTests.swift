import Foundation
import Testing
@testable import GlassModels

@Suite struct WorkspaceTests {
    @Test func diffSplitsIntoHunksThatApplyAlone() {
        let diff = """
        diff --git a/a.txt b/a.txt
        index 1..2 100644
        --- a/a.txt
        +++ b/a.txt
        @@ -1,2 +1,2 @@
        -one
        +uno
         two
        @@ -9 +9 @@
        -nine
        +nueve
        diff --git a/b.txt b/b.txt
        --- a/b.txt
        +++ b/b.txt
        @@ -1 +1 @@
        -x
        +y

        """
        let changes = WorkspaceChanges(files: [], diff: diff)
        let hunks = changes.hunks(for: "a.txt")
        #expect(hunks.count == 2)
        #expect(hunks[1].patch.hasPrefix("diff --git a/a.txt b/a.txt\nindex 1..2 100644\n--- a/a.txt\n+++ b/a.txt\n@@ -9 +9 @@"))
        #expect(hunks[1].patch.hasSuffix("+nueve\n"))
        #expect(changes.hunks(for: "b.txt").map(\.lines) == [["@@ -1 +1 @@", "-x", "+y"]])
    }

    @Test func changedFilesReadAsPlainWords() throws {
        let json = #"{"files":[{"path":"a","index":"M","worktree":" "},{"path":"b","index":"?","worktree":"?"},{"path":"c","index":" ","worktree":"D"}],"diff":""}"#
        let changes = try JSONDecoder().decode(WorkspaceChanges.self, from: Data(json.utf8))
        #expect(changes.files.map(\.summary) == ["Modified", "New file", "Deleted"])
        #expect(changes.files.map(\.isStaged) == [true, false, false])
    }

    @Test func teammatesDecodeLeniently() {
        let value: JSONValue = .array([
            .object(["id": .string("t1"), "name": .string("review"), "status": .string("idle"), "terminal_events": .number(1), "prompt": .string("Check it")]),
            .object(["name": .string("no id")]),
            .object(["id": .string("t2"), "status": .string("parked")]),
        ])
        let rows = Teammate.decodeList(value)
        #expect(rows.map(\.id) == ["t1", "t2"])
        #expect(rows.map(\.statusTitle) == ["Finished", "Paused"])
        #expect(rows[0].task == "Check it")
        #expect(Teammate.decodeList(.object(["agents": value])).count == 2)
    }

    @Test func browseChildPathsJoinCleanly() {
        let page = BrowsePage(path: "/w/", entries: [.init(name: "src", kind: "directory")])
        #expect(page.child(page.entries[0]) == "/w/src")
        #expect(page.entries[0].isDirectory && !page.entries[0].isOpenable)
    }

}
