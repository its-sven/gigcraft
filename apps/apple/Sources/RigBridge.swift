import Foundation
import Observation

/// Connects the Rust rig (devices, Play Acoustic driver, MIDI Learn) with CoreMIDI.
/// The rig decides; this class only moves bytes, runs the timer and saves the config.
@Observable
final class RigBridge {
    private static let configKey = "gigcraft.rig"
    private let rig: RigCore
    private let midi = MIDIManager.shared
    private var tokens: [UUID] = []
    private var timer: Timer?
    private var ports: [MIDIPortInfo] = []

    /// Status line per device ("Play Acoustic: preset 4 Bluesy Drive").
    private(set) var status: [String] = []
    private(set) var log: [String] = []
    /// Stage actions from MIDI input (footswitches); the model performs them.
    var onAction: ((String) -> Void)?
    /// Which action the next learned trigger gets.
    var learningFor: String?

    init() {
        rig = RigCore(configJson: UserDefaults.standard.string(forKey: Self.configKey) ?? "")
        tokens.append(midi.addPortListener { [weak self] ports in self?.portsChanged(ports) })
        tokens.append(midi.addMessageListener { [weak self] msgs in self?.received(msgs) })
        portsChanged(midi.ports)
        refreshStatus()
    }

    private var nowMs: UInt64 { UInt64(HostClock.nowMs) }

    var usesSimulatedPedal: Bool { rig.configJson().contains("\"simulated\":true") }

    private func portsChanged(_ list: [MIDIPortInfo]) {
        ports = list
        handle(rig.portsChanged(outputs: list.filter { $0.type == .output }.map(\.name), nowMs: nowMs))
    }

    private func received(_ msgs: [ReceivedMIDIMessage]) {
        for m in msgs {
            let name = ports.first { $0.id == m.portID }?.name ?? m.portID
            handle(rig.onInput(port: name, bytes: Data(m.bytes), nowMs: nowMs))
        }
    }

    /// The `actions` of a `send_part` perform effect.
    func sendPart(_ actions: [[String: Any]]) {
        guard let data = try? JSONSerialization.data(withJSONObject: actions) else { return }
        handle(rig.sendPart(actionsJson: String(decoding: data, as: UTF8.self), nowMs: nowMs))
    }

    func panic() { handle(rig.panic(nowMs: nowMs)) }

    func learn(_ action: String?) {
        learningFor = action
        rig.learn(on: action != nil)
    }

    /// Use the simulated pedal (no hardware) or look for a real Play Acoustic.
    func setSimulated(_ on: Bool) {
        var cfg = (try? JSONSerialization.jsonObject(with: Data(rig.configJson().utf8))) as? [String: Any] ?? [:]
        var devices = cfg["devices"] as? [[String: Any]] ?? []
        if devices.isEmpty {
            devices = [["name": "Play Acoustic", "profile": "tc-helicon-play-acoustic"]]
        }
        for i in devices.indices where (devices[i]["profile"] as? String) == "tc-helicon-play-acoustic" {
            devices[i]["simulated"] = on
        }
        cfg["devices"] = devices
        guard let data = try? JSONSerialization.data(withJSONObject: cfg),
              let outs = try? rig.setConfigJson(configJson: String(decoding: data, as: UTF8.self), nowMs: nowMs) else { return }
        handle(outs)
        portsChanged(ports)
        save()
    }

    private func save() {
        UserDefaults.standard.set(rig.configJson(), forKey: Self.configKey)
    }

    private func handle(_ json: String) {
        let outs = (try? JSONSerialization.jsonObject(with: Data(json.utf8))) as? [[String: Any]] ?? []
        for o in outs {
            switch o["type"] as? String {
            case "send":
                guard let port = o["port"] as? String, let bytes = o["bytes"] as? [Int],
                      let id = ports.first(where: { $0.type == .output && $0.name == port })?.id else { continue }
                midi.send(bytes.map { UInt8(truncatingIfNeeded: $0) }, to: id)
            case "notice":
                append("\(o["device"] as? String ?? ""): \(Self.localized(o["text"] as? String ?? ""))")
            case "action":
                if let a = o["action"] as? String { onAction?(a) }
            case "learned":
                if let action = learningFor, let trigger = o["trigger"],
                   let data = try? JSONSerialization.data(withJSONObject: trigger),
                   let more = try? rig.addMapping(triggerJson: String(decoding: data, as: UTF8.self), action: action) {
                    learningFor = nil
                    append(String(localized: "Learned: \(Self.actionName(action))"))
                    handle(more)
                }
            case "config_changed":
                save()
            default:
                break
            }
        }
        refreshStatus()
        scheduleTick()
    }

    private func append(_ line: String) {
        log.append(line)
        if log.count > 100 { log.removeFirst(log.count - 100) }
    }

    private func refreshStatus() {
        let list = (try? JSONSerialization.jsonObject(with: Data(rig.statusJson().utf8))) as? [[String: Any]] ?? []
        status = list.map { d in
            let what = (d["current"] as? String).map { String(localized: "preset \($0)") } ?? Self.state(d["state"] as? String ?? "")
            return "\(d["name"] as? String ?? ""): \(what)" + ((d["busy"] as? Bool) == true ? " …" : "")
        }
    }

    /// Text from the core in the user's language, if the string catalog has it.
    static func localized(_ text: String) -> String {
        Bundle.main.localizedString(forKey: text, value: text, table: nil)
    }

    /// A device state from the core ("not connected", "Play Acoustic ready"…).
    static func state(_ text: String) -> String {
        if text.hasSuffix(" ready") {
            let model = String(text.dropLast(" ready".count))
            return String(localized: "\(model) ready")
        }
        return localized(text)
    }

    /// A perform action's name ("next_song" → "Next song").
    static func actionName(_ action: String) -> String {
        switch action {
        case "next": String(localized: "Next")
        case "previous": String(localized: "Previous")
        case "next_song": String(localized: "Next song")
        case "previous_song": String(localized: "Previous song")
        default: action.replacingOccurrences(of: "_", with: " ")
        }
    }

    /// Drive the rig's timeouts and delayed messages.
    private func scheduleTick() {
        timer?.invalidate()
        guard let deadline = rig.nextDeadline() else { return }
        let wait = max(0.005, Double(Int64(deadline) - Int64(nowMs)) / 1000)
        timer = Timer.scheduledTimer(withTimeInterval: min(wait, 0.5), repeats: false) { [weak self] _ in
            guard let self else { return }
            self.handle(self.rig.tick(nowMs: self.nowMs))
        }
    }
}
