import CoreGraphics
import GlassModels
import UIKit

/// Paints desktop region updates into one 1280×800 bitmap.
final class DesktopCanvas {
    struct Failure: Error {}

    private var context: CGContext?
    var hasBase: Bool { context != nil }

    /// Decodes every region first, then paints them together and returns the new picture.
    func apply(_ update: DesktopUpdate) throws -> UIImage? {
        var patches: [(CGImage, CGRect)] = []
        if let full = update.full {
            patches.append((try Self.decode(full, width: update.width, height: update.height),
                            CGRect(x: 0, y: 0, width: update.width, height: update.height)))
        } else {
            for tile in update.tiles {
                patches.append((try Self.decode(tile.data, width: tile.width, height: tile.height),
                                CGRect(x: tile.x, y: update.height - tile.y - tile.height, width: tile.width, height: tile.height)))
            }
        }
        if context == nil || update.full != nil {
            context = CGContext(data: nil, width: update.width, height: update.height, bitsPerComponent: 8, bytesPerRow: 0,
                                space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                bitmapInfo: CGImageAlphaInfo.noneSkipFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue)
        }
        guard let context else { throw Failure() }
        guard !patches.isEmpty else { return nil }
        for (image, rect) in patches { context.draw(image, in: rect) }
        return context.makeImage().map { UIImage(cgImage: $0) }
    }

    private static func decode(_ base64: String, width: Int, height: Int) throws -> CGImage {
        guard base64.utf8.count <= 6 * 1024 * 1024, let data = Data(base64Encoded: base64),
              let image = UIImage(data: data)?.cgImage, image.width == width, image.height == height else { throw Failure() }
        return image
    }
}
