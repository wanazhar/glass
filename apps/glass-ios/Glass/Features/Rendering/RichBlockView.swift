import SwiftUI
import WebKit

/// Mermaid diagrams rendered offline from the bundled script.
/// Falls back to the source as a code block when rendering fails.
struct RichBlockView: View {
    var source: String
    @Environment(\.colorScheme) private var colorScheme
    @State private var height: CGFloat = 44
    @State private var failure: String?

    var body: some View {
        if let failure {
            VStack(alignment: .leading, spacing: 4) {
                CodeBlock(code: source, language: "mermaid")
                Text(failure).font(.caption).foregroundStyle(.secondary)
            }
        } else {
            RenderWebView(source: source, dark: colorScheme == .dark, height: $height, failure: $failure)
                .frame(height: min(height, 640))
                .accessibilityElement()
                .accessibilityLabel("Diagram")
                .accessibilityValue(source)
                .accessibilityIdentifier("timeline.rich.mermaid")
        }
    }
}

private struct RenderWebView: UIViewRepresentable {
    var source: String
    var dark: Bool
    @Binding var height: CGFloat
    @Binding var failure: String?

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeUIView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        configuration.userContentController.add(context.coordinator, name: "glass")
        let view = WKWebView(frame: .zero, configuration: configuration)
        view.isOpaque = false
        view.backgroundColor = .clear
        view.scrollView.backgroundColor = .clear
        view.scrollView.bounces = false
        view.scrollView.showsVerticalScrollIndicator = false
        view.navigationDelegate = context.coordinator
        if let page = Bundle.main.url(forResource: "render", withExtension: "html") {
            view.loadFileURL(page, allowingReadAccessTo: page.deletingLastPathComponent())
        } else {
            failure = "Preview isn’t available."
        }
        return view
    }

    func updateUIView(_ view: WKWebView, context: Context) {
        context.coordinator.parent = self
        context.coordinator.renderIfNeeded(view)
    }

    static func dismantleUIView(_ view: WKWebView, coordinator: Coordinator) {
        view.configuration.userContentController.removeScriptMessageHandler(forName: "glass")
    }

    final class Coordinator: NSObject, WKScriptMessageHandler, WKNavigationDelegate {
        var parent: RenderWebView
        private var loaded = false
        private var rendered: String?

        init(_ parent: RenderWebView) { self.parent = parent }

        func renderIfNeeded(_ view: WKWebView) {
            let key = "\(parent.dark)|\(parent.source)"
            guard loaded, rendered != key else { return }
            rendered = key
            view.callAsyncJavaScript("render(source, dark)",
                                     arguments: ["source": parent.source, "dark": parent.dark],
                                     in: nil, in: .page, completionHandler: nil)
        }

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            loaded = true
            renderIfNeeded(webView)
        }

        func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction) async -> WKNavigationActionPolicy {
            action.request.url?.isFileURL == true ? .allow : .cancel
        }

        func userContentController(_ controller: WKUserContentController, didReceive message: WKScriptMessage) {
            guard let body = message.body as? [String: Any] else { return }
            if let height = body["height"] as? Double { parent.height = max(24, height) }
            if let error = body["error"] as? String { parent.failure = "Couldn’t draw this: \(error)" }
        }
    }
}
