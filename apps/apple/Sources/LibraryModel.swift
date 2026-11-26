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
    /// Leading a live session from this device.
    let leader = LiveLeader()
    private(set) var libraryName = ""
    private(set) var errorMessage: String?
    /// A short note after an import or backup (shown at the bottom, goes by itself).
    var notice: String?
    /// The chart scrolls by itself (song view and stage view).
    var autoScrolling = false
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
            guard let self else { return }
            if self.performState != nil {
                self.perform(action)
            } else if action == "auto_scroll" {
                // A footswitch while practising a song.
                self.autoScrolling.toggle()
            }
        }
        live.onPartChanged = { [weak self] in self?.sendOwnPresetsForLive() }
        audio.onPlayingChanged = { [weak self] playing in self?.rig.playback(playing) }
        audio.onPosition = { [weak self] secs in self?.trackPosition(secs) }
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
            backUp(auto: true)
            return true
        } catch {
            errorMessage = Self.describe(error)
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
            errorMessage = Self.describe(error)
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

    /// Import a picked or dropped file: charts, libraries, web shortcuts
    /// (.webloc, .url) and saved Ultimate Guitar pages.
    @MainActor
    func importFile(_ url: URL) async {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        await handleImport(run("import.file", ["path": url.path(percentEncoded: false)]))
    }

    /// Import text: a song, Ultimate Guitar tab links or a shared playlist link.
    @MainActor
    func importText(_ text: String) async {
        await handleImport(run("import.text", ["text": text]))
    }

    /// What an import made, whatever it was; the result goes to `notice`.
    @MainActor
    private func handleImport(_ out: [String: Any]?) async {
        guard let r = out?["result"] as? [String: Any] else { return }
        switch r["kind"] as? String {
        case "fetch_playlist":
            notice = String(localized: "Loading the playlist…")
            notice = await importPlaylist(r["url"] as? String ?? "")
        case "playlist":
            notice = playlistResult(r)
        case "links":
            refreshSongs(); bump()
            if let first = r["first_song"] as? String { selectedSong = first; section = .songs }
            fetchDetails(r)
            let created = (r["created"] as? [Any])?.count ?? 0
            let existing = (r["existing"] as? [Any])?.count ?? 0
            var parts: [String] = []
            if created > 0 { parts.append(String(localized: "\(created) songs added with their Ultimate Guitar links.")) }
            if existing > 0 { parts.append(String(localized: "\(existing) already in the library.")) }
            notice = parts.joined(separator: " ")
        default:
            notice = importResult(r)
        }
    }

    /// A shared Ultimate Guitar playlist: load the page once, make a setlist of it.
    @MainActor
    func importPlaylist(_ url: String) async -> String? {
        guard let pageURL = URL(string: url) else { return String(localized: "That is not a link.") }
        switch await Self.loadPage(pageURL) {
        case .failure(let message):
            return String(localized: "Could not load the playlist: \(message)")
        case .success(let html):
            guard let out = run("import.ultimate_guitar_playlist", ["html": html, "url": url]),
                  let r = out["result"] as? [String: Any] else { return nil }
            return playlistResult(r)
        }
    }

    private func playlistResult(_ r: [String: Any]) -> String {
        refreshSongs(); refreshSetlists(); bump()
        if let id = r["setlist"] as? String { selectedSetlist = id; section = .setlists }
        fetchDetails(r)
        let name = r["name"] as? String ?? ""
        let songs = r["songs"] as? Int ?? 0
        let created = r["created"] as? Int ?? 0
        return String(localized: "Setlist “\(name)”: \(songs) songs, \(created) new in the library.")
    }

    /// Load the new songs' tab pages one after the other and fill in title,
    /// artist, key, capo and tuning (never the tab itself).
    private func fetchDetails(_ r: [String: Any]) {
        let items: [(String, URL)] = (r["fetch"] as? [[String: Any]] ?? []).compactMap { f in
            guard let song = f["song"] as? String,
                  let url = (f["url"] as? String).flatMap(URL.init(string:)) else { return nil }
            return (song, url)
        }
        guard !items.isEmpty, let core else { return }
        Task { @MainActor in
            for (song, url) in items {
                guard case .success(let html) = await Self.loadPage(url),
                      let data = try? JSONSerialization.data(withJSONObject: ["id": song, "html": html]),
                      let out = try? core.run(command: "import.ultimate_guitar_details", paramsJson: String(decoding: data, as: UTF8.self)),
                      out.contains("\"changed\":true") else { continue }
                refreshSongs(); refreshSetlists(); bump()
            }
        }
    }

    enum PageResult { case success(String), failure(String) }

    /// A web page as Safari on the iPhone would get it.
    static func loadPage(_ url: URL) async -> PageResult {
        var request = URLRequest(url: url, timeoutInterval: 20)
        request.setValue("Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Mobile/15E148 Safari/604.1",
                         forHTTPHeaderField: "User-Agent")
        do {
            let (data, response) = try await URLSession.shared.data(for: request)
            if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
                return .failure("HTTP \(http.statusCode)")
            }
            return .success(String(decoding: data, as: UTF8.self))
        } catch {
            return .failure(error.localizedDescription)
        }
    }

    private func importResult(_ r: [String: Any]) -> String {
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
        rig.setTempo(nil)
        rig.playback(false)
        performState = nil
        performSetlist = nil
        autoScrolling = false
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
            case "auto_scroll":
                autoScrolling.toggle()
            case "show":
                autoScrolling = false
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
        let cur = result["current"] as? [String: Any]
        rig.setTempo(cur?["type"] as? String == "song" ? cur?["tempo_bpm"] as? Double : nil)
    }

    /// The backing track reached `secs`: switch to the part of its section.
    private func trackPosition(_ secs: Double) {
        guard performState != nil, let out = run("perform.track_position", ["secs": secs]),
              let r = out["result"] as? [String: Any], !(r["effects"] as? [Any] ?? []).isEmpty else { return }
        apply(r)
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
        // `-gcImportText <text>`: import as if pasted (tests).
        if let text = defaults.string(forKey: "gcImportText") {
            Task { @MainActor in await self.importText(text) }
        }
        // `-gcAutoScroll YES`: start auto-scroll (tests, screenshots).
        if defaults.bool(forKey: "gcAutoScroll") {
            DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { [weak self] in self?.autoScrolling = true }
        }
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

extension LibraryModel {
    /// Backups live in the app's own Documents/Backups (Files app → GigCraft),
    /// never inside the library (which may be a synced folder).
    static var backupsFolder: URL { URL.documentsDirectory.appending(path: "Backups", directoryHint: .isDirectory) }

    /// The daily automatic backup (`auto`), or one now; runs off the main thread.
    func backUp(auto: Bool, done: ((String?) -> Void)? = nil) {
        guard let core else { return }
        let dir = Self.backupsFolder.path(percentEncoded: false)
        DispatchQueue.global(qos: .utility).async {
            let out = try? core.run(command: auto ? "library.auto_backup" : "library.backup",
                                    paramsJson: String(decoding: (try? JSONSerialization.data(withJSONObject: ["dir": dir])) ?? Data(), as: UTF8.self))
            let path = out.flatMap { try? JSONSerialization.jsonObject(with: Data($0.utf8)) as? [String: Any] }
                .flatMap { ($0["result"] as? [String: Any])?["path"] as? String }
            DispatchQueue.main.async { done?(path) }
        }
    }
}

extension LibraryModel {
    /// A sentence for the musician, not the error value
    /// (`Failed(code: "rejected", message: …)` → the message).
    static func describe(_ error: Error) -> String {
        if case let CoreError.Failed(_, message) = error {
            // Core messages are English; the known ones have a translation.
            return Bundle.main.localizedString(forKey: message, value: message, table: nil)
        }
        return error.localizedDescription
    }

    func clearError() { errorMessage = nil }
}
