// From apps/glass-ios, compile with:
// swiftc Glass/Design/LensMark.swift scripts/render-app-icon.swift -o /path/to/render-app-icon
// Run with the path to AppIcon.appiconset as its argument.
import Foundation
import ImageIO
import SwiftUI
import UniformTypeIdentifiers

@main
@MainActor
struct RenderAppIcon {
    static func main() throws {
        let output = URL(fileURLWithPath: CommandLine.arguments.dropFirst().first ?? ".")
        try render(to: output.appendingPathComponent("AppIcon-1024.png"), field: 0xE5EDF4)
        try render(to: output.appendingPathComponent("AppIcon-1024-dark.png"), field: 0x142126, dark: true)
        try render(to: output.appendingPathComponent("AppIcon-1024-tinted.png"), field: 0x343434, dark: true,
                   tint: Color(lensHex: 0xBFBFBF))
    }

    static func render(to url: URL, field: UInt32, dark: Bool = false, tint: Color? = nil) throws {
        let size: CGFloat = 1024
        let view = Horizon(fullWidth: true)
            .stroke(.white.opacity(dark ? 0.44 : 0.90), lineWidth: size * 0.7 / 67)
            .overlay {
                LensMark(tint: tint).frame(width: size * 47 / 67, height: size * 47 / 67)
            }
            .frame(width: size, height: size)
            .background(Color(lensHex: field))
            .environment(\.colorScheme, dark ? .dark : .light)
        let renderer = ImageRenderer(content: view)
        renderer.isOpaque = true
        guard let image = renderer.cgImage,
              let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)
        else { throw CocoaError(.fileWriteUnknown) }
        CGImageDestinationAddImage(destination, image, nil)
        guard CGImageDestinationFinalize(destination) else { throw CocoaError(.fileWriteUnknown) }
    }
}
