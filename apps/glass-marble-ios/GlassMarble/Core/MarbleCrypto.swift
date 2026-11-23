import Foundation

/// SHA-256 and HMAC-SHA256 in pure Swift, so the certificate-pin check is
/// testable on Linux where CryptoKit does not exist. FIPS 180 / RFC 4231
/// vectors live in the tests.
private extension UInt32 {
    func marbleRotr(_ n: UInt32) -> UInt32 {
        (self >> n) | (self << (32 - n))
    }
}

enum MarbleSHA256 {
    private static let k: [UInt32] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1,
        0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
        0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
        0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
        0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
        0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
        0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ]

    static func hash(_ bytes: [UInt8]) -> [UInt8] {
        var h: [UInt32] = [
            0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
            0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
        ]
        var message = bytes
        let bitLength = UInt64(bytes.count) * 8
        message.append(0x80)
        while message.count % 64 != 56 { message.append(0) }
        for shift in stride(from: 56, through: 0, by: -8) {
            message.append(UInt8((bitLength >> UInt64(shift)) & 0xff))
        }
        for chunkStart in stride(from: 0, to: message.count, by: 64) {
            var w = [UInt32](repeating: 0, count: 64)
            for i in 0..<16 {
                let j = chunkStart + i * 4
                w[i] = UInt32(message[j]) << 24 | UInt32(message[j + 1]) << 16
                    | UInt32(message[j + 2]) << 8 | UInt32(message[j + 3])
            }
            for i in 16..<64 {
                let s0 = w[i - 15].marbleRotr(7) ^ w[i - 15].marbleRotr(18)
                    ^ (w[i - 15] >> 3)
                let s1 = w[i - 2].marbleRotr(17) ^ w[i - 2].marbleRotr(19)
                    ^ (w[i - 2] >> 10)
                w[i] = w[i - 16] &+ s0 &+ w[i - 7] &+ s1
            }
            var (a, b, c, d, e, f, g, hh) =
                (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7])
            for i in 0..<64 {
                let s1 = e.marbleRotr(6) ^ e.marbleRotr(11) ^ e.marbleRotr(25)
                let ch = (e & f) ^ (~e & g)
                let t1 = hh &+ s1 &+ ch &+ k[i] &+ w[i]
                let s0 = a.marbleRotr(2) ^ a.marbleRotr(13) ^ a.marbleRotr(22)
                let maj = (a & b) ^ (a & c) ^ (b & c)
                let t2 = s0 &+ maj
                hh = g; g = f; f = e; e = d &+ t1
                d = c; c = b; b = a; a = t1 &+ t2
            }
            h[0] &+= a; h[1] &+= b; h[2] &+= c; h[3] &+= d
            h[4] &+= e; h[5] &+= f; h[6] &+= g; h[7] &+= hh
        }
        return h.flatMap { word in
            [word >> 24, word >> 16, word >> 8, word]
                .map { UInt8(truncatingIfNeeded: $0) }
        }
    }

    static func hash(_ data: Data) -> Data {
        Data(hash([UInt8](data)))
    }

    static func hex(_ data: Data) -> String {
        hash(data).map { String(format: "%02x", $0) }.joined()
    }

    static func hex(of bytes: [UInt8]) -> String {
        hash(bytes).map { String(format: "%02x", $0) }.joined()
    }

    static func hexString(_ data: Data) -> String {
        data.map { String(format: "%02x", $0) }.joined()
    }
}

enum MarbleHMAC {
    /// HMAC-SHA256 per RFC 2104.
    static func sha256(key: [UInt8], message: [UInt8]) -> [UInt8] {
        var key = key
        if key.count > 64 {
            key = MarbleSHA256.hash(key)
        }
        key.append(contentsOf: [UInt8](repeating: 0, count: 64 - key.count))
        let outer = key.map { $0 ^ 0x5c }
        let inner = key.map { $0 ^ 0x36 }
        return MarbleSHA256.hash(outer + MarbleSHA256.hash(inner + message))
    }

    static func sha256Hex(key: [UInt8], message: [UInt8]) -> String {
        sha256(key: key, message: message)
            .map { String(format: "%02x", $0) }.joined()
    }
}

/// Certificate pinning that trusts the pairing token instead of a CA: the
/// client proves to itself that the leaf it observed is the leaf the token
/// holder serves, via the host's HMAC proof. The server's own `certSha256`
/// field is never trusted — only the locally observed hash is.
enum MarbleTLSPin {
    static let messagePrefix = "marble-tls-pin-v1"

    /// A fresh 64-hex-char nonce for one proof request.
    static func nonce() -> String {
        var bytes = [UInt8](repeating: 0, count: 32)
        #if canImport(Security)
        _ = SecRandomCopyBytes(kSecRandomDefault, bytes.count, &bytes)
        #else
        var state = UInt64(Date().timeIntervalSince1970 * 1_000_000)
            ^ UInt64.random(in: .min ... .max)
        for i in 0..<32 {
            state &+= 0x9e3779b97f4a7c15
            state ^= state >> 29
            state &*= 0xbf58476d1ce4e5b9
            bytes[i] = UInt8(truncatingIfNeeded: state >> ((i % 8) * 8))
        }
        #endif
        return bytes.map { String(format: "%02x", $0) }.joined()
    }

    /// True when the server's proof matches HMAC-SHA256(
    ///   key = token,
    ///   msg = "marble-tls-pin-v1\0" + nonce + "\0" + observedCertSHA256).
    /// `proofHex` and `nonce` come from the server; `observedCertSHA256` comes
    /// from the certificate bytes this client actually saw.
    static func verify(
        token: String,
        nonce: String,
        observedCertSHA256: String,
        proofHex: String
    ) -> Bool {
        var message = [UInt8](messagePrefix.utf8)
        message.append(0)
        message.append(contentsOf: [UInt8](nonce.utf8))
        message.append(0)
        message.append(contentsOf: [UInt8](observedCertSHA256.utf8))
        let expected = MarbleHMAC.sha256Hex(
            key: [UInt8](token.utf8), message: message
        )
        return constantTimeEqual(expected, proofHex)
    }

    /// A lowercase-hex compare that does not early-exit.
    static func constantTimeEqual(_ a: String, _ b: String) -> Bool {
        let a = Array(a.lowercased().utf8)
        let b = Array(b.lowercased().utf8)
        var difference = a.count ^ b.count
        for i in 0..<max(a.count, b.count) {
            difference |= Int(a[safe: i] ?? 0) ^ Int(b[safe: i] ?? 0)
        }
        return difference == 0 && !a.isEmpty
    }
}

private extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}
