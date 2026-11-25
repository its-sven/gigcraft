import Foundation
import Network
import Observation

/// Finds GigCraft live leaders on the local network (Bonjour `_gigcraft._tcp`).
/// The leader puts its name and address into the TXT record, so no extra
/// connection is needed to resolve it.
@Observable
final class LeaderBrowser {
    struct Leader: Identifiable, Hashable {
        var id: String { address }
        let name: String
        let address: String
    }

    private(set) var leaders: [Leader] = []
    private var browser: NWBrowser?

    func start() {
        guard browser == nil else { return }
        let params = NWParameters()
        params.includePeerToPeer = true
        let browser = NWBrowser(for: .bonjourWithTXTRecord(type: "_gigcraft._tcp", domain: nil), using: params)
        browser.browseResultsChangedHandler = { [weak self] results, _ in
            let found: [Leader] = results.compactMap { result in
                guard case .bonjour(let txt) = result.metadata,
                      let address = txt["address"], !address.isEmpty else { return nil }
                var name = txt["name"] ?? ""
                if name.isEmpty, case .service(let service, _, _, _) = result.endpoint { name = service }
                return Leader(name: name, address: address)
            }
            // The same leader shows up once per network interface: keep one per address.
            var byAddress: [String: Leader] = [:]
            for leader in found where byAddress[leader.address] == nil { byAddress[leader.address] = leader }
            let list = byAddress.values.sorted { ($0.name, $0.address) < ($1.name, $1.address) }
            DispatchQueue.main.async { self?.leaders = list }
        }
        browser.start(queue: .main)
        self.browser = browser
    }

    func stop() {
        browser?.cancel()
        browser = nil
    }
}
