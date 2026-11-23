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

        let title = UILabel()
        title.text = "Glass Marble"
        title.font = .preferredFont(forTextStyle: .title1)
        title.adjustsFontForContentSizeCategory = true
        title.textColor = .label
        title.numberOfLines = 0

        let body = UILabel()
        body.text = "Marble runs on your own machine."
        body.font = .preferredFont(forTextStyle: .body)
        body.adjustsFontForContentSizeCategory = true
        body.textColor = .secondaryLabel
        body.numberOfLines = 0

        let button = UIButton(type: .system)
        button.setTitle("Continue", for: .normal)
        button.addTarget(self, action: #selector(continueTapped(_:)), for: .touchUpInside)

        for subview in [title, body, button] {
            subview.translatesAutoresizingMaskIntoConstraints = false
            root.addSubview(subview)
        }
        let safe = root.safeAreaLayoutGuide
        NSLayoutConstraint.activate([
            title.topAnchor.constraint(equalTo: safe.topAnchor, constant: 48),
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
        view.backgroundColor = .systemBackground
        titleLabel.textColor = .label
        bodyLabel.textColor = .secondaryLabel
        bodyLabel.text = """
        Marble runs on your own machine.

        Point this app at your host's Tailscale address, then unlock it with the token you configured.
        """
        var configuration = UIButton.Configuration.filled()
        configuration.cornerStyle = .large
        continueButton.configuration = configuration
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
