import SwiftUI

/// A program change in the editor.
struct PresetDraft: Identifiable, Hashable {
    let id = UUID()
    var device = "Play Acoustic"
    var channel = 1
    var program = 0
}

/// A backing track in the editor (the model's `Track`).
struct TrackDraft: Identifiable, Hashable {
    var id: String { file }
    var file: String
    var label = ""
    var role = "backing"
    var gainDb = 0.0
    var pan = 0.0
    var bus = 0
    var mute = false
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
    @State private var tracks: [TrackDraft] = []
    @State private var audioFiles: [String] = []
    @State private var clickOn = false
    @State private var countIn = 0
    @State private var clickVolume = 0.0
    /// Start time ("m:ss") per chart section; other markers kept as they are.
    @State private var sectionTimes: [String: String] = [:]
    @State private var otherMarkers: [[String: Any]] = []
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
                            Text("+ \(part.other.count) other MIDI messages, kept as they are").font(.caption).foregroundStyle(.secondary)
                        }
                        Button("Add program change") { part.presets.append(PresetDraft(device: part.presets.last?.device ?? "Play Acoustic")) }
                            .font(.caption)
                    }
                }
                .onDelete { parts.remove(atOffsets: $0) }
                .onMove { parts.move(fromOffsets: $0, toOffset: $1) }
                Button("Add part") { parts.append(PartDraft(name: String(localized: "Part \(parts.count + 1)"))) }
            } header: {
                Text("Parts and presets")
            } footer: {
                Text("Each part sends its presets when it starts. Program numbers are MIDI values 0–127.")
            }
            Section {
                Toggle("Click", isOn: $clickOn)
                if clickOn {
                    Stepper(value: $countIn, in: 0...4) { Text("Count-in: \(countIn) bars") }
                    LabeledContent("Click volume") {
                        Slider(value: $clickVolume, in: -40...12) { Text("Click volume") }
                            .frame(maxWidth: 260)
                        Text(verbatim: String(format: "%+.0f dB", clickVolume)).monospacedDigit().frame(width: 60, alignment: .trailing)
                    }
                    if tempo.trimmingCharacters(in: .whitespaces).isEmpty {
                        Text("The click needs a tempo (BPM) above.").foregroundStyle(.orange)
                    }
                }
                ForEach($tracks) { $track in
                    VStack(alignment: .leading, spacing: 6) {
                        HStack {
                            Text(track.file).font(.headline).lineLimit(1)
                            Spacer()
                            Picker("Role", selection: $track.role) {
                                Text("Backing").tag("backing")
                                Text("Click track").tag("click")
                                Text("Guide / cues").tag("guide")
                                Text("Stem").tag("stem")
                            }
                            .labelsHidden()
                        }
                        HStack {
                            Text("Volume").font(.caption).foregroundStyle(.secondary)
                            Slider(value: $track.gainDb, in: -40...12) { Text("Volume") }
                            Text(verbatim: String(format: "%+.0f dB", track.gainDb)).monospacedDigit().font(.caption).frame(width: 56, alignment: .trailing)
                            Toggle("Mute", isOn: $track.mute).toggleStyle(.button).font(.caption)
                        }
                    }
                }
                .onDelete { tracks.remove(atOffsets: $0) }
                let unused = audioFiles.filter { f in !tracks.contains { $0.file == f } }
                if unused.isEmpty {
                    if tracks.isEmpty {
                        Text("Attach an audio file to the song first (Attach in the song view).").font(.caption).foregroundStyle(.secondary)
                    }
                } else {
                    Menu("Add track") {
                        ForEach(unused, id: \.self) { f in Button(f) { tracks.append(TrackDraft(file: f)) } }
                    }
                }
                if !tracks.isEmpty || clickOn {
                    let labels = sectionLabels()
                    if !labels.isEmpty {
                        Text("Section start times").font(.subheadline.weight(.semibold)).padding(.top, 4)
                        ForEach(labels, id: \.self) { label in
                            LabeledContent(label) {
                                TextField("m:ss", text: Binding(get: { sectionTimes[label] ?? "" }, set: { sectionTimes[label] = $0 }))
                                    .keyboardType(.numbersAndPunctuation)
                                    .multilineTextAlignment(.trailing)
                                    .frame(maxWidth: 90)
                            }
                        }
                    }
                }
            } header: {
                Text("Backing tracks and click")
            } footer: {
                Text("In perform mode, Play starts the tracks and the click together (key P or a footswitch). With section start times, the chart follows the backing track. The iPad plays in stereo.")
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
        .onAppear {
            model.isEditing = true
            load()
        }
        .onDisappear { model.isEditing = false }
    }

    private func field(_ label: LocalizedStringKey, _ text: Binding<String>, keyboard: UIKeyboardType = .default, prompt: LocalizedStringKey? = nil) -> some View {
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
        tracks = (s["tracks"] as? [[String: Any]] ?? []).compactMap { t in
            guard let file = t["file"] as? String else { return nil }
            return TrackDraft(file: file, label: t["label"] as? String ?? "", role: t["role"] as? String ?? "backing",
                              gainDb: t["gain_db"] as? Double ?? 0, pan: t["pan"] as? Double ?? 0,
                              bus: t["bus"] as? Int ?? 0, mute: t["mute"] as? Bool ?? false)
        }
        if let click = s["click"] as? [String: Any] {
            clickOn = click["enabled"] as? Bool ?? true
            countIn = click["count_in_bars"] as? Int ?? 0
            clickVolume = click["volume_db"] as? Double ?? 0
        }
        audioFiles = model.attachments(songId).filter { $0.kind == "audio" }.map(\.file)
        let labels = Set(previewChart(source: rec.chart).blocks.compactMap(\.label))
        for m in s["markers"] as? [[String: Any]] ?? [] {
            if let name = m["name"] as? String, labels.contains(name), let at = m["at_secs"] as? Double {
                sectionTimes[name] = Self.clockText(at)
            } else {
                otherMarkers.append(m)
            }
        }
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
        if t.isEmpty { fields["tempo_bpm"] = NSNull() } else if let v = Double(t) { fields["tempo_bpm"] = v } else { issues = [String(localized: "Tempo is not a number")]; return }
        let c = capo.trimmingCharacters(in: .whitespaces)
        if c.isEmpty { fields["capo"] = NSNull() } else if let v = Int(c) { fields["capo"] = v } else { issues = [String(localized: "Capo is not a number")]; return }
        var markers = otherMarkers
        for (label, text) in sectionTimes where !text.trimmingCharacters(in: .whitespaces).isEmpty {
            guard let at = Self.parseClock(text) else {
                issues = [String(localized: "\(label): “\(text)” is not a time like 1:23")]
                return
            }
            markers.append(["name": label, "at_secs": at])
        }
        fields["markers"] = markers.sorted { ($0["at_secs"] as? Double ?? 0) < ($1["at_secs"] as? Double ?? 0) }
        fields["tracks"] = tracks.map { t -> [String: Any] in
            ["file": t.file, "label": t.label, "role": t.role, "gain_db": t.gainDb, "pan": t.pan, "bus": t.bus, "mute": t.mute]
        }
        fields["click"] = clickOn ? (["enabled": true, "count_in_bars": countIn, "volume_db": clickVolume] as [String: Any]) : NSNull()
        fields["parts"] = parts.map { p -> [String: Any] in
            let actions: [[String: Any]] = p.presets.map { ["device": $0.device, "type": "program_change", "channel": $0.channel, "program": $0.program] } + p.other.map { $0 as [String: Any] }
            return ["name": p.name, "notes": p.notes, "actions": actions]
        }
        guard let result = model.updateSong(songId, fields: fields, chart: chart) else {
            let message = model.errorMessage ?? String(localized: "Could not save")
            if message.contains("changed by another device") {
                // Load the other version; ours stays in the editor and wins on the next save.
                model.reload()
                issues = [String(localized: "This song was changed on another device. The library has been reloaded; save again to replace that version with yours, or cancel to keep it.")]
            } else {
                issues = [message]
            }
            return
        }
        if result.isEmpty { dismiss() } else { issues = result }
    }

    private func sectionLabels() -> [String] {
        var seen: [String] = []
        for block in previewChart(source: chart).blocks where block.kind != "chorus_recall" {
            if let l = block.label, !seen.contains(l) { seen.append(l) }
        }
        return seen
    }

    static func clockText(_ secs: Double) -> String {
        let m = Int(secs) / 60
        let s = secs - Double(m * 60)
        return abs(s - s.rounded()) < 0.05 ? String(format: "%d:%02d", m, Int(s.rounded())) : String(format: "%d:%04.1f", m, s)
    }

    /// Seconds from "1:23", "1:23.5" or "83".
    static func parseClock(_ text: String) -> Double? {
        let t = text.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: ",", with: ".")
        let parts = t.split(separator: ":", omittingEmptySubsequences: false)
        if parts.count == 2, let m = Double(parts[0]), let s = Double(parts[1]), m >= 0, (0..<60).contains(s) { return m * 60 + s }
        if parts.count == 1, let v = Double(t), v >= 0, v.isFinite { return v }
        return nil
    }
}
