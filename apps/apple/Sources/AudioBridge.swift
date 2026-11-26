import AVFoundation
import Foundation
import Observation
#if canImport(gigcraft_ffiFFI)
import gigcraft_ffiFFI
#endif

/// Backing tracks and click on iOS (ADR-0014): the Rust engine renders in an
/// `AVAudioSourceNode`; loading and decoding run on a background queue.
@Observable
final class AudioBridge {
    /// Playing, counting in, position, length – refreshed while performing.
    private(set) var status: AudioStatus?
    /// The song the engine has loaded and is ready to play.
    private(set) var readySong: String?
    /// A song being loaded.
    private(set) var loadingSong: String?
    private(set) var error: String?

    private var audio: AudioCore?
    private var engine: AVAudioEngine?
    private var handle: UInt64 = 0
    private var scratch: UnsafeMutablePointer<Float>?
    private static let maxFrames = 4096
    private let work = DispatchQueue(label: "gigcraft.audio.load", qos: .userInitiated)
    private var timer: Timer?
    private var wanted: String?

    /// Output channels in use (2, or more with an audio interface).
    private(set) var channels = 2

    /// Start the audio output (once).
    @discardableResult
    func start() -> Bool {
        if engine != nil { return true }
        do {
            let session = AVAudioSession.sharedInstance()
            // Backing tracks must play with the ring/silent switch on.
            try session.setCategory(.playback, mode: .default)
            try session.setActive(true)
            let rate = session.sampleRate > 0 ? session.sampleRate : 48_000
            let audio = AudioCore(sampleRate: UInt32(rate), maxFrames: UInt32(Self.maxFrames))
            self.audio = audio
            self.handle = audio.takeRenderer()
            try buildOutput()
            NotificationCenter.default.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: .main) { [weak self] note in
                guard let raw = note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
                      AVAudioSession.InterruptionType(rawValue: raw) == .ended else { return }
                try? self?.engine?.start()
            }
            // An audio interface plugged in or out: rebuild for its outputs.
            NotificationCenter.default.addObserver(forName: AVAudioSession.routeChangeNotification, object: nil, queue: .main) { [weak self] _ in
                try? self?.buildOutput()
            }
            return true
        } catch {
            self.error = String(localized: "Audio output: \(error.localizedDescription)")
            return false
        }
    }

    /// (Re)build the output for the current route: stereo through the mixer,
    /// or every output of an interface (bus 1 → outputs 1/2, bus 2 → 3/4…).
    private func buildOutput() throws {
        engine?.stop()
        engine = nil
        let session = AVAudioSession.sharedInstance()
        let wanted = min(8, max(2, session.maximumOutputNumberOfChannels))
        try? session.setPreferredOutputNumberOfChannels(wanted)
        let channels = max(2, min(8, session.outputNumberOfChannels))
        let rate = session.sampleRate > 0 ? session.sampleRate : 48_000
        // The old node has stopped rendering: its buffer can go.
        scratch?.deallocate()
        let scratch = UnsafeMutablePointer<Float>.allocate(capacity: Self.maxFrames * channels)
        scratch.initialize(repeating: 0, count: Self.maxFrames * channels)
        self.scratch = scratch
        let format: AVAudioFormat?
        if channels == 2 {
            format = AVAudioFormat(standardFormatWithSampleRate: rate, channels: 2)
        } else {
            let layout = AVAudioChannelLayout(layoutTag: kAudioChannelLayoutTag_DiscreteInOrder | UInt32(channels))
            format = layout.map { AVAudioFormat(standardFormatWithSampleRate: rate, channelLayout: $0) }
        }
        guard let format else { return }
        let renderer = UnsafeMutableRawPointer(bitPattern: UInt(handle))
        let maxFrames = Self.maxFrames
        let chans = channels
        // The render block runs on the real-time thread: no allocation, no locks.
        let node = AVAudioSourceNode(format: format) { _, _, frameCount, bufferList -> OSStatus in
            let buffers = UnsafeMutableAudioBufferListPointer(bufferList)
            let total = Int(frameCount)
            var done = 0
            while done < total {
                let n = min(maxFrames, total - done)
                gigcraft_audio_render(renderer, scratch, UInt32(n), UInt32(chans))
                for ch in 0..<min(chans, buffers.count) {
                    guard let data = buffers[ch].mData?.assumingMemoryBound(to: Float.self) else { continue }
                    for i in 0..<n { data[done + i] = scratch[i * chans + ch] }
                }
                done += n
            }
            return noErr
        }
        let engine = AVAudioEngine()
        engine.attach(node)
        if channels == 2 {
            engine.connect(node, to: engine.mainMixerNode, format: format)
        } else {
            // Straight to the interface's outputs, no stereo downmix.
            engine.connect(node, to: engine.outputNode, format: format)
        }
        try engine.start()
        self.engine = engine
        self.channels = channels
    }

    /// Follow the show: load the current song, decode the next one.
    func follow(core: Core, current: String?, next: String?) {
        guard let current else { return }
        guard let audio = audio ?? (start() ? audio : nil) else { return }
        guard audio.hasAudio(core: core, songId: current) else {
            if status?.playing == true { try? audio.stop() }
            readySong = nil
            wanted = nil
            return
        }
        guard wanted != current else { return }
        wanted = current
        loadingSong = current
        readySong = nil
        work.async { [weak self] in
            var problem: String?
            do { _ = try audio.loadSong(core: core, songId: current) } catch { problem = LibraryModel.describe(error) }
            if let next { try? audio.preload(core: core, songId: next) }
            DispatchQueue.main.async {
                guard let self, self.wanted == current else { return }
                self.loadingSong = nil
                if let problem { self.error = String(localized: "Backing track: \(problem)") } else { self.readySong = current }
                self.refresh()
            }
        }
        startPolling()
    }

    func toggle(core: Core) {
        guard let audio, readySong != nil else { return }
        try? audio.toggle(core: core)
        refresh()
    }

    func stop() {
        try? audio?.stop()
        refresh()
    }

    /// Leaving perform mode.
    func unload() {
        audio?.unload()
        wanted = nil
        readySong = nil
        loadingSong = nil
        timer?.invalidate()
        timer = nil
        status = nil
    }

    /// Called when playback starts or stops (MIDI clock Start/Stop).
    var onPlayingChanged: ((Bool) -> Void)?
    /// Called with the track position while playing (parts follow the track).
    var onPosition: ((Double) -> Void)?

    private func refresh() {
        let was = status?.playing ?? false
        status = audio?.status()
        let now = status?.playing ?? false
        if now != was { onPlayingChanged?(now) }
        if let s = status, s.playing, !s.countingIn { onPosition?(s.positionSecs) }
    }

    private func startPolling() {
        guard timer == nil else { return }
        timer = Timer.scheduledTimer(withTimeInterval: 0.2, repeats: true) { [weak self] _ in self?.refresh() }
    }

    deinit {
        engine?.stop()
        if handle != 0 { gigcraft_audio_free_renderer(UnsafeMutableRawPointer(bitPattern: UInt(handle))) }
        scratch?.deallocate()
    }
}
