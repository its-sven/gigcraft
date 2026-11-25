import SwiftUI
import UIKit

/// Join a band leader on the same Wi-Fi.
struct JoinLiveView: View {
    @Environment(LibraryModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @AppStorage("gc.live.name") private var name = ""
    @AppStorage("gc.live.instrument") private var instrument = ""
    @AppStorage("gc.live.address") private var address = ""
    @State private var invalid = false

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Your name", text: $name).textContentType(.name)
                    TextField("Instrument", text: $instrument, prompt: Text("Bass, Drums, Vocals…"))
                } header: { Text("You") }
                Section {
                    TextField("192.168.1.20:8787", text: $address)
                        .keyboardType(.URL)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .accessibilityLabel("Leader's address")
                } header: {
                    Text("Leader's address")
                } footer: {
                    Text("On the leader's computer: Live… → Start live session shows the address. You see the leader's song with your own chart, and your own presets are sent to your devices. Without GigCraft, band members can open the address in Safari instead.")
                }
                if invalid {
                    Text("That is not an address. Example: 192.168.1.20:8787").foregroundStyle(.red)
                }
            }
            .navigationTitle("Follow a leader")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Join") {
                        if model.live.join(address: address, name: name, instrument: instrument) { dismiss() } else { invalid = true }
                    }
                    .bold()
                    .disabled(address.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
        }
    }
}

/// The follower's stage view: the leader's current song with this library's
/// chart (or the leader's), ready button and cues.
struct FollowView: View {
    @Environment(LibraryModel.self) private var model
    @State private var customCue = ""
    @State private var askingCue = false

    private static let quickCues: [LocalizedStringResource] = [
        "Last chorus", "Chorus again", "Extend solo", "Slower", "Faster", "Stop after this bar",
    ]

    var body: some View {
        let live = model.live
        let size = model.fontSize * 1.3
        VStack(spacing: 0) {
            header(live)
            Divider()
            ZStack(alignment: .top) {
                content(live.snapshot, size: size)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                if let cue = live.cue, Date().timeIntervalSince(cue.at) < 8 {
                    Text(verbatim: "\(cue.from): \(cue.text)")
                        .font(.system(size: size * 1.1, weight: .bold))
                        .foregroundStyle(.black)
                        .padding()
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(.orange, in: .rect(cornerRadius: 12))
                        .padding()
                        .accessibilityAddTraits(.isStaticText)
                        .transition(.move(edge: .top))
                }
            }
            Divider()
            footer(live, size: size)
        }
        .onAppear { UIApplication.shared.isIdleTimerDisabled = true }
        .onDisappear { UIApplication.shared.isIdleTimerDisabled = false }
        .statusBarHidden()
        .preferredColorScheme(.dark)
        .alert("Cue to the band", isPresented: $askingCue) {
            TextField("Cue to the band", text: $customCue)
            Button("Send") { live.sendCue(customCue); customCue = "" }
            Button("Cancel", role: .cancel) {}
        }
    }

    private func header(_ live: LiveFollower) -> some View {
        HStack {
            LED(color: live.connected ? .ledGreen : .red)
                .accessibilityLabel(live.connected ? Text("connected") : Text(verbatim: live.status))
            if let s = live.snapshot {
                Text(s.setlistName).bold()
                Readout(label: "Song", value: "\(s.index + 1)/\(s.count)", size: 15)
                    .accessibilityLabel("Item \(s.index + 1) of \(s.count)")
            }
            if !live.leader.isEmpty { Text("Leader: \(live.leader)").foregroundStyle(.secondary) }
            if !live.connected { Text(verbatim: live.status).foregroundStyle(.secondary).lineLimit(1) }
            Spacer()
            Button("Leave") { live.leave() }.bold()
        }
        .padding(.horizontal).padding(.vertical, 8)
    }

