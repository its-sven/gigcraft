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
            let handle = audio.takeRenderer()
            let scratch = UnsafeMutablePointer<Float>.allocate(capacity: Self.maxFrames * 2)
            scratch.initialize(repeating: 0, count: Self.maxFrames * 2)
            guard let format = AVAudioFormat(standardFormatWithSampleRate: rate, channels: 2) else { return false }
            let renderer = UnsafeMutableRawPointer(bitPattern: UInt(handle))
            let maxFrames = Self.maxFrames
            // The render block runs on the real-time thread: no allocation, no locks.
            let node = AVAudioSourceNode(format: format) { _, _, frameCount, bufferList -> OSStatus in
                let buffers = UnsafeMutableAudioBufferListPointer(bufferList)
                let total = Int(frameCount)
                var done = 0
                while done < total {
                    let n = min(maxFrames, total - done)
                    gigcraft_audio_render(renderer, scratch, UInt32(n), 2)
                    for ch in 0..<min(2, buffers.count) {
                        guard let data = buffers[ch].mData?.assumingMemoryBound(to: Float.self) else { continue }
                        for i in 0..<n { data[done + i] = scratch[i * 2 + ch] }
                    }
                    done += n
                }
                return noErr
            }
            let engine = AVAudioEngine()
            engine.attach(node)
            engine.connect(node, to: engine.mainMixerNode, format: format)
            try engine.start()
            self.audio = audio
            self.engine = engine
            self.handle = handle
            self.scratch = scratch
            NotificationCenter.default.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: .main) { [weak self] note in
                guard let raw = note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt,
                      AVAudioSession.InterruptionType(rawValue: raw) == .ended else { return }
                try? self?.engine?.start()
            }
            return true
        } catch {
            self.error = String(localized: "Audio output: \(error.localizedDescription)")
            return false
        }
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
            do { _ = try audio.loadSong(core: core, songId: current) } catch { problem = error.localizedDescription }
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

    private func refresh() {
        status = audio?.status()
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
