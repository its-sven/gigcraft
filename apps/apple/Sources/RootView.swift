import SwiftUI

struct RootView: View {
    @Environment(LibraryModel.self) private var model
    @State private var editingSong: String?
    @State private var showingSettings = false
    @State private var importing = false
    @State private var pasting = false
    @State private var pasteText = ""
    @State private var importMessage: String?
    @State private var joiningLive = false
    @State private var showingHelp = false

    var body: some View {
        @Bindable var model = model
        NavigationSplitView {
            VStack(spacing: 0) {
                Picker("Section", selection: $model.section) {
                    ForEach(LibraryModel.Section.allCases) { Text($0.title).tag($0) }
                }
                .pickerStyle(.segmented)
                .padding(.horizontal)
                .padding(.bottom, 8)
                switch model.section {
                case .songs:
                    List(model.songs, id: \.id, selection: $model.selectedSong) { song in
                        VStack(alignment: .leading, spacing: 2) {
                            Text(song.title).font(.headline)
                            Text([song.artist, song.key.map { String(localized: "Key \($0)") } ?? "", song.duration ?? ""]
                                .filter { !$0.isEmpty }.joined(separator: " · "))
                                .font(.caption).foregroundStyle(.secondary)
                        }
                        .tag(song.id)
                    }
                    .searchable(text: $model.query, prompt: "Search songs")
                case .setlists:
                    List(model.setlists, selection: $model.selectedSetlist) { row in
                        VStack(alignment: .leading, spacing: 2) {
                            Text(row.name).font(.headline)
                            Text([row.date ?? "", row.total].filter { !$0.isEmpty }.joined(separator: " · "))
                                .font(.caption).foregroundStyle(.secondary)
                        }
                        .tag(row.id)
                    }
                }
            }
            .navigationTitle(model.libraryName.isEmpty ? "GigCraft" : model.libraryName)
            .toolbar {
                ToolbarItem {
                    Button {
                        switch model.section {
                        case .songs:
                            if let id = model.createSong() { model.selectedSong = id; editingSong = id }
                        case .setlists:
                            if let id = model.createSetlist() { model.selectedSetlist = id }
                        }
                    } label: { Label("New", systemImage: "plus") }
                }
                ToolbarItem(placement: .bottomBar) {
                    Button("Undo") { model.undo() }
                }
                ToolbarItem(placement: .bottomBar) {
                    Button { showingSettings = true } label: { Label("Library", systemImage: "folder") }
                }
                ToolbarItem(placement: .bottomBar) {
                    Button { joiningLive = true } label: { Label("Live", systemImage: "antenna.radiowaves.left.and.right") }
                        .accessibilityHint("Lead the band or follow a band leader on the same Wi-Fi")
                }
                ToolbarItem(placement: .bottomBar) {
                    Button { showingHelp = true } label: { Label("Help", systemImage: "questionmark.circle") }
                }
                ToolbarItem(placement: .bottomBar) {
                    Menu {
                        Button("Import file…") { importing = true }
                        Button("Paste song or links…") { pasteText = UIPasteboard.general.string ?? ""; pasting = true }
                    } label: { Label("Import", systemImage: "square.and.arrow.down") }
                }
            }
        } detail: {
            NavigationStack {
                switch model.section {
                case .songs:
                    if let id = model.selectedSong {
                        SongDetailView(songId: id, onEdit: { editingSong = id }).id("\(id)-\(model.revision)")
                    } else {
                        ContentUnavailableView("Pick a song", systemImage: "music.note.list")
                    }
                case .setlists:
                    if let id = model.selectedSetlist {
                        SetlistDetailView(setlistId: id).id(id)
                    } else {
                        ContentUnavailableView("Pick a setlist", systemImage: "list.number")
                    }
                }
            }
        }
        .sheet(item: Binding(get: { editingSong.map(EditTarget.init) }, set: { editingSong = $0?.id })) { target in
            NavigationStack { SongEditorView(songId: target.id) }
                .onDisappear { model.refreshSongs() }
        }
        .sheet(isPresented: $showingSettings) { LibrarySettingsView() }
        .sheet(isPresented: $joiningLive) { JoinLiveView() }
        .onAppear { if UserDefaults.standard.bool(forKey: "gcShowJoin") { joiningLive = true } }
        .onAppear {
            // `-gcLead YES`: start leading a live session (tests, screenshots).
            if UserDefaults.standard.bool(forKey: "gcLead"), let core = model.core {
                model.leader.start(name: UserDefaults.standard.string(forKey: "gcName") ?? "", library: core)
            }
        }
        .sheet(isPresented: $showingHelp) { HelpView() }
        .fullScreenCover(isPresented: Binding(get: { model.live.active }, set: { if !$0 { model.live.leave() } })) {
            FollowView().environment(model)
        }
        .fileImporter(isPresented: $importing, allowedContentTypes: [.data, .text, .json, .commaSeparatedText, .tabSeparatedText]) { result in
            if case .success(let url) = result { importMessage = model.importFile(url) }
        }
        .sheet(isPresented: $pasting) {
            NavigationStack {
                TextEditor(text: $pasteText)
                    .font(.system(.body, design: .monospaced))
                    .padding()
                    .navigationTitle("Paste a song or links")
                    .toolbar {
                        ToolbarItem(placement: .cancellationAction) { Button("Cancel") { pasting = false } }
                        ToolbarItem(placement: .confirmationAction) {
                            Button("Import") {
                                if let playlist = ugPlaylistLink(text: pasteText) {
                                    pasting = false
                                    Task { importMessage = await model.importPlaylist(playlist) }
                                } else {
                                    importMessage = model.importText(pasteText); pasting = false
                                }
                            }.bold()
                        }
                    }
            }
        }
        .alert("Import", isPresented: Binding(get: { importMessage != nil }, set: { if !$0 { importMessage = nil } })) {
            Button("OK") { importMessage = nil }
        } message: { Text(importMessage ?? "") }
        .fullScreenCover(isPresented: Binding(get: { model.performState != nil }, set: { if !$0 { model.stopPerform() } })) {
            PerformView().environment(model)
        }
        .onChange(of: model.editRequest) { _, id in
            if let id { editingSong = id; model.editRequest = nil }
        }
        .overlay(alignment: .bottom) {
            if let message = model.errorMessage {
                Text(message).font(.footnote).padding(8).background(.red.opacity(0.85), in: .rect(cornerRadius: 8))
                    .foregroundStyle(.white).padding()
            }
        }
    }
}

