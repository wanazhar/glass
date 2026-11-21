import AVFoundation
import Observation
import Speech

/// On-device dictation into the composer with `SFSpeechRecognizer`.
@MainActor @Observable
final class Dictation {
    enum State: Equatable {
        case idle
        case listening
        case unavailable(String)
    }

    private(set) var state: State = .idle
    private let engine = AVAudioEngine()
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?

    var isListening: Bool { state == .listening }

    /// Starts listening, or stops if already listening. `update` receives the full transcript so far.
    func toggle(update: @escaping @MainActor (String) -> Void) async {
        if isListening { return stop() }
        guard await Self.authorize() else {
            state = .unavailable("Allow Microphone and Speech Recognition for Glass in Settings to dictate.")
            return
        }
        guard let recognizer = SFSpeechRecognizer(), recognizer.isAvailable else {
            state = .unavailable("Dictation isn’t available right now.")
            return
        }
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.record, mode: .measurement, options: .duckOthers)
            try session.setActive(true, options: .notifyOthersOnDeactivation)
            let request = SFSpeechAudioBufferRecognitionRequest()
            request.shouldReportPartialResults = true
            let input = engine.inputNode
            input.installTap(onBus: 0, bufferSize: 1024, format: input.outputFormat(forBus: 0), block: Self.tap(request))
            engine.prepare()
            try engine.start()
            self.request = request
            task = recognizer.recognitionTask(with: request, resultHandler: Self.results { [weak self] text, finished in
                if let text { update(text) }
                if finished { self?.stop() }
            })
            state = .listening
        } catch {
            stop()
            state = .unavailable("Dictation couldn’t start: \(error.localizedDescription)")
        }
    }

    func stop() {
        if engine.isRunning { engine.stop() }
        engine.inputNode.removeTap(onBus: 0)
        request?.endAudio()
        task?.cancel()
        request = nil
        task = nil
        if state == .listening { state = .idle }
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    func clearMessage() { if case .unavailable = state { state = .idle } }

    private nonisolated static func authorize() async -> Bool {
        let speech = await withCheckedContinuation { continuation in
            SFSpeechRecognizer.requestAuthorization { continuation.resume(returning: $0 == .authorized) }
        }
        guard speech else { return false }
        return await AVAudioApplication.requestRecordPermission()
    }

    // Built outside the main actor: the audio and recognition callbacks run on background queues.
    private nonisolated static func tap(_ request: SFSpeechAudioBufferRecognitionRequest) -> AVAudioNodeTapBlock {
        { buffer, _ in request.append(buffer) }
    }

    private nonisolated static func results(
        _ handle: @escaping @MainActor (String?, Bool) -> Void
    ) -> @Sendable (SFSpeechRecognitionResult?, (any Error)?) -> Void {
        { result, error in
            let text = result?.bestTranscription.formattedString
            let finished = error != nil || result?.isFinal == true
            Task { @MainActor in handle(text, finished) }
        }
    }
}
