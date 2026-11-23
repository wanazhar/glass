import Foundation
import GlassTransport
import Testing

@Suite struct SSEParserTests {
    @Test func framesSplitAcrossChunksAreReassembled() throws {
        var parser = SSEParser()
        #expect(try parser.feed(Data("id: 7:1\nda".utf8)).isEmpty)
        #expect(try parser.feed(Data("ta: {\"cursor\":1}\n".utf8)).isEmpty)
        let frames = try parser.feed(Data("\nid: 7:2\ndata: {}\n\n".utf8))
        #expect(frames == [SSEFrame(id: "7:1", data: #"{"cursor":1}"#), SSEFrame(id: "7:2", data: "{}")])
        #expect(parser.lastEventID == "7:2")
    }

    @Test func crlfCommentsAndMultilineDataAreHandled() throws {
        var parser = SSEParser()
        let frames = try parser.feed(Data(": keepalive\r\n\r\nid: 1:3\r\ndata: a\r\ndata:b\r\n\r\n".utf8))
        #expect(frames == [SSEFrame(id: "1:3", data: "a\nb")])
    }

    @Test func idWithoutDataUpdatesTheResumeCursor() throws {
        var parser = SSEParser(lastEventID: "1:1")
        #expect(try parser.feed(Data("id: 1:9\n\ndata: x\n\n".utf8)) == [SSEFrame(id: "1:9", data: "x")])
    }

    @Test func oversizedFramesAreRejected() throws {
        var parser = SSEParser()
        let big = Data(("data: " + String(repeating: "x", count: SSEParser.maxFrameBytes)).utf8)
        #expect(throws: GlassError.invalidResponse("event exceeds bounds")) { try parser.feed(big) }
    }

    @Test func eventCursorRoundTrips() {
        #expect(EventCursor("17:42") == EventCursor(epoch: 17, cursor: 42))
        #expect(EventCursor("17:42")?.description == "17:42")
        #expect(EventCursor("nope") == nil)
        #expect(EventCursor("1:2:3") == nil)
    }
}