struct EditTarget: Identifiable { let id: String }

/// Where the library lives; sync between devices = the same folder in iCloud Drive or Nextcloud.
struct LibrarySettingsView: View {
    @Environment(LibraryModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var picking = false
    @State private var backupMessage: String?

    var body: some View {
        NavigationStack {
            Form {
                Section("Library") {
                    LabeledContent("Name", value: model.libraryName)
                    Text(model.libraryLocation).font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
                }
                Section {
                    Button("Choose a folder…") { picking = true }
                    Button("Use the app's own folder") { model.useAppFolder(); dismiss() }
                } footer: {
                    Text("To use the same songs on iPhone, iPad and computer, pick the same folder in iCloud Drive, Nextcloud or another Files provider on each device. An empty folder becomes a new library. Changes from other devices appear within a few seconds; if both change the same song, the second save asks first.")
                }
                Section {
                    Button("Back up now") {
                        model.backUp(auto: false) { path in
                            backupMessage = path.map { _ in String(localized: "Backup saved in Files → GigCraft → Backups.") }
                                ?? String(localized: "Backup failed.")
                        }
                    }
                    if let backupMessage { Text(backupMessage).font(.callout).foregroundStyle(.secondary) }
                } header: {
                    Text("Backups")
                } footer: {
                    Text("GigCraft saves the library once a day as a ZIP file in Files → GigCraft → Backups and keeps the last 14. To restore, unzip a backup and choose its folder.")
                }
            }
            .safeAreaInset(edge: .bottom) { RigSection().padding() }
            .navigationTitle("Library")
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
            .fileImporter(isPresented: $picking, allowedContentTypes: [.folder]) { result in
                if case .success(let url) = result { model.useFolder(url); dismiss() }
            }
        }
    }
}

/// MIDI devices and footswitches (simulated pedal, status, MIDI Learn).
struct RigSection: View {
    @Environment(LibraryModel.self) private var model

    var body: some View {
        let rig = model.rig
        VStack(alignment: .leading, spacing: 8) {
            Text("Rig").font(.headline).accessibilityAddTraits(.isHeader)
            ForEach(rig.status, id: \.self) { Text($0).font(.callout) }
            Toggle("Simulated Play Acoustic (no hardware)", isOn: Binding(get: { rig.usesSimulatedPedal }, set: { rig.setSimulated($0) }))
            Picker("MIDI clock", selection: Binding(get: { rig.clockMode }, set: { rig.setClockMode($0) })) {
                Text("Off").tag("off")
                Text("Always, song tempo").tag("always")
                Text("While tracks play").tag("while_playing")
            }
            .accessibilityHint("Tempo sync for delays and loopers")
            HStack {
                ForEach(["next", "previous", "next_song", "previous_song"], id: \.self) { action in
                    Button {
                        rig.learn(rig.learningFor == action ? nil : action)
                    } label: {
                        if rig.learningFor == action {
                            Text("Press the switch…")
                        } else {
                            Text("Learn: \(RigBridge.actionName(action))")
                        }
                    }
                    .buttonStyle(.bordered)
                    .font(.caption)
                }
            }
            if let last = rig.log.last { Text(last).font(.caption).foregroundStyle(.secondary) }
            if model.audio.channels > 2 {
                Text("Audio interface: \(model.audio.channels) outputs").font(.caption).foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
        .background(.background.secondary, in: .rect(cornerRadius: 12))
    }
}
