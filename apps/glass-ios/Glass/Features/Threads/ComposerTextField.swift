import SwiftUI
import UIKit

/// The multiline message field. With a hardware keyboard, Return sends and Shift-Return inserts a line break;
/// the on-screen Return key always inserts a line break.
struct ComposerTextField: UIViewRepresentable {
    var placeholder: String
    @Binding var text: String
    var isEnabled: Bool
    var maxLines = 6
    /// A new pending decision puts the keyboard away so the request is visible.
    var dismissFor: String?
    /// ⌘K and ⌘/ while typing open the command palette.
    var onCommands: (() -> Void)?
    var onReturn: () -> Void

    func makeCoordinator() -> Coordinator { Coordinator(text: $text) }

    func makeUIView(context: Context) -> ComposerTextView {
        let view = ComposerTextView()
        view.delegate = context.coordinator
        view.accessibilityIdentifier = "composer.field"
        return view
    }

    func updateUIView(_ view: ComposerTextView, context: Context) {
        context.coordinator.text = $text
        if view.text != text {
            view.text = text
            // A restored draft or dictation result continues at its end.
            view.selectedRange = NSRange(location: (text as NSString).length, length: 0)
        }
        view.isEditable = isEnabled
        view.textColor = isEnabled ? .label : .secondaryLabel
        view.onReturn = onReturn
        view.onCommands = onCommands
        view.placeholder = placeholder
        if context.coordinator.dismissedFor != dismissFor {
            context.coordinator.dismissedFor = dismissFor
            if dismissFor != nil { view.resignFirstResponder() }
        }
    }

    func sizeThatFits(_ proposal: ProposedViewSize, uiView: ComposerTextView, context: Context) -> CGSize? {
        let width = proposal.width ?? 240
        let fitting = uiView.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude)).height
        let limit = ceil((uiView.font?.lineHeight ?? 22) * CGFloat(maxLines))
        uiView.isScrollEnabled = fitting > limit
        return CGSize(width: width, height: min(fitting, limit))
    }

    final class Coordinator: NSObject, UITextViewDelegate {
        var text: Binding<String>
        var dismissedFor: String?
        init(text: Binding<String>) { self.text = text }

        func textViewDidChange(_ textView: UITextView) {
            if text.wrappedValue != textView.text { text.wrappedValue = textView.text }
            (textView as? ComposerTextView)?.refreshPlaceholder()
        }
    }
}

final class ComposerTextView: UITextView {
    var onReturn: (() -> Void)?
    var onCommands: (() -> Void)?
    var placeholder = "" {
        didSet {
            placeholderLabel.text = placeholder
            accessibilityLabel = placeholder
        }
    }
    override var text: String! {
        didSet { refreshPlaceholder() }
    }
    /// UIKit derives the editing traits on the fly, so a read-only field adds `.notEnabled` here rather than
    /// storing a fixed set.
    override var accessibilityTraits: UIAccessibilityTraits {
        get { isEditable ? super.accessibilityTraits : super.accessibilityTraits.union(.notEnabled) }
        set { super.accessibilityTraits = newValue }
    }
    private let placeholderLabel = UILabel()
    private var consumingReturn = false

    init() {
        super.init(frame: .zero, textContainer: nil)
        font = .preferredFont(forTextStyle: .body)
        adjustsFontForContentSizeCategory = true
        backgroundColor = .clear
        textContainerInset = .zero
        textContainer.lineFragmentPadding = 0
        isScrollEnabled = false
        setContentCompressionResistancePriority(.defaultLow, for: .horizontal)

        placeholderLabel.font = .preferredFont(forTextStyle: .body)
        placeholderLabel.adjustsFontForContentSizeCategory = true
        placeholderLabel.textColor = .placeholderText
        placeholderLabel.isAccessibilityElement = false
        placeholderLabel.translatesAutoresizingMaskIntoConstraints = false
        addSubview(placeholderLabel)
        NSLayoutConstraint.activate([
            placeholderLabel.leadingAnchor.constraint(equalTo: frameLayoutGuide.leadingAnchor),
            placeholderLabel.trailingAnchor.constraint(lessThanOrEqualTo: frameLayoutGuide.trailingAnchor),
            placeholderLabel.topAnchor.constraint(equalTo: frameLayoutGuide.topAnchor),
        ])
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { fatalError("init(coder:) is not supported") }

    override var keyCommands: [UIKeyCommand]? {
        guard onCommands != nil else { return super.keyCommands }
        return (super.keyCommands ?? []) + ["k", "/"].map { input in
            let command = UIKeyCommand(title: "Commands", action: #selector(openCommands), input: input, modifierFlags: .command)
            command.wantsPriorityOverSystemBehavior = true
            return command
        }
    }

    @objc private func openCommands() { onCommands?() }

    func refreshPlaceholder() { placeholderLabel.isHidden = !(text ?? "").isEmpty }

    private func isReturn(_ presses: Set<UIPress>) -> UIKey? {
        presses.compactMap(\.key).first { $0.keyCode == .keyboardReturnOrEnter }
    }

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        guard isEditable, markedTextRange == nil, let key = isReturn(presses) else {
            return super.pressesBegan(presses, with: event)
        }
        consumingReturn = true
        if key.modifierFlags.contains(.shift) {
            insertText("\n")
            delegate?.textViewDidChange?(self)
        } else {
            onReturn?()
        }
    }

    override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        guard consumingReturn, isReturn(presses) != nil else { return super.pressesEnded(presses, with: event) }
        consumingReturn = false
    }

    override func pressesCancelled(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        guard consumingReturn, isReturn(presses) != nil else { return super.pressesCancelled(presses, with: event) }
        consumingReturn = false
    }
}
