import SwiftUI
import UIKit

/// Storyboard scene shown on first launch. The layout lives in `Main.storyboard`;
/// this controller only applies the product palette and reports `Continue`.
final class ConnectIntroductionController: UIViewController {
    @IBOutlet private weak var titleLabel: UILabel!
    @IBOutlet private weak var bodyLabel: UILabel!
    @IBOutlet private weak var continueButton: UIButton!

    var onContinue: (() -> Void)?

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = UIColor(red: 0.04, green: 0.07, blue: 0.12, alpha: 1)
        titleLabel.textColor = UIColor(red: 0.93, green: 0.95, blue: 0.99, alpha: 1)
        bodyLabel.textColor = UIColor(red: 0.62, green: 0.68, blue: 0.78, alpha: 1)
        bodyLabel.text = """
        Marble runs on your own machine and keeps every turn in a durable journal.

        Point this app at your host's Tailscale address, unlock it with the token you configured, and you can read, steer, and audit the conversation from any device.
        """
        continueButton.backgroundColor = UIColor(red: 0.43, green: 0.66, blue: 0.95, alpha: 1)
        continueButton.layer.cornerRadius = 25
        continueButton.layer.cornerCurve = .continuous
        continueButton.setTitleColor(.white, for: .normal)
        continueButton.titleLabel?.font = .systemFont(ofSize: 16, weight: .semibold)
    }

    @IBAction private func continueTapped(_ sender: UIButton) {
        onContinue?()
        dismiss(animated: true)
    }
}

/// Hosts the storyboard scene inside SwiftUI.
struct ConnectIntroductionView: UIViewControllerRepresentable {
    var onFinish: () -> Void

    func makeUIViewController(context: Context) -> ConnectIntroductionController {
        let storyboard = UIStoryboard(name: "Main", bundle: .main)
        let controller = storyboard.instantiateInitialViewController() as? ConnectIntroductionController
            ?? ConnectIntroductionController(nibName: nil, bundle: nil)
        controller.onContinue = onFinish
        return controller
    }

    func updateUIViewController(
        _ uiViewController: ConnectIntroductionController,
        context: Context
    ) {
        uiViewController.onContinue = onFinish
    }
}
