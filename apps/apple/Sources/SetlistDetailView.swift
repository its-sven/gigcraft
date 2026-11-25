import SwiftUI

struct SetlistDetailView: View {
    @Environment(LibraryModel.self) private var model
    let setlistId: String
    @State private var editMode: EditMode = .inactive
    @State private var addingSongs = false
    @State private var editingEntry: EntryRef?
    @State private var renaming = false
    @State private var newName = ""

    struct EntryRef: Identifiable, Hashable {
        let set: Int
        let index: Int
        var id: String { "\(set)-\(index)" }
    }

    var body: some View {
        let _ = model.revision
        if let list = model.setlist(setlistId) {
            List {
                Section {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(list.name).font(.title.weight(.semibold))
                        Text([list.date ?? "", "\(list.total) total"].filter { !$0.isEmpty }.joined(separator: " · "))
                            .foregroundStyle(.secondary)
                        if !list.notes.isEmpty { Text(list.notes).italic() }
                    }
                }
                ForEach(list.sets) { set in
                    Section {
                        ForEach(numbered(set.entries), id: \.entry.id) { item in
                            row(item.entry, number: item.number, set: set.id)
                        }
                        .onMove { from, to in
                            guard let f = from.first else { return }
                            let target = to > f ? to - 1 : to
                            model.setlistCommand("setlist.move_entry", ["id": setlistId, "from_set": set.id, "from": f, "to": target])
                        }
                        .onDelete { offsets in
                            for i in offsets.sorted(by: >) {
                                model.setlistCommand("setlist.remove_entry", ["id": setlistId, "set": set.id, "index": i])
                            }
                        }
                        if editMode.isEditing {
                            Button("Add break") { model.setlistCommand("setlist.add_break", ["id": setlistId, "set": set.id, "minutes": 15]) }
                        }
                    } header: {
                        Text("\(set.name) (\(set.duration))")
                    }
                }
                if editMode.isEditing {
                    Section {
                        Button("Add songs…") { addingSongs = true }
                        Button("Add set") { model.setlistCommand("setlist.add_set", ["id": setlistId]) }
                        Button("Rename…") { newName = list.name; renaming = true }
                        Button("Move to trash", role: .destructive) {
                            model.setlistCommand("setlist.trash", ["id": setlistId])
                            model.selectedSetlist = nil
                        }
                    }
                }
            }
            .environment(\.editMode, $editMode)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItemGroup {
                    Button { model.startPerform(setlistId) } label: { Label("Perform", systemImage: "play.fill") }
                    ShareLink(item: list.plainText) { Label("Share", systemImage: "square.and.arrow.up") }
                    EditButton().environment(\.editMode, $editMode)
                }
            }
            .sheet(isPresented: $addingSongs) { AddSongsSheet(setlistId: setlistId) }
            .sheet(item: $editingEntry) { ref in EntrySheet(setlistId: setlistId, ref: ref, list: list) }
            .alert("Rename setlist", isPresented: $renaming) {
                TextField("Name", text: $newName)
                Button("Save") { model.setlistCommand("setlist.update", ["id": setlistId, "fields": ["name": newName]]) }
                Button("Cancel", role: .cancel) {}
            }
            .onAppear { if model.launchEdit { editMode = .active } }
        } else {
            ContentUnavailableView("Setlist not found", systemImage: "questionmark")
        }
    }

    private func numbered(_ entries: [SetlistEntry]) -> [(entry: SetlistEntry, number: Int?)] {
        var n = 0
        return entries.map { e in
            if e.isBreak { return (e, nil) }
            n += 1
            return (e, n)
        }
    }

    @ViewBuilder
    private func row(_ e: SetlistEntry, number: Int?, set: Int) -> some View {
        if e.isBreak {
            Button { if editMode.isEditing { editingEntry = EntryRef(set: set, index: e.id) } } label: {
                Text("— \(e.title) \(e.minutes) min —").font(.footnote).foregroundStyle(.secondary).frame(maxWidth: .infinity)
            }
            .buttonStyle(.plain)
        } else {
            Button {
                if editMode.isEditing {
                    editingEntry = EntryRef(set: set, index: e.id)
                } else if let id = e.songId, !e.missing {
                    model.selectedSong = id
                    model.section = .songs
                }
            } label: {
                VStack(alignment: .leading, spacing: 2) {
                    HStack {
                        Text("\(number ?? 0). \(e.title)").font(.system(size: model.fontSize)).strikethrough(e.missing)
                        Spacer()
                        if e.optional { Text("optional").font(.caption).foregroundStyle(.secondary) }
                        if let k = e.key { Text(k).font(.headline.monospaced()).foregroundStyle(.tint) }
                    }
                    if !e.notes.isEmpty { Text(e.notes).font(.caption).italic().foregroundStyle(.secondary) }
                }
            }
            .buttonStyle(.plain)
            .contextMenu {
                Button("Perform from here") { model.startPerform(setlistId, at: flatIndex(set: set, index: e.id)) }
            }
        }
    }

    private func flatIndex(set: Int, index: Int) -> Int {
        guard let list = model.setlist(setlistId) else { return 0 }
        return list.sets.prefix(set).reduce(0) { $0 + $1.entries.count } + index
    }
}

