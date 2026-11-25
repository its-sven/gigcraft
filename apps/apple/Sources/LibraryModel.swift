import Foundation
import Observation

/// A setlist entry as returned by `setlist.get` (decoded from the core's JSON).
struct SetlistEntry: Identifiable, Hashable {
    let id: Int
    let isBreak: Bool
    let title: String
    let songId: String?
    let key: String?
    let notes: String
    let optional: Bool
    let missing: Bool
    let minutes: Int
}

struct SetlistSet: Identifiable, Hashable {
    let id: Int
    let name: String
    let duration: String
    let entries: [SetlistEntry]
}

struct SetlistDetail: Hashable {
    let name: String
    let date: String?
    let notes: String
    let total: String
    let sets: [SetlistSet]
}

struct SetlistRow: Identifiable, Hashable {
    let id: String
    let name: String
    let date: String?
    let total: String
}

/// The app's view of one GigCraft library, backed by the Rust core.
@Observable
final class LibraryModel {
    enum Section: String, CaseIterable, Identifiable {
        case songs = "Songs"
        case setlists = "Setlists"
        var id: String { rawValue }
    }

    private(set) var core: Core?
    private(set) var libraryName = ""
    private(set) var errorMessage: String?
    var section: Section = .songs
    var query = "" { didSet { refreshSongs() } }
    private(set) var songs: [SongSummary] = []
    private(set) var setlists: [SetlistRow] = []
    var selectedSong: String?
    var selectedSetlist: String?
    var fontSize: Double = 20

    /// The library lives in the app's Documents folder (visible in the Files app).
    static var libraryURL: URL {
        URL.documentsDirectory.appending(path: "GigCraft Library", directoryHint: .isDirectory)
    }

    func load() {
        guard core == nil else { return }
        do {
            let core = try Core.openOrCreateDemo(path: Self.libraryURL.path(percentEncoded: false), name: "GigCraft Demo")
            self.core = core
            libraryName = core.libraryName()
            refreshSongs()
            refreshSetlists()
            applyLaunchSelection()
        } catch {
            errorMessage = "\(error)"
        }
    }

    func refreshSongs() {
        songs = core?.songs(query: query) ?? []
    }

    func refreshSetlists() {
        guard let json = run("setlist.list"), let rows = json["result"] as? [[String: Any]] else { return }
        setlists = rows.compactMap { r in
            guard let id = r["id"] as? String, let name = r["name"] as? String else { return nil }
            return SetlistRow(id: id, name: name, date: r["date"] as? String, total: r["total"] as? String ?? "")
        }
    }

    func song(_ id: String) -> SongSummary? { songs.first { $0.id == id } ?? core?.songs(query: "").first { $0.id == id } }

    func chart(_ id: String, transpose: Int8) -> Chart? {
        try? core?.chart(songId: id, semitones: transpose)
    }

    func setlist(_ id: String) -> SetlistDetail? {
        guard let json = run("setlist.get", ["id": id]), let d = json["result"] as? [String: Any] else { return nil }
        let sets = (d["sets"] as? [[String: Any]] ?? []).enumerated().map { si, s in
            let entries = (s["entries"] as? [[String: Any]] ?? []).enumerated().map { ei, e in
                SetlistEntry(
                    id: ei,
                    isBreak: (e["type"] as? String) == "break",
                    title: (e["title"] as? String) ?? ((e["label"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? "Break"),
                    songId: e["song"] as? String,
                    key: e["key"] as? String,
                    notes: e["notes"] as? String ?? "",
                    optional: e["optional"] as? Bool ?? false,
                    missing: e["missing"] as? Bool ?? false,
                    minutes: e["minutes"] as? Int ?? 0
                )
            }
            return SetlistSet(id: si, name: s["name"] as? String ?? "", duration: s["duration"] as? String ?? "", entries: entries)
        }
        return SetlistDetail(
            name: d["name"] as? String ?? "",
            date: d["date"] as? String,
            notes: d["notes"] as? String ?? "",
            total: d["total"] as? String ?? "",
            sets: sets
        )
    }

    /// Run a core command; errors go to `errorMessage`.
    @discardableResult
    func run(_ command: String, _ params: [String: Any] = [:]) -> [String: Any]? {
        guard let core else { return nil }
        do {
            let data = try JSONSerialization.data(withJSONObject: params)
            let out = try core.run(command: command, paramsJson: String(decoding: data, as: UTF8.self))
            return try JSONSerialization.jsonObject(with: Data(out.utf8)) as? [String: Any]
        } catch {
            errorMessage = "\(error)"
            return nil
        }
    }

    /// `-gcSong <title>` / `-gcSetlist <name>` launch arguments (for screenshots and tests).
    private func applyLaunchSelection() {
        let defaults = UserDefaults.standard
        if let want = defaults.string(forKey: "gcSong")?.lowercased() {
            section = .songs
            selectedSong = songs.first { $0.title.lowercased().contains(want) }?.id
        }
        if let want = defaults.string(forKey: "gcSetlist")?.lowercased() {
            section = .setlists
            selectedSetlist = setlists.first { $0.name.lowercased().contains(want) }?.id
        }
    }
}
