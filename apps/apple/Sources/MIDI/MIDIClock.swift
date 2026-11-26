import CoreMIDI
import Darwin

/// MIDI clock for one destination: 24 ticks per quarter note, scheduled up to
/// 100 ms ahead with CoreMIDI timestamps, so the beat stays exact even when
/// this queue wakes up late.
final class MIDIClock {
    private let queue = DispatchQueue(label: "gigcraft.midi.clock", qos: .userInteractive)
    private let port: MIDIPortRef
    private let dest: MIDIEndpointRef
    private var bpm: Double
    private var next: UInt64 = 0
    private var timer: DispatchSourceTimer?

    private static let timebase: mach_timebase_info_data_t = {
        var info = mach_timebase_info_data_t()
        mach_timebase_info(&info)
        return info
    }()

    private static func hostTicks(seconds: Double) -> UInt64 {
        UInt64(seconds * 1_000_000_000 * Double(timebase.denom) / Double(timebase.numer))
    }

    init(port: MIDIPortRef, dest: MIDIEndpointRef, bpm: Double) {
        self.port = port
        self.dest = dest
        self.bpm = bpm
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now(), repeating: .milliseconds(25), leeway: .milliseconds(5))
        timer.setEventHandler { [weak self] in self?.fill() }
        timer.resume()
        self.timer = timer
    }

    func set(bpm: Double) {
        queue.async { self.bpm = min(400, max(20, bpm)) }
    }

    func stop() {
        timer?.cancel()
        timer = nil
    }

    deinit { stop() }

    private func fill() {
        let now = mach_absolute_time()
        let horizon = now + Self.hostTicks(seconds: 0.1)
        let interval = Self.hostTicks(seconds: 60 / (bpm * 24))
        if next < now { next = now }
        while next < horizon {
            send(at: next)
            next += interval
        }
    }

    private func send(at time: MIDITimeStamp) {
        guard var word = UMP.word(forShortMessage: [0xF8]) else { return }
        var list = MIDIEventList()
        let packet = MIDIEventListInit(&list, ._1_0)
        MIDIEventListAdd(&list, MemoryLayout<MIDIEventList>.size, packet, time, 1, &word)
        MIDISendEventList(port, dest, &list)
    }
}
