import Foundation

/// `GET /api/desktop/status`: the host's managed desktop.
public struct DesktopStatus: Sendable, Hashable, Decodable {
    public var available: Bool
    public var sessionId: String?
    public var environment: String?
    public var lifecycle: String?
    public var profileId: String?
    public var lease: String
    public var width: Int?
    public var height: Int?
    public var frameSeq: Int?
    public var frameAgeMs: Int?
    public var error: String?
    public var regionUpdates: Int?
    public var capabilities: [DesktopOption]

    public init(available: Bool = false, sessionId: String? = nil, environment: String? = nil, lifecycle: String? = nil,
                lease: String = "agent", error: String? = nil, capabilities: [DesktopOption] = []) {
        self.available = available; self.sessionId = sessionId; self.environment = environment; self.lifecycle = lifecycle
        self.lease = lease; self.error = error; self.capabilities = capabilities
    }

    enum CodingKeys: String, CodingKey {
        case available, sessionId, environment, lifecycle, profileId, lease, width, height, frameSeq, frameAgeMs, error, regionUpdates, capabilities
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        available = (try? c.decode(Bool.self, forKey: .available)) ?? false
        sessionId = try? c.decodeIfPresent(String.self, forKey: .sessionId)
        environment = try? c.decodeIfPresent(String.self, forKey: .environment)
        lifecycle = try? c.decodeIfPresent(String.self, forKey: .lifecycle)
        profileId = try? c.decodeIfPresent(String.self, forKey: .profileId)
        lease = (try? c.decode(String.self, forKey: .lease)) ?? "agent"
        width = try? c.decodeIfPresent(Int.self, forKey: .width)
        height = try? c.decodeIfPresent(Int.self, forKey: .height)
        frameSeq = try? c.decodeIfPresent(Int.self, forKey: .frameSeq)
        frameAgeMs = try? c.decodeIfPresent(Int.self, forKey: .frameAgeMs)
        error = try? c.decodeIfPresent(String.self, forKey: .error)
        regionUpdates = try? c.decodeIfPresent(Int.self, forKey: .regionUpdates)
        capabilities = (try? c.decode([DesktopOption].self, forKey: .capabilities)) ?? []
    }

    public var isRunning: Bool { sessionId != nil }
    public var supportsRegionUpdates: Bool { regionUpdates == 1 }
    public var youHaveControl: Bool { lease == "human" }
    /// Plain words for who drives the desktop.
    public var controlSummary: String {
        switch lease {
        case "human": "You have control"
        case "read-only": "View only"
        default: "Glass has control"
        }
    }
    public var current: DesktopOption? { capabilities.first { $0.environment == environment && $0.lifecycle == lifecycle } }
}

/// One kind of desktop the host can start.
public struct DesktopOption: Sendable, Hashable, Decodable, Identifiable {
    public var environment: String
    public var lifecycle: String
    public var label: String
    public var available: Bool
    public var reason: String?
    public var profileId: String?
    public var profileExists: Bool

    public var id: String { environment + "/" + lifecycle }
    public var isReusable: Bool { lifecycle == "reusable" }

    public init(environment: String, lifecycle: String, label: String, available: Bool = true, reason: String? = nil,
                profileId: String? = nil, profileExists: Bool = false) {
        self.environment = environment; self.lifecycle = lifecycle; self.label = label; self.available = available
        self.reason = reason; self.profileId = profileId; self.profileExists = profileExists
    }

    enum CodingKeys: String, CodingKey { case environment, lifecycle, label, available, reason, profileId, profileExists }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        environment = try c.decode(String.self, forKey: .environment)
        lifecycle = try c.decode(String.self, forKey: .lifecycle)
        label = (try? c.decode(String.self, forKey: .label)) ?? environment
        available = (try? c.decode(Bool.self, forKey: .available)) ?? false
        reason = try? c.decodeIfPresent(String.self, forKey: .reason)
        profileId = try? c.decodeIfPresent(String.self, forKey: .profileId)
        profileExists = (try? c.decode(Bool.self, forKey: .profileExists)) ?? false
    }
}

public enum DesktopLeaseAction: String, Sendable {
    case takeControl = "take-control", returnToAgent = "return-to-agent", readOnly = "read-only"
}

