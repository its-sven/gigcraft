import SwiftUI

struct RootView: View {
    @Environment(LibraryModel.self) private var model

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
        } detail: {
            NavigationStack {
                switch model.section {
                case .songs:
                    if let id = model.selectedSong {
                        SongDetailView(songId: id).id(id)
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
        .overlay(alignment: .bottom) {
            if let message = model.errorMessage {
                Text(message).font(.footnote).padding(8).background(.red.opacity(0.85), in: .rect(cornerRadius: 8))
                    .foregroundStyle(.white).padding()
            }
        }
    }
}
