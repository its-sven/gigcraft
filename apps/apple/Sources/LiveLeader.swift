import CoreImage.CIFilterBuiltins
import Foundation
import Observation
import UIKit

/// Leading a live session from the iPad or iPhone (ADR-0013). The server and
/// the protocol run in the Rust core (`LeaderCore`); Swift announces the
/// session with Bonjour and pumps it a few times a second.
@Observable
final class LiveLeader: NSObject, NetServiceDelegate {
    private(set) var active = false
    private(set) var urls: [String] = []
    private(set) var roster: [LiveMember] = []
    private(set) var cue: (from: String, text: String, at: Date)?
    private(set) var error: String?

    private var core: LeaderCore?
    private var timer: Timer?
    private var service: NetService?
    private var started = Date()

    var readyCount: Int { roster.filter(\.ready).count }

    func start(name: String, library: Core) {
        stop()
        let leader = LeaderCore(name: name)
        do {
            urls = try leader.start()
        } catch {
            self.error = LibraryModel.describe(error)
            return
        }
        core = leader
        active = true
        started = Date()
        announce(name: name.isEmpty ? String(localized: "Band leader") : name, port: Int32(leader.port()))
        timer = Timer.scheduledTimer(withTimeInterval: 0.15, repeats: true) { [weak self] _ in self?.pump(library) }
    }

    func stop() {
        timer?.invalidate()
        timer = nil
        service?.stop()
        service = nil
        core?.stop()
        core = nil
        active = false
        urls = []
        roster = []
        cue = nil
    }

    func sendCue(_ text: String) {
        guard let core, !text.trimmingCharacters(in: .whitespaces).isEmpty else { return }
        handle(core.cue(text: text, nowMs: nowMs))
    }

    private var nowMs: UInt64 { UInt64(max(0, Date().timeIntervalSince(started)) * 1000) }

    private func pump(_ library: Core) {
        guard let core else { return }
        handle(core.pump(core: library, nowMs: nowMs))
    }

    private static let decoder: JSONDecoder = {
        let d = JSONDecoder()
        d.keyDecodingStrategy = .convertFromSnakeCase
        return d
    }()

    private func handle(_ json: String) {
        guard json != "[]", let core,
              let events = (try? JSONSerialization.jsonObject(with: Data(json.utf8))) as? [[String: Any]] else { return }
        for e in events {
            switch e["type"] as? String {
            case "joined", "left", "ready":
                roster = (try? Self.decoder.decode([LiveMember].self, from: Data(core.rosterJson().utf8))) ?? []
            case "cue":
                cue = (e["from"] as? String ?? "", e["text"] as? String ?? "", Date())
            default:
                break
            }
        }
    }

    /// Bonjour through the system's mDNS service (no multicast entitlement needed).
    private func announce(name: String, port: Int32) {
        let service = NetService(domain: "local.", type: "_gigcraft._tcp.", name: name, port: port)
        let address = urls.first.map { $0.replacingOccurrences(of: "http://", with: "").trimmingCharacters(in: CharacterSet(charactersIn: "/")) } ?? ""
        let txt: [String: Data] = ["protocol": Data("1".utf8), "name": Data(name.utf8), "address": Data(address.utf8)]
        service.setTXTRecord(NetService.data(fromTXTRecord: txt))
        service.delegate = self
        service.publish()
        self.service = service
    }

    /// A QR code for the first address.
    func qrImage() -> UIImage? {
        guard let url = urls.first else { return nil }
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(url.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage?.transformed(by: CGAffineTransform(scaleX: 10, y: 10)),
              let cg = CIContext().createCGImage(output, from: output.extent) else { return nil }
        return UIImage(cgImage: cg)
    }
}
