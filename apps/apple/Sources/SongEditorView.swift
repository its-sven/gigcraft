import SwiftUI

/// A program change in the editor.
struct PresetDraft: Identifiable, Hashable {
    let id = UUID()
    var device = "Play Acoustic"
    var channel = 1
    var program = 0
}

struct PartDraft: Identifiable, Hashable {
    let id = UUID()
    var name = ""
    var notes = ""
    var presets: [PresetDraft] = []
    /// Actions this editor does not edit (CC, SysEx…), kept as JSON.
    var other: [[String: AnyHashable]] = []
}

/// Edit a song: fields, parts with presets, ChordPro chart.
struct SongEditorView: View {
    @Environment(LibraryModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let songId: String

    @State private var title = ""
    @State private var artist = ""
    @State private var key = ""
    @State private var tempo = ""
    @State private var time = ""
    @State private var duration = ""
    @State private var capo = ""
    @State private var tags = ""
    @State private var notes = ""
    @State private var chart = ""
    @State private var parts: [PartDraft] = []
    @State private var issues: [String] = []
    @State private var loaded = false

    var body: some View {
        Form {
            if !issues.isEmpty {
                Section { ForEach(issues, id: \.self) { Text($0).foregroundStyle(.orange) } }
            }
            Section("Song") {
                field("Title", $title)
                field("Artist", $artist)
                field("Key", $key)
                field("Tempo (BPM)", $tempo, keyboard: .decimalPad)
                field("Time signature", $time)
                field("Duration (m:ss)", $duration)
                field("Capo", $capo, keyboard: .numberPad)
                field("Tags", $tags, prompt: "comma separated")
                LabeledContent("Notes") { TextField("Notes", text: $notes, axis: .vertical).multilineTextAlignment(.trailing) }
            }
            Section {
                ForEach($parts) { $part in
                    VStack(alignment: .leading) {
                        TextField("Part name", text: $part.name).font(.headline)
                        ForEach($part.presets) { $preset in
                            HStack {
                                TextField("Device", text: $preset.device)
                                Stepper("Ch \(preset.channel)", value: $preset.channel, in: 1...16).fixedSize()
                                Stepper("Program \(preset.program)", value: $preset.program, in: 0...127).fixedSize()
                            }
                        }
                        if !part.other.isEmpty {
                            Text("+ \(part.other.count) other MIDI message(s), kept as they are").font(.caption).foregroundStyle(.secondary)
                        }
                        Button("Add program change") { part.presets.append(PresetDraft(device: part.presets.last?.device ?? "Play Acoustic")) }
                            .font(.caption)
                    }
                }
                .onDelete { parts.remove(atOffsets: $0) }
                .onMove { parts.move(fromOffsets: $0, toOffset: $1) }
                Button("Add part") { parts.append(PartDraft(name: "Part \(parts.count + 1)")) }
            } header: {
                Text("Parts and presets")
            } footer: {
                Text("Each part sends its presets when it starts. Program numbers are MIDI values 0–127.")
            }
            Section("Chart (ChordPro)") {
                TextEditor(text: $chart)
                    .font(.system(.body, design: .monospaced))
                    .frame(minHeight: 300)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
            }
            Section("Preview") {
                ChartView(chart: previewChart(source: chart), fontSize: 16)
            }
        }
        .navigationTitle("Edit song")
        .toolbar {
            ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            ToolbarItem(placement: .confirmationAction) { Button("Save") { save() }.bold() }
        }
        .onAppear(perform: load)
    }

    private func field(_ label: String, _ text: Binding<String>, keyboard: UIKeyboardType = .default, prompt: String? = nil) -> some View {
        LabeledContent(label) {
            TextField(prompt ?? label, text: text)
                .keyboardType(keyboard)
                .multilineTextAlignment(.trailing)
        }
    }

    private func load() {
        guard !loaded, let rec = model.songRecord(songId) else { return }
        loaded = true
        let s = rec.song
        title = s["title"] as? String ?? ""
        artist = s["artist"] as? String ?? ""
        key = s["key"] as? String ?? ""
        tempo = (s["tempo_bpm"] as? Double).map { $0.truncatingRemainder(dividingBy: 1) == 0 ? String(Int($0)) : String($0) } ?? ""
        time = s["time_signature"] as? String ?? ""
        duration = (s["duration_secs"] as? Int).map { String(format: "%d:%02d", $0 / 60, $0 % 60) } ?? ""
        capo = (s["capo"] as? Int).map(String.init) ?? ""
        tags = (s["tags"] as? [String] ?? []).joined(separator: ", ")
        notes = s["notes"] as? String ?? ""
        chart = rec.chart
        parts = (s["parts"] as? [[String: Any]] ?? []).map { p in
            var d = PartDraft(name: p["name"] as? String ?? "", notes: p["notes"] as? String ?? "")
            for a in p["actions"] as? [[String: Any]] ?? [] {
                if a["type"] as? String == "program_change", a["bank_msb"] == nil, a["bank_lsb"] == nil {
                    d.presets.append(PresetDraft(device: a["device"] as? String ?? "", channel: a["channel"] as? Int ?? 1, program: a["program"] as? Int ?? 0))
                } else {
                    d.other.append(a.compactMapValues { $0 as? AnyHashable })
                }
            }
            return d
        }
    }

    private func save() {
        func opt(_ s: String) -> Any { s.trimmingCharacters(in: .whitespaces).isEmpty ? NSNull() : s.trimmingCharacters(in: .whitespaces) }
        var fields: [String: Any] = [
            "title": title.trimmingCharacters(in: .whitespaces),
            "artist": artist.trimmingCharacters(in: .whitespaces),
            "key": opt(key), "time_signature": opt(time), "duration": opt(duration), "notes": notes,
            "tags": tags.split(separator: ",").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty },
        ]
        let t = tempo.replacingOccurrences(of: ",", with: ".").trimmingCharacters(in: .whitespaces)
        if t.isEmpty { fields["tempo_bpm"] = NSNull() } else if let v = Double(t) { fields["tempo_bpm"] = v } else { issues = ["Tempo is not a number"]; return }
        let c = capo.trimmingCharacters(in: .whitespaces)
        if c.isEmpty { fields["capo"] = NSNull() } else if let v = Int(c) { fields["capo"] = v } else { issues = ["Capo is not a number"]; return }
        fields["parts"] = parts.map { p -> [String: Any] in
            let actions: [[String: Any]] = p.presets.map { ["device": $0.device, "type": "program_change", "channel": $0.channel, "program": $0.program] } + p.other.map { $0 as [String: Any] }
            return ["name": p.name, "notes": p.notes, "actions": actions]
        }
        guard let result = model.updateSong(songId, fields: fields, chart: chart) else {
            issues = [model.errorMessage ?? "Could not save"]
            return
        }
        if result.isEmpty { dismiss() } else { issues = result }
    }
}
