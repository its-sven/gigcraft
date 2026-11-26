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
        var title: LocalizedStringResource {
            switch self {
            case .songs: "Songs"
            case .setlists: "Setlists"
            }
        }
    }

    private(set) var core: Core?

    /// The app's model, for scenes SwiftUI does not create (external display).
    nonisolated(unsafe) static weak var current: LibraryModel?

    init() {
        Self.current = self
    }
    /// MIDI devices and pedals.
    let rig = RigBridge()
    /// Following a band leader (live session).
    let live = LiveFollower()
    /// Backing tracks and click.
    let audio = AudioBridge()
    private(set) var libraryName = ""
    private(set) var errorMessage: String?
    var section: Section = .songs
    var query = "" { didSet { refreshSongs() } }
    private(set) var songs: [SongSummary] = []
    private(set) var setlists: [SetlistRow] = []
    var selectedSong: String?
    var selectedSetlist: String?
    var fontSize: Double = 20
    /// Open the selected song or setlist in edit mode (launch argument `-gcEdit YES`).
    var launchEdit = false
    /// A song the UI should open in the editor.
    var editRequest: String?

    /// The library lives in the app's Documents folder (visible in the Files app).
    static var libraryURL: URL {
        URL.documentsDirectory.appending(path: "GigCraft Library", directoryHint: .isDirectory)
    }

    private static let bookmarkKey = "gigcraft.libraryBookmark"
    private var scopedURL: URL?

    /// Where the library is: a folder the user chose (iCloud Drive, Nextcloud…)
    /// or the app's own Documents folder.
    private(set) var libraryLocation = ""

    func load() {
        guard core == nil else { return }
        rig.onAction = { [weak self] action in
            guard let self, self.performState != nil else { return }
            self.perform(action)
        }
        live.onPartChanged = { [weak self] in self?.sendOwnPresetsForLive() }
        if let url = resolveBookmark(), open(url, createDemo: false) {
            applyLaunchSelection()
            return
        }
        _ = open(Self.libraryURL, createDemo: true)
        applyLaunchSelection()
    }

    /// Open (or create) the library in `url`.
    @discardableResult
    private func open(_ url: URL, createDemo: Bool) -> Bool {
        do {
            let path = url.path(percentEncoded: false)
            let core = createDemo ? try Core.openOrCreateDemo(path: path, name: "GigCraft Demo")
                                  : try Core.openOrCreate(path: path, name: url.lastPathComponent)
            self.core = core
            libraryName = core.libraryName()
            libraryLocation = path
            selectedSong = nil
            selectedSetlist = nil
            errorMessage = nil
            refreshSongs()
            refreshSetlists()
            bump()
            return true
        } catch {
            errorMessage = "\(error)"
            return false
        }
    }

    /// Use a folder picked in the Files app as the library (kept across launches).
    func useFolder(_ url: URL) {
        scopedURL?.stopAccessingSecurityScopedResource()
        guard url.startAccessingSecurityScopedResource() else {
            errorMessage = String(localized: "No access to that folder.")
            return
        }
        scopedURL = url
        if let data = try? url.bookmarkData(options: [], includingResourceValuesForKeys: nil, relativeTo: nil) {
            UserDefaults.standard.set(data, forKey: Self.bookmarkKey)
        }
        open(url, createDemo: false)
    }

    /// Go back to the library in the app's Documents folder.
    func useAppFolder() {
        scopedURL?.stopAccessingSecurityScopedResource()
        scopedURL = nil
        UserDefaults.standard.removeObject(forKey: Self.bookmarkKey)
        open(Self.libraryURL, createDemo: true)
    }

    private func resolveBookmark() -> URL? {
        guard let data = UserDefaults.standard.data(forKey: Self.bookmarkKey) else { return nil }
        var stale = false
        guard let url = try? URL(resolvingBookmarkData: data, options: [], relativeTo: nil, bookmarkDataIsStale: &stale),
              url.startAccessingSecurityScopedResource() else { return nil }
        scopedURL = url
        if stale, let fresh = try? url.bookmarkData(options: [], includingResourceValuesForKeys: nil, relativeTo: nil) {
            UserDefaults.standard.set(fresh, forKey: Self.bookmarkKey)
        }
        return url
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
                    title: (e["title"] as? String) ?? ((e["label"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? String(localized: "Break")),
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

    // MARK: Editing

    /// The full song record and chart (`song.get`).
    func songRecord(_ id: String) -> (song: [String: Any], chart: String)? {
        guard let out = run("song.get", ["id": id]), let r = out["result"] as? [String: Any],
              let song = r["song"] as? [String: Any] else { return nil }
        return (song, r["chart"] as? String ?? "")
    }

    /// Save fields and chart; returns validation issues (empty when all is well), nil on error.
    @discardableResult
    func updateSong(_ id: String, fields: [String: Any], chart: String?) -> [String]? {
        var params: [String: Any] = ["id": id, "fields": fields]
        if let chart { params["chart"] = chart }
        guard let out = run("song.update", params) else { return nil }
        refreshSongs()
        let issues = ((out["result"] as? [String: Any])?["issues"] as? [[String: Any]]) ?? []
        return issues.map { "\($0["field"] as? String ?? ""): \($0["message"] as? String ?? "")" }
    }

    func createSong() -> String? {
        let title = String(localized: "New song")
        let chart = "{title: \(title)}\n\n" + String(localized: "[C]Lyrics with [G]chords") + "\n"
        guard let out = run("song.create", ["title": title, "chart": chart]),
              let id = (out["result"] as? [String: Any])?["id"] as? String else { return nil }
        refreshSongs()
        return id
    }

    /// Import a picked file (ChordPro, OnSong, OpenSong, text, BandHelper TSV, CSV, Cuebook library).
    func importFile(_ url: URL) -> String? {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        return importResult(run("import.file", ["path": url.path(percentEncoded: false)]))
    }

    /// Import pasted text: a song, or Ultimate Guitar links.
    func importText(_ text: String) -> String? {
        if text.contains("ultimate-guitar.com/") {
            guard let out = run("import.ultimate_guitar", ["links": text]), let r = out["result"] as? [String: Any] else { return nil }
            refreshSongs(); bump()
            let n = (r["created"] as? [Any])?.count ?? 0
            return String(localized: "\(n) songs added with their Ultimate Guitar links.")
        }
        return importResult(run("import.text", ["text": text]))
    }

    private func importResult(_ out: [String: Any]?) -> String? {
        guard let r = out?["result"] as? [String: Any] else { return nil }
        refreshSongs(); refreshSetlists(); bump()
        if let first = r["first_song"] as? String { selectedSong = first; section = .songs }
        let warnings = (r["warnings"] as? [Any])?.count ?? 0
        let created = r["songs_created"] as? Int ?? 0
        let updated = r["songs_updated"] as? Int ?? 0
        let setlists = r["setlists"] as? Int ?? 0
        var text = String(localized: "Imported: \(created) new, \(updated) updated songs, \(setlists) setlists.")
        if warnings > 0 { text += " " + String(localized: "\(warnings) warnings – see the songs' notes.") }
        return text
    }

    func trashSong(_ id: String) {
        if run("song.trash", ["id": id]) != nil {
            if selectedSong == id { selectedSong = nil }
            refreshSongs()
            refreshSetlists()
        }
    }

    func createSetlist() -> String? {
        guard let out = run("setlist.create", ["name": String(localized: "New setlist")]),
              let id = (out["result"] as? [String: Any])?["id"] as? String else { return nil }
        refreshSetlists()
        return id
    }

    /// Run a setlist command and refresh the list.
    func setlistCommand(_ command: String, _ params: [String: Any]) {
        run(command, params)
        refreshSetlists()
        revision += 1
    }

    func undo() {
        run("edit.undo")
        refreshSongs()
        refreshSetlists()
        revision += 1
    }

    /// Bumped after changes so views that read the core directly re-render.
    private(set) var revision = 0

    func bump() { revision += 1 }

    /// True while an editor is open; outside changes are then loaded afterwards.
    var isEditing = false

    /// Pick up changes made by another device (iCloud Drive, Nextcloud…) or the CLI.
    /// Not while performing or editing.
    func watchDisk() {
        guard core != nil, performState == nil, !isEditing else { return }
        guard let out = run("library.check"), let r = out["result"] as? [String: Any],
              let n = r["external_changes"] as? Int, n > 0 else { return }
        reload()
    }

    func reload() {
        run("library.reload")
        refreshSongs()
        refreshSetlists()
        bump()
    }

    /// Semitones from the song's key to the gig key.
    func semitones(songKey: String?, gigKey: String?) -> Int8 {
        guard let from = songKey, let to = gigKey else { return 0 }
        return semitonesBetween(from: from, to: to) ?? 0
    }

    // MARK: Perform

    /// The running show (`perform.*` results); nil when not performing.
    var performState: [String: Any]?
    var performSetlist: String?
    private(set) var lastSent: [String] = []

    func startPerform(_ setlistId: String, at index: Int? = nil) {
        var params: [String: Any] = ["setlist": setlistId]
        if let index { params["index"] = index }
        guard let out = run("perform.start", params), let r = out["result"] as? [String: Any] else { return }
        apply(r)
        performSetlist = setlistId
    }

    func perform(_ action: String) {
        guard let out = run("perform.action", ["action": action]), let r = out["result"] as? [String: Any] else { return }
        if (r["running"] as? Bool) == false {
            stopPerform()
            return
        }
        apply(r)
    }

    func performGoto(index: Int, part: Int) {
        guard let out = run("perform.goto", ["index": index, "part": part]), let r = out["result"] as? [String: Any] else { return }
        apply(r)
    }

    func stopPerform() {
        run("perform.stop")
        audio.unload()
        performState = nil
        performSetlist = nil
        lastSent = []
    }

    private func apply(_ result: [String: Any]) {
        for effect in result["effects"] as? [[String: Any]] ?? [] {
            switch effect["type"] as? String {
            case "panic":
                rig.panic()
                audio.stop()
            case "playback":
                if let core { audio.toggle(core: core) }
            default:
                break
            }
        }
        for effect in result["effects"] as? [[String: Any]] ?? [] where effect["type"] as? String == "send_part" {
            rig.sendPart(effect["actions"] as? [[String: Any]] ?? [])
            lastSent = (effect["actions"] as? [[String: Any]] ?? []).map { a in
                "\(a["device"] as? String ?? ""): program \(a["program"].map { "\($0)" } ?? "?"), ch \(a["channel"].map { "\($0)" } ?? "?")"
            }
        }
        performState = result
        followAudio()
    }

    /// Load the current song's backing tracks, decode the next song's.
    private func followAudio() {
        guard let core, let out = run("perform.snapshot"),
              let snap = (out["result"] as? [String: Any])?["snapshot"] as? [String: Any] else { return }
        let current = (snap["current"] as? [String: Any])?["song"] as? String
        let next = (snap["next"] as? [String: Any])?["song"] as? String
        audio.follow(core: core, current: current, next: next)
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
            if defaults.bool(forKey: "gcPerform"), let id = selectedSetlist {
                startPerform(id)
                // `-gcPlay YES`: press Play once the backing tracks are loaded (tests, screenshots).
                if defaults.bool(forKey: "gcPlay") {
                    DispatchQueue.main.asyncAfter(deadline: .now() + 2) { [weak self] in self?.perform("playback") }
                }
            }
        }
        launchEdit = defaults.bool(forKey: "gcEdit")
        if launchEdit, section == .songs { editRequest = selectedSong }
        // `-gcJoin 192.168.1.20:8787 -gcName Anna`: follow a live leader.
        if let address = defaults.string(forKey: "gcJoin") {
            _ = live.join(address: address, name: defaults.string(forKey: "gcName") ?? "iPad", instrument: defaults.string(forKey: "gcInstrument") ?? "")
        }
    }
}

extension LibraryModel {
    /// While following a leader: this library's presets for the leader's
    /// current part, for the devices configured on this device only.
    func sendOwnPresetsForLive() {
        guard let snap = live.snapshot, let songId = snap.current?.song,
              let rec = songRecord(songId),
              let parts = rec.song["parts"] as? [[String: Any]], parts.indices.contains(snap.part) else { return }
        let devices = Set(rig.deviceNames)
        let actions = (parts[snap.part]["actions"] as? [[String: Any]] ?? []).filter { devices.contains($0["device"] as? String ?? "") }
        if !actions.isEmpty { rig.sendPart(actions) }
    }
}

extension LibraryModel {
    /// The chart block the backing track is in: the last section start time at
    /// or before the position, matched to a block by its label.
    func playingSection(songId: String, chart: Chart) -> Int? {
        guard let status = audio.status, status.playing, !status.countingIn, status.songId == songId,
              let rec = songRecord(songId) else { return nil }
        let markers = (rec.song["markers"] as? [[String: Any]] ?? [])
            .compactMap { m -> (String, Double)? in
                guard let name = m["name"] as? String, let at = m["at_secs"] as? Double else { return nil }
                return (name, at)
            }
            .filter { $0.1 <= status.positionSecs }
        guard let current = markers.max(by: { $0.1 < $1.1 }) else { return nil }
        let want = current.0.trimmingCharacters(in: .whitespaces).lowercased()
        return chart.blocks.firstIndex { ($0.label ?? "").trimmingCharacters(in: .whitespaces).lowercased() == want }
    }
}