/// Mouse buttons the host accepts.
public enum DesktopButton: String, Sendable, CaseIterable { case left, middle, right }

/// One input event for `POST /api/desktop/input`. Coordinates are 0…1 across the desktop.
public enum DesktopInput: Sendable, Hashable {
    case move(x: Double, y: Double)
    case click(x: Double, y: Double, button: DesktopButton)
    case button(x: Double?, y: Double?, button: DesktopButton, pressed: Bool)
    case scroll(x: Double, y: Double, deltaX: Double, deltaY: Double)
    case key(String, pressed: Bool)
    case keyTap(String)
    case chord([String])
    case text(String)

    /// Keys the host's desktop understands, beyond plain text.
    public static let keys: Set<String> = {
        var keys: Set<String> = ["Enter", "Escape", "Tab", "BackSpace", "Delete", "Home", "End", "PageUp", "PageDown",
                                 "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Shift", "Control", "Alt", "Meta", "Space"]
        for n in 1...12 { keys.insert("F\(n)") }
        return keys
    }()

    private static func clamp(_ value: Double) -> Double { min(max(value, 0), 1) }

    public var body: JSONValue {
        switch self {
        case .move(let x, let y):
            .object(["action": .string("pointer-move"), "x": .number(Self.clamp(x)), "y": .number(Self.clamp(y))])
        case .click(let x, let y, let button):
            .object(["action": .string("pointer-click"), "x": .number(Self.clamp(x)), "y": .number(Self.clamp(y)),
                     "button": .string(button.rawValue)])
        case .button(let x, let y, let button, let pressed):
            {
                var value: [String: JSONValue] = ["action": .string("pointer-button"), "button": .string(button.rawValue),
                                                  "pressed": .bool(pressed)]
                if let x, let y { value["x"] = .number(Self.clamp(x)); value["y"] = .number(Self.clamp(y)) }
                return .object(value)
            }()
        case .scroll(let x, let y, let deltaX, let deltaY):
            .object(["action": .string("scroll"), "x": .number(Self.clamp(x)), "y": .number(Self.clamp(y)),
                     "deltaX": .number(min(max(deltaX, -800), 800)), "deltaY": .number(min(max(deltaY, -800), 800))])
        case .key(let key, let pressed):
            .object(["action": .string("key"), "key": .string(key), "pressed": .bool(pressed)])
        case .keyTap(let key):
            .object(["action": .string("key-tap"), "key": .string(key)])
        case .chord(let keys):
            .object(["action": .string("key-chord"), "keys": .array(keys.map(JSONValue.string))])
        case .text(let text):
            .object(["action": .string("text"), "text": .string(text)])
        }
    }
}

/// One JPEG frame of the desktop.
public struct DesktopFrame: Sendable, Hashable {
    public var jpeg: Data
    public var sequence: Int
    /// How old the frame was when the host sent it (capture plus encode), in milliseconds.
    public var ageMs: Int?
    /// How long the host took to encode it as JPEG, in milliseconds.
    public var encodeMs: Int?
    public init(jpeg: Data, sequence: Int, ageMs: Int? = nil, encodeMs: Int? = nil) {
        self.jpeg = jpeg; self.sequence = sequence; self.ageMs = ageMs; self.encodeMs = encodeMs
    }
}

/// Splits `GET /api/desktop/stream` (`--glassframe` parts) into frames; heartbeats yield nothing.
public struct DesktopStreamParser: Sendable {
    public enum Failure: Error, Equatable { case malformed, tooLarge }
    static let boundary = Data("--glassframe\r\n".utf8)
    static let headerEnd = Data("\r\n\r\n".utf8)
    static let maxFrame = 8 * 1024 * 1024

    private var buffer = Data()
    public init() {}

