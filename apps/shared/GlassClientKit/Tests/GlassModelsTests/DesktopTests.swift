import Foundation
import GlassModels
import Testing

@Suite struct DesktopTests {
    @Test func parsesFramesAcrossChunksAndSkipsHeartbeats() throws {
        let jpeg = Data([0xFF, 0xD8, 0x01, 0x02, 0xFF, 0xD9])
        var wire = Data("--glassframe\r\nX-Glass-Heartbeat: 1\r\nContent-Length: 0\r\n\r\n\r\n".utf8)
        wire += Data("--glassframe\r\nContent-Type: image/jpeg\r\nContent-Length: \(jpeg.count)\r\nX-Frame-Seq: 7\r\nX-Frame-Time-Ms: 10\r\n\r\n".utf8)
        wire += jpeg + Data("\r\n".utf8)
        var parser = DesktopStreamParser()
        var frames: [DesktopFrame] = []
        for byte in wire { frames += try parser.feed(Data([byte])) }
        #expect(frames == [DesktopFrame(jpeg: jpeg, sequence: 7)])
    }

    @Test func rejectsMalformedStreams() {
        var parser = DesktopStreamParser()
        #expect(throws: DesktopStreamParser.Failure.malformed) { try parser.feed(Data("HTTP garbage that is long enough".utf8)) }
        var big = DesktopStreamParser()
        #expect(throws: DesktopStreamParser.Failure.tooLarge) {
            try big.feed(Data("--glassframe\r\nContent-Length: 99999999\r\n\r\n".utf8))
        }
    }

    @Test func inputClampsCoordinatesAndDeltas() {
        #expect(DesktopInput.move(x: 1.4, y: -0.2).body == .object(["action": .string("pointer-move"), "x": .number(1), "y": .number(0)]))
        #expect(DesktopInput.scroll(x: 0.5, y: 0.5, deltaX: 0, deltaY: 2000).body["deltaY"] == .number(800))
        #expect(DesktopInput.chord(["Control", "L"]).body["keys"] == .array([.string("Control"), .string("L")]))
        #expect(DesktopInput.button(x: nil, y: nil, button: .left, pressed: false).body["x"] == nil)
    }

    @Test func statusDecodesIdleAndRunningHosts() throws {
        let idle = try JSONDecoder().decode(DesktopStatus.self, from: Data("""
        {"available":false,"sessionId":null,"lease":"agent","error":null,"capabilities":[
          {"environment":"gnome","lifecycle":"reusable","label":"Reusable GNOME workspace","available":false,"reason":"GNOME is not installed","profileId":"default","profileExists":true},
          {"environment":"xfce","lifecycle":"disposable","label":"Quick XFCE desktop","available":true,"reason":null,"profileId":null,"profileExists":false}]}
        """.utf8))
        #expect(!idle.isRunning)
        #expect(idle.capabilities.map(\.isReusable) == [true, false])
        #expect(idle.controlSummary == "Glass has control")
        let running = try JSONDecoder().decode(DesktopStatus.self, from: Data("""
        {"available":true,"sessionId":"s1","environment":"xfce","lifecycle":"disposable","lease":"human","width":1280,"height":800,"frameSeq":3,"capabilities":[]}
        """.utf8))
        #expect(running.isRunning && running.youHaveControl)
    }

    @Test func validatesRegionUpdatesLikeTheWebClient() throws {
        let hashes = Array(repeating: "0123456789abcdef", count: 260)
        try DesktopUpdate(sessionId: "s", frameSeq: 1, full: "AA==", hashes: hashes).validate(session: "s", after: 0, hasBase: false)
        #expect(throws: DesktopUpdate.Invalid.self) {
            try DesktopUpdate(sessionId: "s", frameSeq: 1, full: "AA==", hashes: hashes).validate(session: "other", after: 0, hasBase: false)
        }
        let edge = DesktopUpdate.Tile(index: 259, x: 1216, y: 768, width: 64, height: 32, hash: "0123456789abcdef", data: "AA==")
        try DesktopUpdate(sessionId: "s", frameSeq: 2, tiles: [edge]).validate(session: "s", after: 1, hasBase: true)
        #expect(throws: DesktopUpdate.Invalid.self) {
            try DesktopUpdate(sessionId: "s", frameSeq: 2, tiles: [edge]).validate(session: "s", after: 1, hasBase: false)
        }
        var moved = edge
        moved.x = 0
        #expect(throws: DesktopUpdate.Invalid.self) {
            try DesktopUpdate(sessionId: "s", frameSeq: 2, tiles: [moved]).validate(session: "s", after: 1, hasBase: true)
        }
        #expect(throws: DesktopUpdate.Invalid.self) {
            try DesktopUpdate(sessionId: "s", frameSeq: 1, tiles: [edge]).validate(session: "s", after: 1, hasBase: true)
        }
    }
}
