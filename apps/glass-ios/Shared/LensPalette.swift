import SwiftUI

enum LensPalette {
    static let lightTeal: UInt32 = 0x08737D
    static let darkTeal: UInt32 = 0x60E4E8
}

extension Color {
    init(lensHex rgb: UInt32) {
        self.init(red: Double((rgb >> 16) & 255) / 255,
                  green: Double((rgb >> 8) & 255) / 255,
                  blue: Double(rgb & 255) / 255)
    }
}
