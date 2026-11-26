import Foundation
import GlassModels
import ImageIO
import Testing
import UniformTypeIdentifiers
@testable import Glass

@Suite struct AttachmentInputTests {
    @Test func filesPickerAllowsSupportedImagesHEIFAndTextButNotArbitraryData() {
        for type in [UTType.jpeg, .png, .gif, .webP, .heic, .heif, .plainText, .utf8PlainText, .sourceCode, .json] {
            #expect(AttachmentInput.fileTypes.contains { type.conforms(to: $0) })
        }
        for type in [UTType.image, .pdf, .rtf, .movie, .data] {
            #expect(!AttachmentInput.fileTypes.contains { type.conforms(to: $0) })
        }
    }

    @Test func convertsOtherImagesAndDownscalesWhenJPEGExceedsLimit() throws {
        let tiff = try makeImage(as: .tiff)
        let input = AttachmentInput(name: "sample.tiff", data: tiff)
        let full = try input.prepared()
        #expect(full.name == "sample.jpg")
        #expect(UploadPolicy.imageType(full.data) == "image/jpeg")
        #expect(try pixelWidth(full.data) == 256)

        let limit = full.data.count / 2
        let scaled = try AttachmentInput(name: full.name, data: full.data, isPhoto: true).prepared(imageLimit: limit)
        #expect(UploadPolicy.imageType(scaled.data) == "image/jpeg")
        #expect(scaled.data.count <= limit)
        #expect(try pixelWidth(scaled.data) < 256)
    }

    @Test func preservesAcceptedImagesAndRejectsInvalidTextLocally() throws {
        let png = try makeImage(as: .png)
        let prepared = try AttachmentInput(name: "sample.png", data: png).prepared()
        #expect(prepared.name == "sample.png")
        #expect(prepared.data == png)
        let oversized = Data(repeating: 0x41, count: UploadPolicy.textLimit + 1)
        #expect(throws: UploadPolicy.Rejection.notText("notes.txt")) {
            try AttachmentInput(name: "notes.txt", data: oversized).prepared()
        }
    }

    private func makeImage(as type: UTType) throws -> Data {
        let context = try #require(CGContext(data: nil, width: 256, height: 128, bitsPerComponent: 8, bytesPerRow: 0,
                                             space: CGColorSpaceCreateDeviceRGB(),
                                             bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue))
        for y in stride(from: 0, to: 128, by: 4) {
            for x in stride(from: 0, to: 256, by: 4) {
                let shade = CGFloat((x * y + x + y) % 255) / 255
                context.setFillColor(red: shade, green: 1 - shade, blue: 0.5, alpha: 1)
                context.fill(CGRect(x: x, y: y, width: 4, height: 4))
            }
        }
        let image = try #require(context.makeImage())
        let data = NSMutableData()
        let destination = try #require(CGImageDestinationCreateWithData(data, type.identifier as CFString, 1, nil))
        CGImageDestinationAddImage(destination, image, nil)
        try #require(CGImageDestinationFinalize(destination))
        return data as Data
    }

    private func pixelWidth(_ data: Data) throws -> Int {
        let source = try #require(CGImageSourceCreateWithData(data as CFData, nil))
        return try #require(CGImageSourceCreateImageAtIndex(source, 0, nil)).width
    }
}
