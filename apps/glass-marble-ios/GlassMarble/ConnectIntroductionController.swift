import SwiftUI
import UIKit

/// Storyboard scene shown on first launch. The layout lives in `Main.storyboard`;
/// this controller only applies the product palette and reports `Continue`.
final class ConnectIntroductionController: UIViewController {
    @IBOutlet private weak var titleLabel: UILabel!
    @IBOutlet private weak var bodyLabel: UILabel!
    @IBOutlet private weak var continueButton: UIButton!

    var onContinue: (() -> Void)?

    /// SwiftPM builds ship no compiled storyboard; with `nibName == nil` the
    /// outlets would never be wired, so rebuild the same scene in code.
    override func loadView() {
        guard nibName == nil else {
            super.loadView()
            return
        }
        let root = UIView()
        view = root

        let mark = UILabel()
        mark.text = "M"
        mark.textAlignment = .center
        mark.font = .systemFont(ofSize: 32, weight: .semibold)
        mark.textColor = .label

        let title = UILabel()
        title.text = "Glass Marble"
        title.font = .systemFont(ofSize: 28, weight: .bold)
        title.textColor = .label
        title.numberOfLines = 0

        let body = UILabel()
        body.text = "Marble runs on your own machine."
        body.font = .systemFont(ofSize: 15)
        body.textColor = .secondaryLabel
        body.numberOfLines = 0

        let button = UIButton(type: .system)
        button.setTitle("Got it", for: .normal)
        button.addTarget(self, action: #selector(continueTapped(_:)), for: .touchUpInside)

        for subview in [mark, title, body, button] {
            subview.translatesAutoresizingMaskIntoConstraints = false
            root.addSubview(subview)
        }
        let safe = root.safeAreaLayoutGuide
        NSLayoutConstraint.activate([
            mark.topAnchor.constraint(equalTo: safe.topAnchor, constant: 73),
            mark.leadingAnchor.constraint(equalTo: safe.leadingAnchor, constant: 24),
            mark.widthAnchor.constraint(equalToConstant: 60),
            mark.heightAnchor.constraint(equalToConstant: 60),
            title.topAnchor.constraint(equalTo: mark.bottomAnchor, constant: 20),
            title.leadingAnchor.constraint(equalTo: safe.leadingAnchor, constant: 24),
            title.trailingAnchor.constraint(equalTo: safe.trailingAnchor, constant: -24),
            body.topAnchor.constraint(equalTo: title.bottomAnchor, constant: 12),
            body.leadingAnchor.constraint(equalTo: title.leadingAnchor),
            body.trailingAnchor.constraint(equalTo: title.trailingAnchor),
            button.topAnchor.constraint(equalTo: body.bottomAnchor, constant: 44),
            button.leadingAnchor.constraint(equalTo: title.leadingAnchor),
            button.trailingAnchor.constraint(equalTo: title.trailingAnchor),
            button.heightAnchor.constraint(equalToConstant: 50),
        ])

        titleLabel = title
        bodyLabel = body
        continueButton = button
    }

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = UIColor(red: 0.04, green: 0.07, blue: 0.12, alpha: 1)
        titleLabel.textColor = UIColor(red: 0.93, green: 0.95, blue: 0.99, alpha: 1)
        bodyLabel.textColor = UIColor(red: 0.62, green: 0.68, blue: 0.78, alpha: 1)
        bodyLabel.text = """
        Marble runs on your own machine.

        Point this app at your host's Tailscale address, then unlock it with the token you configured.
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
        let controller: ConnectIntroductionController
        if Bundle.main.url(forResource: "Main", withExtension: "storyboardc") != nil {
            controller = UIStoryboard(name: "Main", bundle: .main)
                .instantiateInitialViewController() as? ConnectIntroductionController
                ?? ConnectIntroductionController(nibName: nil, bundle: nil)
        } else {
            controller = ConnectIntroductionController(nibName: nil, bundle: nil)
        }
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