/// Key, note and "optional" of one setlist entry, or a break's length.
struct EntrySheet: View {
    @Environment(LibraryModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let setlistId: String
    let ref: SetlistDetailView.EntryRef
    let list: SetlistDetail
    @State private var key = ""
    @State private var notes = ""
    @State private var optional = false
    @State private var minutes = 15

    private var entry: SetlistEntry? {
        guard list.sets.indices.contains(ref.set), list.sets[ref.set].entries.indices.contains(ref.index) else { return nil }
        return list.sets[ref.set].entries[ref.index]
    }

    var body: some View {
        NavigationStack {
            Form {
                if let e = entry {
                    if e.isBreak {
                        Stepper("\(minutes) min", value: $minutes, in: 0...240, step: 5)
                    } else {
                        Section(e.title) {
                            TextField("Key for this gig", text: $key)
                            TextField("Note for this gig", text: $notes, axis: .vertical)
                            Toggle("Optional", isOn: $optional)
                        }
                    }
                }
            }
            .navigationTitle("Entry")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) { Button("Save") { save(); dismiss() }.bold() }
            }
            .onAppear {
                key = entry?.key ?? ""
                notes = entry?.notes ?? ""
                optional = entry?.optional ?? false
                minutes = entry?.minutes ?? 15
            }
        }
        .presentationDetents([.medium])
    }

    private func save() {
        guard let e = entry else { return }
        let fields: [String: Any] = e.isBreak ? ["minutes": minutes] : ["key": key, "notes": notes, "optional": optional]
        model.setlistCommand("setlist.update_entry", ["id": setlistId, "set": ref.set, "index": ref.index, "fields": fields])
    }
}

struct AddSongsSheet: View {
    @Environment(LibraryModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let setlistId: String
    @State private var query = ""
    @State private var added: [String] = []

    var body: some View {
        NavigationStack {
            List(model.core?.songs(query: query) ?? [], id: \.id) { song in
                Button {
                    model.setlistCommand("setlist.add_song", ["id": setlistId, "song": song.id])
                    added.append(song.id)
                } label: {
                    HStack {
                        VStack(alignment: .leading) {
                            Text(song.title)
                            if !song.artist.isEmpty { Text(song.artist).font(.caption).foregroundStyle(.secondary) }
                        }
                        Spacer()
                        let n = added.filter { $0 == song.id }.count
                        if n > 0 { Text(n == 1 ? "added" : "added ×\(n)").font(.caption).foregroundStyle(.tint) }
                        Image(systemName: "plus.circle")
                    }
                }
            }
            .searchable(text: $query, prompt: "Search songs")
            .navigationTitle("Add songs")
            .toolbar { ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } } }
        }
    }
}

extension SetlistDetail {
    /// For sharing and printing.
    var plainText: String {
        var out = name + (date.map { " – \($0)" } ?? "") + "\n"
        for set in sets {
            out += "\n\(set.name) (\(set.duration))\n"
            var n = 0
            for e in set.entries {
                if e.isBreak { out += "   — \(e.title) \(e.minutes) min —\n"; continue }
                n += 1
                out += "\(n). \(e.title)" + (e.key.map { "  [\($0)]" } ?? "") + (e.optional ? "  (optional)" : "") + (e.notes.isEmpty ? "" : "  – \(e.notes)") + "\n"
            }
        }
        return out + "\nTotal \(total)\n"
    }
}
