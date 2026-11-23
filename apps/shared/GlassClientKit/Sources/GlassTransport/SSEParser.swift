import Foundation

/// One server-sent event frame.
public struct SSEFrame: Sendable, Hashable {
    public var id: String?
    public var data: String
    public init(id: String?, data: String) { self.id = id; self.data = data }
}

/// Incremental `text/event-stream` parser with the web client's 4 MiB frame bound.
public struct SSEParser: Sendable {
    public static let maxFrameBytes = 4 * 1024 * 1024
    private var buffer = Data()
    public private(set) var lastEventID: String?

    public init(lastEventID: String? = nil) { self.lastEventID = lastEventID }

    /// Feeds bytes and returns every completed frame. Frames without `data` (comments, bare ids) are not returned.
    public mutating func feed(_ chunk: Data) throws(GlassError) -> [SSEFrame] {
        buffer.append(chunk)
        var frames: [SSEFrame] = []
        while let (end, separator) = Self.frameEnd(in: buffer) {
            let frameData = buffer[buffer.startIndex..<end]
            buffer.removeSubrange(buffer.startIndex..<(end + separator))
            if let frame = parse(frameData) { frames.append(frame) }
        }
        if buffer.count > Self.maxFrameBytes { throw .invalidResponse("event exceeds bounds") }
        return frames
    }

    private static func frameEnd(in data: Data) -> (Int, Int)? {
        var index = data.startIndex
        while index < data.endIndex {
            if data[index] == 0x0A {
                let next = data.index(after: index)
                if next < data.endIndex, data[next] == 0x0A { return (index, 2) }
                if next < data.endIndex, data[next] == 0x0D, data.index(after: next) < data.endIndex, data[data.index(after: next)] == 0x0A {
                    return (index, 3)
                }
            }
            index = data.index(after: index)
        }
        return nil
    }

    private mutating func parse(_ raw: Data) -> SSEFrame? {
        let text = String(decoding: raw, as: UTF8.self).replacingOccurrences(of: "\r\n", with: "\n")
        var data: [String] = []
        var id: String?
        for rawLine in text.split(separator: "\n", omittingEmptySubsequences: false) {
            let line = rawLine.hasSuffix("\r") ? rawLine.dropLast() : rawLine
            if line.hasPrefix(":") { continue }
            let (field, value) = Self.split(line)
            switch field {
            case "data": data.append(value)
            case "id" where !value.contains("\0"): id = value
            default: break
            }
        }
        if let id { lastEventID = id }
        guard !data.isEmpty else { return nil }
        return SSEFrame(id: id ?? lastEventID, data: data.joined(separator: "\n"))
    }

    private static func split(_ line: Substring) -> (Substring, String) {
        guard let colon = line.firstIndex(of: ":") else { return (line, "") }
        var value = line[line.index(after: colon)...]
        if value.first == " " { value = value.dropFirst() }
        return (line[..<colon], String(value))
    }
}

/// `{epoch}:{cursor}` event id used by `/api/events`.
public struct EventCursor: Sendable, Hashable, Codable, CustomStringConvertible {
    public var epoch: UInt64
    public var cursor: UInt64
    public init(epoch: UInt64, cursor: UInt64) { self.epoch = epoch; self.cursor = cursor }
    public init?(_ raw: String) {
        let parts = raw.split(separator: ":")
        guard parts.count == 2, let e = UInt64(parts[0]), let c = UInt64(parts[1]) else { return nil }
        self.init(epoch: e, cursor: c)
    }
    public var description: String { "\(epoch):\(cursor)" }
}