    @ViewBuilder
    private func content(_ snap: LiveSnapshot?, size: Double) -> some View {
        if let snap, let cur = snap.current {
            switch cur.type {
            case "song": song(cur, part: snap.part, size: size)
            case "break":
                VStack(spacing: 16) {
                    Spacer()
                    Text(cur.label.flatMap { $0.isEmpty ? nil : $0 } ?? String(localized: "Break")).font(.system(size: size * 2, weight: .bold))
                    Text("\(cur.minutes ?? 0) min").font(.system(size: size * 1.5))
                    Spacer()
                }
                .frame(maxWidth: .infinity)
            default:
                Text("This song is no longer in the library.").font(.title).padding()
            }
        } else {
            VStack { Spacer(); Text("Waiting for the show to start…").font(.title2).foregroundStyle(.secondary); Spacer() }
                .frame(maxWidth: .infinity)
        }
    }

    private func song(_ cur: LiveSlot, part: Int, size: Double) -> some View {
        let songId = cur.song ?? ""
        let own = model.song(songId)
        let chart: Chart? = {
            if own != nil {
                return model.chart(songId, transpose: model.semitones(songKey: own?.key, gigKey: cur.key))
            }
            return cur.chartSource.map { transposedChart(source: $0, semitones: Int8(clamping: cur.semitones ?? 0)) }
        }()
        let parts = cur.partNames ?? []
        return ScrollView {
            VStack(alignment: .leading, spacing: 10) {
                HStack(alignment: .center, spacing: 12) {
                    Text(cur.title ?? "").font(.system(size: scaled(size * 1.6), weight: .bold)).accessibilityAddTraits(.isHeader)
                    if let k = cur.key { Readout(label: "Key", value: k, size: size * 0.8) }
                    if let t = cur.tempoBpm { Readout(label: "BPM", value: "\(Int(t))", size: size * 0.8) }
                }
                if parts.count > 1 {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack {
                            ForEach(Array(parts.enumerated()), id: \.offset) { i, name in
                                Text(name)
                                    .font(.system(size: size * 0.7, weight: i == part ? .bold : .regular))
                                    .padding(.horizontal, 10).padding(.vertical, 4)
                                    .background(i == part ? Color.accentColor.opacity(0.25) : Color.secondary.opacity(0.12), in: .capsule)
                                    .accessibilityAddTraits(i == part ? .isSelected : [])
                            }
                        }
                    }
                }
                if let n = cur.gigNotes, !n.isEmpty { Text(n).font(.system(size: size)).foregroundStyle(.orange) }
                if let chart, !chart.blocks.isEmpty {
                    ChartView(chart: chart, fontSize: size)
                } else if own != nil, let pdf = model.attachments(songId).first(where: { $0.kind == "pdf" }) {
                    PDFKitView(url: pdf.url).frame(minHeight: 900)
                } else {
                    Text("No chart. Edit the song to add one, or attach a PDF.").foregroundStyle(.secondary)
                }
            }
            .padding()
        }
    }

    private func footer(_ live: LiveFollower, size: Double) -> some View {
        let nextText: String = {
            guard let n = live.snapshot?.next else { return String(localized: "Last song") }
            switch n.type {
            case "song": return String(localized: "Up next: \((n.title ?? "") + (n.key.map { "  [\($0)]" } ?? ""))")
            case "break": return String(localized: "Up next: break \(n.minutes ?? 0) min")
            default: return String(localized: "Up next: (missing song)")
            }
        }()
        return HStack(spacing: 16) {
            Text(nextText).font(.system(size: size * 0.8)).lineLimit(2)
            Spacer()
            Menu {
                ForEach(Array(Self.quickCues.enumerated()), id: \.offset) { _, c in
                    Button(String(localized: c)) { live.sendCue(String(localized: c)) }
                }
                Divider()
                Button("Other…") { askingCue = true }
            } label: {
                Label("Cue", systemImage: "megaphone").frame(minHeight: 34)
            }
            .buttonStyle(StageButtonStyle())
            .accessibilityHint("Show a short message on every band member's screen")
            Button { live.toggleReady() } label: {
                Label("Ready", systemImage: live.ready ? "checkmark.circle.fill" : "circle").frame(minWidth: 110, minHeight: 34)
            }
            .buttonStyle(StageButtonStyle(prominent: !live.ready))
            .overlay(alignment: .topTrailing) { if live.ready { LED(color: .ledGreen).offset(x: 4, y: -4) } }
            .accessibilityValue(live.ready ? Text("ready") : Text("not ready"))
            .accessibilityHint("Tell the band you are ready for this song")
        }
        .padding()
    }
}