    public mutating func feed(_ chunk: Data) throws(Failure) -> [DesktopFrame] {
        buffer.append(chunk)
        var frames: [DesktopFrame] = []
        while true {
            while buffer.starts(with: Data("\r\n".utf8)) { buffer.removeFirst(2) }
            guard buffer.count >= Self.boundary.count else { break }
            guard buffer.starts(with: Self.boundary) else { throw .malformed }
            let afterBoundary = buffer.index(buffer.startIndex, offsetBy: Self.boundary.count)
            guard let end = buffer.range(of: Self.headerEnd, in: afterBoundary..<buffer.endIndex) else {
                if buffer.count > 4096 { throw .malformed }
                break
            }
            guard let header = String(data: buffer[afterBoundary..<end.lowerBound], encoding: .utf8) else { throw .malformed }
            var fields: [String: String] = [:]
            for line in header.split(separator: "\r\n") {
                guard let colon = line.firstIndex(of: ":") else { throw .malformed }
                fields[line[..<colon].lowercased()] = line[line.index(after: colon)...].trimmingCharacters(in: .whitespaces)
            }
            guard let length = fields["content-length"].flatMap(Int.init), length >= 0 else { throw .malformed }
            guard length <= Self.maxFrame else { throw .tooLarge }
            let bodyStart = end.upperBound
            guard buffer.distance(from: bodyStart, to: buffer.endIndex) >= length + 2 else { break }
            let bodyEnd = buffer.index(bodyStart, offsetBy: length)
            if length > 0, fields["x-glass-heartbeat"] == nil {
                frames.append(DesktopFrame(jpeg: Data(buffer[bodyStart..<bodyEnd]), sequence: fields["x-frame-seq"].flatMap(Int.init) ?? 0,
                                           ageMs: fields["x-frame-age-ms"].flatMap(Int.init),
                                           encodeMs: fields["x-frame-encode-ms"].flatMap(Int.init)))
            }
            buffer = Data(buffer[buffer.index(bodyEnd, offsetBy: 2)...])
        }
        return frames
    }
}

/// `POST /api/desktop/updates`: a full 1280×800 frame or the 64-pixel tiles that changed since `after`.
public struct DesktopUpdate: Sendable, Hashable, Decodable {
    public struct Tile: Sendable, Hashable, Decodable {
        public var index: Int
        public var x: Int
        public var y: Int
        public var width: Int
        public var height: Int
        public var hash: String
        public var mime: String?
        public var data: String

        public init(index: Int, x: Int, y: Int, width: Int, height: Int, hash: String, mime: String? = nil, data: String) {
            self.index = index; self.x = x; self.y = y; self.width = width; self.height = height
            self.hash = hash; self.mime = mime; self.data = data
        }
    }

    public struct Invalid: Error, Equatable { public var reason: String }

    public var version: Int
    public var sessionId: String
    public var frameSeq: Int
    public var frameTimeMs: Double
    public var width: Int
    public var height: Int
    public var tileSize: Int
    public var columns: Int
    public var hashes: [String]?
    public var full: String?
    public var tiles: [Tile]

    public init(sessionId: String, frameSeq: Int, full: String? = nil, hashes: [String]? = nil, tiles: [Tile] = []) {
        version = 1; self.sessionId = sessionId; self.frameSeq = frameSeq; frameTimeMs = 0
        width = 1280; height = 800; tileSize = 64; columns = 20
        self.full = full; self.hashes = hashes; self.tiles = tiles
    }

    static let tileCount = 260

    private static func isHash(_ value: String) -> Bool {
        value.utf8.count == 16 && value.utf8.allSatisfy { (48...57).contains($0) || (97...102).contains($0) }
    }

    /// Mirrors the web client's checks so a bad update never paints over a good frame.
    public func validate(session: String, after: Int, hasBase: Bool) throws(Invalid) {
        guard version == 1, sessionId == session, width == 1280, height == 800, tileSize == 64, columns == 20,
              frameSeq > after, frameTimeMs >= 0, tiles.count <= Self.tileCount else { throw Invalid(reason: "update") }
        if full != nil {
            guard let hashes, hashes.count == Self.tileCount, hashes.allSatisfy(Self.isHash), tiles.isEmpty else {
                throw Invalid(reason: "full frame")
            }
            return
        }
        guard hasBase else { throw Invalid(reason: "no base frame") }
        var seen = Set<Int>()
        for tile in tiles {
            guard (0..<Self.tileCount).contains(tile.index), seen.insert(tile.index).inserted,
                  tile.x == (tile.index % columns) * tileSize, tile.y == (tile.index / columns) * tileSize,
                  tile.width == min(tileSize, width - tile.x), tile.height == min(tileSize, height - tile.y),
                  Self.isHash(tile.hash), ["image/png", "image/jpeg"].contains(tile.mime ?? "image/png") else {
                throw Invalid(reason: "tile")
            }
        }
    }
}
