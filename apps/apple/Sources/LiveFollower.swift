import Foundation
import Observation

/// One item of the leader's running order (`gigcraft_live::Slot`).
struct LiveSlot: Decodable, Equatable {
    var type: String
    var song: String?
    var title: String?
    var artist: String?
    var key: String?
    var tempoBpm: Double?
    var timeSignature: String?
    var gigNotes: String?
    var partNames: [String]?
    var number: Int?
    var chartSource: String?
    var semitones: Int?
    var label: String?
    var minutes: Int?
}

/// Where the leader's show is (`gigcraft_live::Snapshot`).
struct LiveSnapshot: Decodable, Equatable {
    var setlistName: String
    var index: Int
    var count: Int
    var part: Int
    var current: LiveSlot?
    var next: LiveSlot?
}

struct LiveMember: Decodable, Identifiable, Equatable {
    var id: Int
    var name: String
    var instrument: String
    var ready: Bool
}

/// Following a band leader on the local network. The protocol state lives in
/// the Rust core (`FollowerCore`); this class moves the WebSocket messages.
@Observable
final class LiveFollower {
    private(set) var active = false
    private(set) var connected = false
    private(set) var status = ""
    private(set) var leader = ""
    private(set) var snapshot: LiveSnapshot?
    private(set) var roster: [LiveMember] = []
    private(set) var ready = false
    private(set) var cue: (from: String, text: String, at: Date)?
    /// The band's mark, if one is set.
    private(set) var mark: BandMark?
    /// A new mark: scroll there once.
    var jump: BandMark?
    /// Where the leader's chart is.
    private(set) var leaderScroll: LiveScroll?

    /// Called when the leader moves to another part (send own presets).
    var onPartChanged: (() -> Void)?

    private var core: FollowerCore?
    private var task: URLSessionWebSocketTask?
    private var address = ""
    private var retry: Timer?

    private static let decoder: JSONDecoder = {
        let d = JSONDecoder()
        d.keyDecodingStrategy = .convertFromSnakeCase
        return d
    }()

    func join(address: String, name: String, instrument: String) -> Bool {
        guard liveWsUrl(address: address) != nil else { return false }
        leave()
        self.address = address
        core = FollowerCore(name: name, instrument: instrument)
        active = true
        connect()
        return true
    }

    func leave() {
        retry?.invalidate()
        task?.cancel(with: .goingAway, reason: nil)
        task = nil
        active = false
        connected = false
        snapshot = nil
        roster = []
        cue = nil
        mark = nil
        leaderScroll = nil
    }

    func toggleReady() {
        guard let core else { return }
        send(core.setReady(ready: !core.ready()))
        ready = core.ready()
    }

    func sendCue(_ text: String) {
        guard let core, !text.trimmingCharacters(in: .whitespaces).isEmpty else { return }
        send(core.cueMessage(text: text))
    }

    /// Mark a place for the whole band (`nil`: clear the mark).
    func setMark(song: String?, spot: ChartSpot = ChartSpot(block: 0, fraction: 0)) {
        guard let core else { return }
        send(core.markMessage(songId: song ?? "", block: UInt32(max(0, spot.block)), fraction: Float(spot.fraction), text: ""))
    }



    private func connect() {
        guard active, let urlText = liveWsUrl(address: address), let url = URL(string: urlText), let core else { return }
        status = String(localized: "connecting…")
        let task = URLSession.shared.webSocketTask(with: url)
        task.maximumMessageSize = 4 << 20
        self.task = task
        task.resume()
        send(core.joinMessage())
        receive(task)
    }

    private func receive(_ task: URLSessionWebSocketTask) {
        task.receive { [weak self] result in
            DispatchQueue.main.async {
                guard let self, self.task === task else { return }
                switch result {
                case .success(.string(let text)):
                    self.connected = true
                    self.handle(text)
                    self.receive(task)
                case .success:
                    self.receive(task)
                case .failure(let error):
                    self.connected = false
                    self.status = String(localized: "Disconnected – reconnecting…")
                    if self.active {
                        _ = error
                        self.retry = Timer.scheduledTimer(withTimeInterval: 2, repeats: false) { [weak self] _ in self?.connect() }
                    }
                }
            }
        }
    }

    private func send(_ text: String) {
        task?.send(.string(text)) { _ in }
    }

    private func handle(_ text: String) {
        guard let core, let json = try? core.onText(text: text),
              let events = (try? JSONSerialization.jsonObject(with: Data(json.utf8))) as? [[String: Any]] else { return }
        for e in events {
            switch e["type"] as? String {
            case "joined":
                leader = e["leader"] as? String ?? ""
                status = String(localized: "connected")
                mark = core.markJson().flatMap { try? JSONDecoder().decode(BandMark.self, from: Data($0.utf8)) }
            case "marked":
                mark = core.markJson().flatMap { try? JSONDecoder().decode(BandMark.self, from: Data($0.utf8)) }
                if let mark { jump = mark }
            case "scrolled":
                leaderScroll = core.leaderScrollJson().flatMap { try? JSONDecoder().decode(LiveScroll.self, from: Data($0.utf8)) }
            case "moved":
                snapshot = core.snapshotJson().flatMap { try? Self.decoder.decode(LiveSnapshot.self, from: Data($0.utf8)) }
                ready = core.ready()
                if (e["part_changed"] as? Bool) == true { onPartChanged?() }
            case "roster":
                roster = (try? Self.decoder.decode([LiveMember].self, from: Data(core.rosterJson().utf8))) ?? []
                ready = core.ready()
            case "cue":
                cue = (e["from"] as? String ?? "", e["text"] as? String ?? "", Date())
            case "ended":
                let reason = e["reason"] as? String ?? ""
                status = String(localized: "Session ended: \(reason)")
                active = false
                task?.cancel(with: .normalClosure, reason: nil)
            default:
                break
            }
        }
    }
}
