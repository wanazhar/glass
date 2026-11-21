import AVFoundation
import Testing
@testable import Glass

@Suite struct DictationLevelTests {
    private func buffer(_ value: Float) -> AVAudioPCMBuffer {
        let format = AVAudioFormat(standardFormatWithSampleRate: 44_100, channels: 1)!
        let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: 512)!
        buffer.frameLength = 512
        for i in 0..<512 { buffer.floatChannelData![0][i] = value }
        return buffer
    }

    @Test func silenceReadsAsZeroAndFullScaleAsOne() {
        #expect(Dictation.rms(buffer(0)) == 0)
        #expect(Dictation.rms(buffer(1)) == 1)
        let speech = Dictation.rms(buffer(0.05))
        #expect(speech > 0.2 && speech < 0.9)
    }
}
