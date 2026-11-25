import SwiftUI

struct SetlistDetailView: View {
    @Environment(LibraryModel.self) private var model
    let setlistId: String

    var body: some View {
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
                    Section("\(set.name) (\(set.duration))") {
                        ForEach(numbered(set.entries), id: \.entry.id) { item in
                            row(item.entry, number: item.number)
                        }
                    }
                }
            }
            .navigationBarTitleDisplayMode(.inline)
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
    private func row(_ e: SetlistEntry, number: Int?) -> some View {
        if e.isBreak {
            Text("— \(e.title) \(e.minutes) min —").font(.footnote).foregroundStyle(.secondary)
                .frame(maxWidth: .infinity)
        } else {
            Button {
                if let id = e.songId, !e.missing {
                    model.selectedSong = id
                    model.section = .songs
                }
            } label: {
                VStack(alignment: .leading, spacing: 2) {
                    HStack {
                        Text("\(number ?? 0). \(e.title)").font(.system(size: model.fontSize))
                            .strikethrough(e.missing)
                        Spacer()
                        if e.optional { Text("optional").font(.caption).foregroundStyle(.secondary) }
                        if let k = e.key { Text(k).font(.headline.monospaced()).foregroundStyle(.tint) }
                    }
                    if !e.notes.isEmpty { Text(e.notes).font(.caption).italic().foregroundStyle(.secondary) }
                }
            }
            .buttonStyle(.plain)
            .disabled(e.missing)
        }
    }
}
