import CoreGraphics
import Testing
@testable import Glass

@Suite struct DesktopViewportTests {
    let viewport = DesktopViewport(container: CGSize(width: 400, height: 400), image: CGSize(width: 1280, height: 800))

    @Test func letterboxesThePicture() {
        #expect(viewport.pictureRect == CGRect(x: 0, y: 75, width: 400, height: 250))
        #expect(viewport.desktopPoint(at: CGPoint(x: 200, y: 50)) == nil)
        #expect(viewport.desktopPoint(at: CGPoint(x: 200, y: 50), clamped: true) == CGPoint(x: 0.5, y: 0))
    }

    @Test func mapsTouchesThroughZoomAndPan() throws {
        var zoomed = viewport
        zoomed.zoom = 2
        zoomed.pan = CGSize(width: 100, height: 0)
        let point = try #require(zoomed.desktopPoint(at: CGPoint(x: 300, y: 200)))
        #expect(abs(point.x - 0.5) < 0.0001 && abs(point.y - 0.5) < 0.0001)
        let back = zoomed.screenPoint(for: point)
        #expect(abs(back.x - 300) < 0.0001 && abs(back.y - 200) < 0.0001)
    }

    @Test func keepsPanInsideTheZoomedPicture() {
        var zoomed = viewport
        zoomed.zoom = 2
        #expect(zoomed.clampedPan(CGSize(width: 9999, height: 0)).width == 200)
        #expect(viewport.clampedPan(CGSize(width: 50, height: 50)) == .zero)
    }
}
