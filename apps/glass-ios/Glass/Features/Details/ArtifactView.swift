import GlassAPI
import GlassTransport
import GlassModels
import SwiftUI
import WebKit

/// Interactive HTML, registered with the host and shown from its isolated, sandboxed artifact route.
struct ArtifactView: View {
    let conversation: ConversationModel
    let html: Data
    let title: String
    @Environment(AppModel.self) private var app
    @Environment(\.dismiss) private var dismiss
    @State private var token: String?
    @State private var error: String?

    var body: some View {
        NavigationStack {
            Group {
                if let token, let api = app.api {
                    ArtifactWebView(token: token, client: api)
                        .ignoresSafeArea(edges: .bottom)
                        .accessibilityIdentifier("artifact.web")
                } else if let error {
                    ContentUnavailableView("Can’t preview", systemImage: "exclamationmark.triangle", description: Text(error))
                } else {
                    ProgressView()
                }
            }
            .navigationTitle(title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
            .task { await register() }
        }
    }

    private func register() async {
        guard let api = app.api else { return }
        do {
            token = try await api.createArtifact(html, thread: conversation.thread.id, sourceRef: conversation.thread.sourceRef).token
        } catch {
            self.error = error.localizedDescription
        }
    }
}

private struct ArtifactWebView: UIViewRepresentable {
    static let scheme = "glass-artifact"
    let token: String
    let client: GlassClient

    func makeCoordinator() -> ArtifactSchemeHandler { ArtifactSchemeHandler(client: client) }

    func makeUIView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.websiteDataStore = .nonPersistent()
        configuration.setURLSchemeHandler(context.coordinator, forURLScheme: Self.scheme)
        let view = WKWebView(frame: .zero, configuration: configuration)
        view.navigationDelegate = context.coordinator
        view.load(URLRequest(url: URL(string: "\(Self.scheme)://artifact/?token=\(token)")!))
        return view
    }

    func updateUIView(_ view: WKWebView, context: Context) {}
}

/// Serves only the artifact document; every other navigation is refused.
final class ArtifactSchemeHandler: NSObject, WKURLSchemeHandler, WKNavigationDelegate {
    /// The host's sandbox policy for artifacts, reapplied because custom-scheme responses carry no host headers.
    static let policy = "sandbox allow-scripts; default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; media-src data: blob:; connect-src 'none'; frame-src 'none'; child-src 'none'; worker-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'"

    private let client: GlassClient
    private var active: Set<ObjectIdentifier> = []

    init(client: GlassClient) { self.client = client }

    func webView(_ webView: WKWebView, start task: any WKURLSchemeTask) {
        let id = ObjectIdentifier(task)
        active.insert(id)
        let url = task.request.url
        let token = url.flatMap { URLComponents(url: $0, resolvingAgainstBaseURL: false)?.queryItems?.first { $0.name == "token" }?.value }
        Task { @MainActor in
            defer { active.remove(id) }
            do {
                guard let url, let token else { throw GlassError.invalidResponse("Missing preview token") }
                let document = try await client.artifactDocument(token: token)
                guard active.contains(id) else { return }
                let response = HTTPURLResponse(url: url, statusCode: 200, httpVersion: "HTTP/1.1", headerFields: [
                    "Content-Type": "text/html; charset=utf-8", "Content-Security-Policy": Self.policy,
                ])!
                task.didReceive(response)
                task.didReceive(document.data)
                task.didFinish()
            } catch {
                guard active.contains(id) else { return }
                task.didFailWithError(error)
            }
        }
    }

    func webView(_ webView: WKWebView, stop task: any WKURLSchemeTask) {
        active.remove(ObjectIdentifier(task))
    }

    func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction) async -> WKNavigationActionPolicy {
        action.request.url?.scheme == ArtifactWebView.scheme ? .allow : .cancel
    }
}
