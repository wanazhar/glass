import AVFoundation
import Speech
import SwiftUI

/// Speaks a reply with the on-device synthesizer.
///
/// Marble never sends audio to a provider: the host only shapes the text (no
/// code fences, no markup), and the phone speaks it locally.
@MainActor
final class MarbleSpeaker {
    private let synthesizer = AVSpeechSynthesizer()

    func speak(_ text: String, rate: Float) {
        guard !text.isEmpty else { return }
        synthesizer.stopSpeaking(at: .immediate)
        let utterance = AVSpeechUtterance(string: text)
        // The host stores 0.2–0.8; AVSpeechUtterance wants a fraction of its
        // default rate, so a 0.5 preference speaks at the natural pace.
        let scale = max(0.5, min(1.7, Double(rate) / 0.5))
        utterance.rate = min(
            AVSpeechUtteranceMaximumSpeechRate,
            max(
                AVSpeechUtteranceMinimumSpeechRate,
                AVSpeechUtteranceDefaultSpeechRate * Float(scale)
            )
        )
        utterance.voice = AVSpeechSynthesisVoice(
            language: Locale.current.language.languageCode?.identifier ?? "en-US"
        )
        synthesizer.speak(utterance)
    }

    func stop() {
        synthesizer.stopSpeaking(at: .immediate)
    }
}

/// Turns speech into a message without leaving the device.
///
/// On-device recognition is required: if the device cannot recognize offline,
/// dictation stays off instead of quietly uploading audio.
@MainActor
final class MarbleDictation: ObservableObject {
    @Published private(set) var isRecording = false
    @Published private(set) var transcript = ""
    @Published var errorMessage: String?

    private let recognizer = SFSpeechRecognizer(locale: Locale.current)
    private let engine = AVAudioEngine()
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?

    func toggle() {
        if isRecording {
            stop()
        } else {
            begin()
        }
    }

    func begin() {
        guard !isRecording else { return }
        transcript = ""
        errorMessage = nil
        SFSpeechRecognizer.requestAuthorization { [weak self] status in
            Task { @MainActor in
                guard status == .authorized else {
                    self?.errorMessage = "Speech recognition needs permission in Settings."
                    return
                }
                self?.startEngine()
            }
        }
    }

    private func startEngine() {
        guard let recognizer, recognizer.isAvailable else {
            errorMessage = "Speech recognition is not available right now."
            return
        }
        guard recognizer.supportsOnDeviceRecognition else {
            errorMessage = "This device has no offline recognizer, so dictation stays off."
            return
        }
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.record, mode: .measurement, options: .duckOthers)
            try session.setActive(true, options: .notifyOthersOnDeactivation)

            let request = SFSpeechAudioBufferRecognitionRequest()
            request.shouldReportPartialResults = true
            request.requiresOnDeviceRecognition = true

            let input = engine.inputNode
            let format = input.outputFormat(forBus: 0)
            input.removeTap(onBus: 0)
            input.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in
                request.append(buffer)
            }
            engine.prepare()
            try engine.start()
            self.request = request
            isRecording = true
            task = recognizer.recognitionTask(with: request) { [weak self] result, error in
                Task { @MainActor in
                    if let result {
                        self?.transcript = result.bestTranscription.formattedString
                    }
                    if error != nil || (result?.isFinal ?? false) {
                        self?.stop()
                    }
                }
            }
        } catch {
            errorMessage = error.localizedDescription
            stop()
        }
    }

    func stop() {
        guard isRecording || engine.isRunning else { return }
        engine.stop()
        engine.inputNode.removeTap(onBus: 0)
        request?.endAudio()
        task?.cancel()
        request = nil
        task = nil
        isRecording = false
        try? AVAudioSession.sharedInstance().setActive(
            false,
            options: .notifyOthersOnDeactivation
        )
    }
}
