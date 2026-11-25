import SwiftUI

struct RootView: View {
    @Environment(LibraryModel.self) private var model
    @State private var editingSong: String?
    @State private var showingSettings = false

    var body: some View {
        @Bindable var model = model
        NavigationSplitView {
            VStack(spacing: 0) {
                Picker("Section", selection: $model.section) {
                    ForEach(LibraryModel.Section.allCases) { Text($0.rawValue).tag($0) }
                }
                .pickerStyle(.segmented)
                .padding(.horizontal)
                .padding(.bottom, 8)
                switch model.section {
                case .songs:
                    List(model.songs, id: \.id, selection: $model.selectedSong) { song in
                        VStack(alignment: .leading, spacing: 2) {
                            Text(song.title).font(.headline)
                            Text([song.artist, song.key.map { "Key \($0)" } ?? "", song.duration ?? ""]
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
            }
            .navigationTitle("Library")
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
            .fileImporter(isPresented: $picking, allowedContentTypes: [.folder]) { result in
                if case .success(let url) = result { model.useFolder(url); dismiss() }
            }
        }
    }
}
