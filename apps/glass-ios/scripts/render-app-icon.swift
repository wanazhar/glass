// Renders the Glass app icon variants into the asset catalog.
// Usage: swift apps/glass-ios/scripts/render-app-icon.swift <AppIcon.appiconset>
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let size = 1024
let outDir = URL(fileURLWithPath: CommandLine.arguments.dropFirst().first ?? ".")
let space = CGColorSpace(name: CGColorSpace.displayP3)!

func color(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat, _ a: CGFloat = 1) -> CGColor {
    CGColor(colorSpace: space, components: [r, g, b, a])!
}

enum Variant { case light, dark, tinted }

func render(_ variant: Variant, to name: String) throws {
    let ctx = CGContext(
        data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0, space: space,
        bitmapInfo: variant == .light
            ? CGImageAlphaInfo.noneSkipLast.rawValue : CGImageAlphaInfo.premultipliedLast.rawValue)!
    let s = CGFloat(size)
    let full = CGRect(x: 0, y: 0, width: s, height: s)

    switch variant {
    case .light:
        let bg = CGGradient(
            colorsSpace: space,
            colors: [color(0.36, 0.62, 1.0), color(0.20, 0.30, 0.86), color(0.42, 0.20, 0.78)] as CFArray,
            locations: [0, 0.55, 1])!
        ctx.drawLinearGradient(bg, start: CGPoint(x: 0, y: s), end: CGPoint(x: s, y: 0), options: [])
    case .dark:
        ctx.setFillColor(color(0.04, 0.05, 0.09))
        ctx.fill(full)
    case .tinted:
        ctx.clear(full)
    }

    // A faceted glass prism: three overlapping translucent panes.
    let panes: [(CGRect, CGFloat, CGFloat)] = [
        (CGRect(x: 0.20, y: 0.30, width: 0.42, height: 0.50), -0.16, 0.30),
        (CGRect(x: 0.38, y: 0.20, width: 0.42, height: 0.50), 0.12, 0.42),
        (CGRect(x: 0.29, y: 0.25, width: 0.42, height: 0.50), 0.0, 0.62),
    ]
    for (unit, angle, alpha) in panes {
        let rect = CGRect(x: unit.minX * s, y: unit.minY * s, width: unit.width * s, height: unit.height * s)
        ctx.saveGState()
        ctx.translateBy(x: rect.midX, y: rect.midY)
        ctx.rotate(by: angle)
        let local = CGRect(x: -rect.width / 2, y: -rect.height / 2, width: rect.width, height: rect.height)
        let path = CGPath(roundedRect: local, cornerWidth: s * 0.085, cornerHeight: s * 0.085, transform: nil)
        ctx.addPath(path)
        ctx.clip()
        let top: CGColor
        let bottom: CGColor
        switch variant {
        case .light:
            top = color(1, 1, 1, alpha)
            bottom = color(1, 1, 1, alpha * 0.35)
        case .dark:
            top = color(0.55, 0.75, 1.0, alpha)
            bottom = color(0.45, 0.30, 0.95, alpha * 0.45)
        case .tinted:
            top = color(1, 1, 1, alpha)
            bottom = color(1, 1, 1, alpha * 0.4)
        }
        let fill = CGGradient(colorsSpace: space, colors: [top, bottom] as CFArray, locations: [0, 1])!
        ctx.drawLinearGradient(
            fill, start: CGPoint(x: 0, y: local.maxY), end: CGPoint(x: 0, y: local.minY), options: [])
        ctx.resetClip()
        ctx.addPath(path)
        ctx.setStrokeColor(color(1, 1, 1, variant == .dark ? 0.55 : 0.75))
        ctx.setLineWidth(s * 0.006)
        ctx.strokePath()
        ctx.restoreGState()
    }

    let image = ctx.makeImage()!
    let url = outDir.appendingPathComponent(name)
    let dest = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)!
    CGImageDestinationAddImage(dest, image, nil)
    guard CGImageDestinationFinalize(dest) else { throw CocoaError(.fileWriteUnknown) }
}

try render(.light, to: "AppIcon-1024.png")
try render(.dark, to: "AppIcon-1024-dark.png")
try render(.tinted, to: "AppIcon-1024-tinted.png")
