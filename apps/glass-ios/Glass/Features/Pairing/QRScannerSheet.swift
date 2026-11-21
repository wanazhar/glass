import SwiftUI
import VisionKit

/// Live camera QR scanning with VisionKit; explains itself when the camera is unavailable.
struct QRScannerSheet: View {
    var onScan: (String) -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Group {
                if DataScannerViewController.isSupported && DataScannerViewController.isAvailable {
                    QRScanner(onScan: onScan)
                        .ignoresSafeArea()
                        .overlay(alignment: .bottom) {
                            Text("Point at the QR code from gc connect qr")
                                .font(.subheadline.weight(.medium))
                                .padding(.horizontal, 18)
                                .padding(.vertical, 12)
                                .glassSurface()
                                .padding(.bottom, 32)
                        }
                } else {
                    ContentUnavailableView {
                        Label("Camera unavailable", systemImage: "camera.metering.unknown")
                    } description: {
                        Text("Allow camera access for Glass in Settings, or paste the link instead.")
                    } actions: {
                        Button("Open Settings") {
                            if let url = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(url) }
                        }
                        .glassButtonStyle()
                    }
                    .accessibilityIdentifier("scan.unavailable")
                }
            }
            .navigationTitle("Scan QR Code")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel", role: .cancel) { dismiss() }
                }
            }
        }
    }
}

private struct QRScanner: UIViewControllerRepresentable {
    var onScan: (String) -> Void

    func makeUIViewController(context: Context) -> DataScannerViewController {
        let scanner = DataScannerViewController(
            recognizedDataTypes: [.barcode(symbologies: [.qr])],
            qualityLevel: .balanced,
            recognizesMultipleItems: false,
            isHighFrameRateTrackingEnabled: false,
            isHighlightingEnabled: true
        )
        scanner.delegate = context.coordinator
        try? scanner.startScanning()
        return scanner
    }

    func updateUIViewController(_ controller: DataScannerViewController, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator(onScan: onScan) }

    final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        let onScan: (String) -> Void
        private var delivered = false

        init(onScan: @escaping (String) -> Void) { self.onScan = onScan }

        func dataScanner(_ scanner: DataScannerViewController, didAdd items: [RecognizedItem], allItems: [RecognizedItem]) {
            for case .barcode(let code) in items {
                guard !delivered, let payload = code.payloadStringValue, payload.contains("://") else { continue }
                delivered = true
                scanner.stopScanning()
                UINotificationFeedbackGenerator().notificationOccurred(.success)
                onScan(payload)
            }
        }
    }
}
