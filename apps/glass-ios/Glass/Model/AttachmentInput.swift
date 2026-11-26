import Foundation
import GlassModels
import ImageIO
import UniformTypeIdentifiers

struct AttachmentInput: Sendable {
    static let fileTypes: [UTType] = [.jpeg, .png, .gif, .webP, .heic, .heif, .plainText, .sourceCode, .json]

    var name: String
    var data: Data?
    var isPhoto = false

    func prepared(imageLimit: Int = UploadPolicy.imageLimit) throws -> (name: String, data: Data) {
        guard let data else { throw PreparationError.unreadable(name) }
        if UploadPolicy.imageType(data) != nil, data.count <= imageLimit {
            return (name, data)
        }
        if let source = CGImageSourceCreateWithData(data as CFData, [kCGImageSourceShouldCache: false] as CFDictionary),
           let identifier = CGImageSourceGetType(source), UTType(identifier as String)?.conforms(to: .image) == true {
            let output = try jpeg(from: source, limit: imageLimit)
            return ((name as NSString).deletingPathExtension + ".jpg", output)
        }
        if isPhoto {
            if data.isEmpty { throw UploadPolicy.Rejection.empty(name) }
            throw PreparationError.unreadable(name)
        }
        _ = try UploadPolicy.check(data, name: name)
        return (name, data)
    }

    private func jpeg(from source: CGImageSource, limit: Int) throws -> Data {
        let dimensions = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any]
        let width = dimensions?[kCGImagePropertyPixelWidth] as? Int ?? 0
        let height = dimensions?[kCGImagePropertyPixelHeight] as? Int ?? 0
        var size = min(max(width, height), 4096)
        while size > 0 {
            let options: [CFString: Any] = [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: size,
            ]
            guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { break }
            let output = NSMutableData()
            guard let destination = CGImageDestinationCreateWithData(output, UTType.jpeg.identifier as CFString, 1, nil) else {
                break
            }
            CGImageDestinationAddImage(destination, image, [kCGImageDestinationLossyCompressionQuality: 0.85] as CFDictionary)
            guard CGImageDestinationFinalize(destination) else { break }
            if output.length <= limit { return output as Data }
            size = size * 3 / 4
        }
        throw PreparationError.cannotEncode(name)
    }

    private enum PreparationError: LocalizedError {
        case unreadable(String), cannotEncode(String)

        var errorDescription: String? {
            switch self {
                case .unreadable(let name): "Couldn’t read \(name)."
                case .cannotEncode(let name): "Couldn’t convert \(name) to a JPEG within 8 MiB."
            }
        }
    }
}
