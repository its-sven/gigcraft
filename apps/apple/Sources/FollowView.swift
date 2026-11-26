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
    @State private var browser = LeaderBrowser()
    @State private var leaderCue = ""

    /// Lead the band from this device.
    @ViewBuilder
    private var leadSection: some View {
        let leader = model.leader
        Section {
            if leader.active {
                HStack(alignment: .top, spacing: 16) {
                    if let qr = leader.qrImage() {
                        Image(uiImage: qr).interpolation(.none).resizable().frame(width: 150, height: 150)
                            .accessibilityLabel("QR code of the address")
                    }
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Band members open this address on the same Wi-Fi:").font(.callout)
                        ForEach(leader.urls, id: \.self) { Text(verbatim: $0).font(.body.monospaced()).textSelection(.enabled) }
                        if leader.urls.isEmpty { Text("No network found. Connect to a Wi-Fi or a router.").foregroundStyle(.orange) }
                    }
                }
                ForEach(leader.roster) { m in
                    HStack {
                        LED(color: m.ready ? .ledGreen : .secondary)
                        Text(m.instrument.isEmpty ? m.name : "\(m.name) (\(m.instrument))")
                        Spacer()
                        if m.ready { Text("ready").foregroundStyle(Color.ledGreen) }
                    }
                }
                HStack {
                    TextField("Cue to the band", text: $leaderCue)
                    Button("Send") { leader.sendCue(leaderCue); leaderCue = "" }.disabled(leaderCue.isEmpty)
                }
                Button("End live session", role: .destructive) { leader.stop() }
            } else {
                Button {
                    if let core = model.core { leader.start(name: name, library: core) }
                } label: {
                    Label("Start live session", systemImage: "dot.radiowaves.left.and.right")
                }
                .disabled(model.core == nil)
            }
        } header: {
            Text("Lead the band")
        } footer: {
            if !leader.active {
                Text("Band members follow your setlist on their phones, tablets or computers – on the same Wi-Fi, no app needed.")
            }
        }
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Your name", text: $name).textContentType(.name)
                    TextField("Instrument", text: $instrument, prompt: Text("Bass, Drums, Vocals…"))
                } header: { Text("You") }
                leadSection
                if !model.leader.active {
                Section {
                    if browser.leaders.isEmpty {
                        HStack(spacing: 10) {
                            ProgressView()
                            Text("Looking for leaders on this network…").foregroundStyle(.secondary)
                        }
                    }
                    ForEach(browser.leaders) { leader in
                        Button {
                            address = leader.address
                            if model.live.join(address: leader.address, name: name, instrument: instrument) { dismiss() }
                        } label: {
                            HStack(spacing: 10) {
                                LED(color: .ledGreen)
                                VStack(alignment: .leading) {
                                    Text(leader.name).font(.headline).foregroundStyle(.primary)
                                    Text(verbatim: leader.address).font(.caption.monospaced()).foregroundStyle(.secondary)
                                }
                                Spacer()
                                Text("Join").bold().foregroundStyle(Color.accentColor)
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityHint("Follow this leader")
                    }
                } header: {
                    Text("Leaders on this network")
                }
                Section {
                    TextField("192.168.1.20:8787", text: $address)
                        .keyboardType(.URL)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .accessibilityLabel("Leader's address")
                } header: {
                    Text("Or enter the address")
                } footer: {
                    Text("On the leader's computer: Live… → Start live session shows the address. You see the leader's song with your own chart, and your own presets are sent to your devices. Without GigCraft, band members can open the address in Safari instead.")
                }
                }
                if invalid {
                    Text("That is not an address. Example: 192.168.1.20:8787").foregroundStyle(.red)
                }
            }
            .navigationTitle("Live")
            .onAppear { browser.start() }
            .onDisappear { browser.stop() }
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
    /// Where the chart's sections are and where it is scrolled.
    @State private var chartLayout = ChartLayout()
    @State private var scrollY: CGFloat = 0
    @State private var scrollTarget: CGFloat?
    @AppStorage("gc.live.followScroll") private var followScroll = true
    @Environment(\.horizontalSizeClass) private var sizeClass

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
                    let mark = model.live.mark.flatMap { $0.song == songId ? (block: $0.spot.block, fraction: $0.spot.fraction, label: $0.label) : nil }
                    ChartView(chart: chart, fontSize: size, mark: mark, onLayout: { chartLayout = $0 })
                } else if own != nil, let pdf = model.attachments(songId).first(where: { $0.kind == "pdf" }) {
                    PDFKitView(url: pdf.url).frame(minHeight: 900)
                } else {
                    Text("No chart. Edit the song to add one, or attach a PDF.").foregroundStyle(.secondary)
                }
            }
            .padding()
            .coordinateSpace(name: "gcScroll")
            .background(ScrollBridge(onScroll: { scrollY = $0 }, target: $scrollTarget))
        }
        // Scroll with the leader (unless switched off).
        .onChange(of: model.live.leaderScroll) { _, s in
            guard followScroll, let s, s.song == songId else { return }
            scrollTarget = s.spot.offset(tops: chartLayout.tops, end: chartLayout.end)
        }
        // A new band mark: go there once.
        .onChange(of: model.live.jump) { _, jump in
            guard let jump, jump.song == songId else { return }
            scrollTarget = jump.spot.offset(tops: chartLayout.tops, end: chartLayout.end)
            model.live.jump = nil
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
        let controls = HStack(spacing: 12) {
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
            Menu {
                Toggle(isOn: $followScroll) { Label("Scroll with the leader", systemImage: "arrow.up.and.down.text.horizontal") }
                Section("Band mark") {
                    MarkControls(canMark: live.snapshot?.current?.song != nil && !chartLayout.tops.isEmpty, hasMark: live.mark != nil,
                                 mark: {
                                     live.setMark(song: live.snapshot?.current?.song,
                                                  spot: ChartSpot.at(scrollY, tops: chartLayout.tops, end: chartLayout.end))
                                 },
                                 clear: { live.setMark(song: nil) })
                }
            } label: {
                Label("More", systemImage: "ellipsis.circle").labelStyle(.iconOnly).font(.title2).frame(minHeight: 34)
            }
            .accessibilityLabel("More")
            Button { live.toggleReady() } label: {
                Label("Ready", systemImage: live.ready ? "checkmark.circle.fill" : "circle").frame(minWidth: 110, minHeight: 34)
            }
            .buttonStyle(StageButtonStyle(prominent: !live.ready))
            .overlay(alignment: .topTrailing) { if live.ready { LED(color: .ledGreen).offset(x: 4, y: -4) } }
            .accessibilityValue(live.ready ? Text("ready") : Text("not ready"))
            .accessibilityHint("Tell the band you are ready for this song")
        }
        return Group {
            if sizeClass == .compact {
                // iPhone: what comes next above, the buttons below.
                VStack(alignment: .leading, spacing: 8) {
                    Text(nextText).font(.system(size: size * 0.7)).lineLimit(1)
                    controls
                }
            } else {
                HStack(spacing: 16) {
                    Text(nextText).font(.system(size: size * 0.8)).lineLimit(2)
                    Spacer()
                    controls
                }
            }
        }
        .padding()
    }
}
