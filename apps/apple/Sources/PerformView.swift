import SwiftUI
import UIKit

/// The stage view of a running setlist: big type, next/previous by tap,
/// buttons or a Bluetooth page turner, the screen stays on.
struct PerformView: View {
    @Environment(LibraryModel.self) private var model
    @FocusState private var focused: Bool
    @State private var breakEnds: Date?
    @State private var startedAt = Date()
    @AppStorage("gc.stage.lyricsOnly") private var stageLyricsOnly = false
    @AppStorage("gc.scroll.factor") private var scrollFactor = 1.0
    @Environment(\.horizontalSizeClass) private var sizeClass
    /// Live session: where the chart's sections are and where it is scrolled.
    @State private var chartLayout = ChartLayout()
    @State private var scrollY: CGFloat = 0
    @State private var scrollTarget: CGFloat?

    private var currentSongId: String? {
        ((model.performState?["current"] as? [String: Any])?["song"] as? String)
    }

    var body: some View {
        let st = model.performState ?? [:]
        let cur = st["current"] as? [String: Any] ?? [:]
        let size = model.fontSize * 1.3
        VStack(spacing: 0) {
            header(st)
            Divider()
            Group {
                switch cur["type"] as? String {
                case "song": song(cur, st: st, size: size)
                case "break": breakView(cur, size: size)
                case "missing": Text("This song is no longer in the library.").font(.title)
                default: Text("This setlist is empty.").font(.title)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .overlay(alignment: .top) {
                TimelineView(.periodic(from: .now, by: 1)) { ctx in
                    if let cue = model.leader.cue, ctx.date.timeIntervalSince(cue.at) < 8 {
                        Text(verbatim: "\(cue.from): \(cue.text)")
                            .font(.system(size: size * 1.1, weight: .bold))
                            .foregroundStyle(.black)
                            .padding()
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(Color.accentColor, in: .rect(cornerRadius: 12))
                            .padding()
                    }
                }
            }
            .contentShape(Rectangle())
            .accessibilityAction(named: Text("Next")) { model.perform("next") }
            .accessibilityAction(named: Text("Previous")) { model.perform("previous") }
            .accessibilityAction(named: Text("Next song")) { model.perform("next_song") }
            .accessibilityAction(named: Text("Previous song")) { model.perform("previous_song") }
            .gesture(SpatialTapGesture().onEnded { tap in
                // Tap on the right third: next; on the left third: previous.
                let width = UIScreen.main.bounds.width
                if tap.location.x > width * 2 / 3 { model.perform("next") }
                else if tap.location.x < width / 3 { model.perform("previous") }
            })
            Divider()
            footer(st, size: size)
        }
        .focusable()
        .focused($focused)
        .focusEffectDisabled()
        .onKeyPress(.rightArrow) { model.perform("next"); return .handled }
        .onKeyPress(.space) { model.perform("next"); return .handled }
        .onKeyPress(.leftArrow) { model.perform("previous"); return .handled }
        .onKeyPress(.downArrow) { model.perform("next"); return .handled }
        .onKeyPress(.upArrow) { model.perform("previous"); return .handled }
        .onKeyPress(.pageDown) { model.perform("next"); return .handled }
        .onKeyPress(.pageUp) { model.perform("previous"); return .handled }
        .onKeyPress(.escape) { model.stopPerform(); return .handled }
        .onKeyPress("p") { model.perform("playback"); return .handled }
        .onKeyPress("s") { model.perform("auto_scroll"); return .handled }
        .onAppear {
            focused = true
            UIApplication.shared.isIdleTimerDisabled = true
        }
        .onDisappear { UIApplication.shared.isIdleTimerDisabled = false }
        .onChange(of: (cur["type"] as? String) ?? "") { _, type in
            if type == "break", let m = cur["minutes"] as? Int { breakEnds = Date().addingTimeInterval(Double(m) * 60) } else { breakEnds = nil }
        }
        .statusBarHidden()
        .preferredColorScheme(.dark)
    }

    private func header(_ st: [String: Any]) -> some View {
        let name = st["name"] as? String ?? ""
        let position = ((st["index"] as? Int) ?? 0) + 1
        let count = st["count"] as? Int ?? 0
        let song = Readout(label: "Song", value: "\(position)/\(count)", size: 15)
            .accessibilityLabel("Item \(position) of \(count)")
        return Group {
            if sizeClass == .compact {
                // iPhone: two rows, the rest behind "…".
                VStack(alignment: .leading, spacing: 6) {
                    HStack {
                        Text(name).bold().lineLimit(1)
                        Spacer()
                        Button("Exit") { model.stopPerform() }.bold()
                    }
                    HStack(spacing: 8) {
                        song
                        transport(st, compact: true)
                        band
                        Spacer(minLength: 4)
                        scrollButton
                        moreMenu
                    }
                }
            } else {
                HStack {
                    Text(name).bold().lineLimit(1)
                    song
                    TimelineView(.periodic(from: .now, by: 1)) { ctx in
                        let s = Int(ctx.date.timeIntervalSince(startedAt))
                        Readout(label: "On stage", value: String(format: "%d:%02d", s / 60, s % 60), size: 15)
                    }
                    transport(st, compact: false)
                    band
                    Spacer()
                    scrollButton
                    if model.autoScrolling {
                        Readout(label: "Scroll", value: "\(Int(scrollFactor * 100))%", size: 15)
                    }
                    moreMenu
                    Button("Panic", role: .destructive) { model.perform("panic") }
                    Button("Exit") { model.stopPerform() }.bold()
                }
            }
        }
        .padding(.horizontal).padding(.vertical, 8)
    }

    @ViewBuilder private var band: some View {
        if model.leader.active {
            Readout(label: "Band", value: "\(model.leader.readyCount)/\(model.leader.roster.count)", size: 15)
                .accessibilityLabel("\(model.leader.readyCount) of \(model.leader.roster.count) band members ready")
        }
    }

    private var scrollButton: some View {
        Button { model.perform("auto_scroll") } label: {
            Label(model.autoScrolling ? "Stop scrolling" : "Auto-scroll",
                  systemImage: model.autoScrolling ? "pause.circle.fill" : "arrow.down.circle")
                .labelStyle(.iconOnly)
        }
        .buttonStyle(StageButtonStyle(prominent: model.autoScrolling))
        .accessibilityHint("The chart scrolls by itself, as fast as the song is long (key S). Touch the chart to pause.")
    }

    private var moreMenu: some View {
        Menu {
            Section("Auto-scroll") {
                Button { scrollFactor = AutoScrollSpeed.step(scrollFactor, up: false) } label: {
                    Label("Scroll slower", systemImage: "tortoise")
                }
                Button { scrollFactor = AutoScrollSpeed.step(scrollFactor, up: true) } label: {
                    Label("Scroll faster", systemImage: "hare")
                }
            }
            if model.leader.active {
                Section("Band") {
                    MarkControls(canMark: currentSongId != nil && !chartLayout.tops.isEmpty, hasMark: model.leader.mark != nil,
                                 mark: {
                                     model.leader.setMark(song: currentSongId,
                                                          spot: ChartSpot.at(scrollY, tops: chartLayout.tops, end: chartLayout.end))
                                 },
                                 clear: { model.leader.setMark(song: nil) })
                }
            }
            if UIApplication.shared.connectedScenes.contains(where: { $0.session.role == .windowExternalDisplayNonInteractive }) {
                Picker(selection: $stageLyricsOnly) {
                    Text("Chart with chords (band)").tag(false)
                    Text("Lyrics only (singers, audience)").tag(true)
                } label: {
                    Label("Screen 2", systemImage: "rectangle.on.rectangle")
                }
            }
            Section {
                Button(role: .destructive) { model.perform("panic") } label: {
                    Label("Panic: all notes off, stop", systemImage: "exclamationmark.octagon")
                }
            }
        } label: {
            Label("More", systemImage: "ellipsis.circle").labelStyle(.iconOnly).font(.title3)
        }
        .accessibilityLabel("More")
    }

    /// Play/stop and position, for songs with backing tracks or a click.
    @ViewBuilder
    private func transport(_ st: [String: Any], compact: Bool) -> some View {
        let songId = (st["current"] as? [String: Any])?["song"] as? String
        let audio = model.audio
        if let songId, audio.loadingSong == songId {
            ProgressView().controlSize(.small)
            if !compact { Text("Loading backing tracks…").font(.caption).foregroundStyle(.secondary) }
        } else if let songId, audio.readySong == songId {
            let status = audio.status
            let playing = status?.playing ?? false
            Button { model.perform("playback") } label: {
                Label(playing ? "Stop" : "Play", systemImage: playing ? "stop.fill" : "play.fill")
                    .labelStyle(AdaptiveLabelStyle(iconOnly: compact))
            }
            .buttonStyle(StageButtonStyle(prominent: !playing))
            .accessibilityLabel(playing ? "Stop" : "Play")
            .accessibilityHint("Start or stop the backing tracks and click (key P)")
            if let status {
                Readout(label: "Track", value: status.countingIn ? String(localized: "count-in") : clock(status.positionSecs, status.lengthSecs), size: 15)
            }
        }
    }

    private func clock(_ pos: Double, _ len: Double) -> String {
        func mmss(_ s: Double) -> String { let t = max(0, Int(s)); return String(format: "%d:%02d", t / 60, t % 60) }
        return len > 0 ? "\(mmss(pos)) / \(mmss(len))" : mmss(pos)
    }

    private func song(_ cur: [String: Any], st: [String: Any], size: Double) -> some View {
        let songId = cur["song"] as? String ?? ""
        let gigKey = cur["key"] as? String
        let songKey = model.song(songId)?.key
        let semis = model.semitones(songKey: songKey, gigKey: gigKey)
        let parts = cur["part_names"] as? [String] ?? []
        let part = st["part"] as? Int ?? 0
        let index = st["index"] as? Int ?? 0
        let chart = model.chart(songId, transpose: semis)
        let section = chart.flatMap { model.playingSection(songId: songId, chart: $0) }
        return ScrollViewReader { proxy in
        ScrollView {
            VStack(alignment: .leading, spacing: 10) {
                HStack(alignment: .center, spacing: 12) {
                    Text(cur["title"] as? String ?? "").font(.system(size: scaled(size * 1.6), weight: .bold))
                        .accessibilityAddTraits(.isHeader)
                    if let k = gigKey { Readout(label: "Key", value: k, size: size * 0.8) }
                    if let t = cur["tempo_bpm"] as? Double { Readout(label: "BPM", value: "\(Int(t))", size: size * 0.8) }
                }
                if parts.count > 1 {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack {
                            ForEach(Array(parts.enumerated()), id: \.offset) { i, name in
                                Button(name) { model.performGoto(index: index, part: i) }
                                    .buttonStyle(.bordered)
                                    .tint(i == part ? .accentColor : .secondary)
                                    .font(.system(size: size * 0.7, weight: i == part ? .bold : .regular))
                                    .accessibilityAddTraits(i == part ? .isSelected : [])
                            }
                        }
                    }
                }
                if let n = cur["gig_notes"] as? String, !n.isEmpty {
                    Text(n).font(.system(size: size)).foregroundStyle(.orange)
                }
                if let chart, !chart.blocks.isEmpty {
                    let mark = model.leader.mark.flatMap { $0.song == songId ? (block: $0.spot.block, fraction: $0.spot.fraction, label: $0.label) : nil }
                    ChartView(chart: chart, fontSize: size, highlight: section, followsTrack: true,
                              mark: mark, onLayout: model.leader.active ? { chartLayout = $0 } : nil)
                } else if let pdf = model.attachments(songId).first(where: { $0.kind == "pdf" }) {
                    PDFKitView(url: pdf.url).frame(minHeight: 900)
                } else if let link = model.song(songId)?.ultimateGuitar, let url = URL(string: link) {
                    // Imported from Ultimate Guitar: the chart is one tap away.
                    VStack(alignment: .leading, spacing: 12) {
                        Text("The chart is on Ultimate Guitar.").font(.system(size: size * 0.8)).foregroundStyle(.secondary)
                        Link(destination: url) {
                            Label("Open on Ultimate Guitar", systemImage: "safari").frame(minHeight: 34)
                        }
                        .buttonStyle(StageButtonStyle(prominent: true))
                    }
                    .padding(.top, 8)
                }
            }
            .padding()
            .coordinateSpace(name: "gcScroll")
            .background(ScrollBridge(onScroll: { y in
                scrollY = y
                // Leading a band: followers who follow the scrolling go along.
                if model.leader.active, !chartLayout.tops.isEmpty {
                    model.leader.scroll(song: songId, spot: ChartSpot.at(y, tops: chartLayout.tops, end: chartLayout.end))
                }
            }, target: $scrollTarget))
            .background(AutoScrollDriver(running: Binding(get: { model.autoScrolling }, set: { model.autoScrolling = $0 })) { content, visible in
                AutoScrollSpeed.pointsPerSecond(content: content, visible: visible,
                                                lengthSecs: AutoScrollSpeed.seconds(model.song(songId)?.duration),
                                                fontSize: scaled(size), factor: scrollFactor)
            })
        }
        // A band member marked a place: go there once.
        .onChange(of: model.leader.jump) { _, jump in
            guard let jump, jump.song == songId else { return }
            scrollTarget = jump.spot.offset(tops: chartLayout.tops, end: chartLayout.end)
            model.leader.jump = nil
        }
        // Follow the backing track: scroll when it enters the next section.
        .onChange(of: section) { _, new in
            if let new { withAnimation(.easeInOut(duration: 0.4)) { proxy.scrollTo(new, anchor: .top) } }
        }
        }
    }

    private func breakView(_ cur: [String: Any], size: Double) -> some View {
        VStack(spacing: 20) {
            Spacer()
            let label = (cur["label"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? String(localized: "Break")
            Text(label).font(.system(size: size * 2, weight: .bold))
            if let ends = breakEnds {
                TimelineView(.periodic(from: .now, by: 1)) { ctx in
                    let left = max(0, Int(ends.timeIntervalSince(ctx.date)))
                    Text(verbatim: String(format: "%d:%02d", left / 60, left % 60)).font(.system(size: size * 3).monospacedDigit())
                        .accessibilityLabel("Break ends in \(String(format: "%d:%02d", left / 60, left % 60))")
                }
            }
            Spacer()
        }
        .frame(maxWidth: .infinity)
    }

    private func footer(_ st: [String: Any], size: Double) -> some View {
        let next = st["next"] as? [String: Any]
        let nextText: String = {
            guard let next else { return String(localized: "Last song") }
            switch next["type"] as? String {
            case "song":
                let title = (next["title"] as? String ?? "") + ((next["key"] as? String).map { "  [\($0)]" } ?? "")
                return String(localized: "Up next: \(title)")
            case "break": return String(localized: "Up next: break \(next["minutes"] as? Int ?? 0) min")
            default: return String(localized: "Up next: (missing song)")
            }
        }()
        let compact = sizeClass == .compact
        let buttons = HStack(spacing: compact ? 10 : 16) {
            Button { model.perform("previous") } label: {
                Label("Previous", systemImage: "chevron.left").frame(maxWidth: compact ? .infinity : nil, minHeight: 34).frame(minWidth: compact ? 0 : 120)
            }
            .buttonStyle(StageButtonStyle())
            Button { model.perform("next") } label: {
                Label("Next", systemImage: "chevron.right").frame(maxWidth: compact ? .infinity : nil, minHeight: 34).frame(minWidth: compact ? 0 : 120)
            }
            .buttonStyle(StageButtonStyle(prominent: true))
        }
        let status: String? = !model.rig.status.isEmpty ? model.rig.status.joined(separator: "   ")
            : (!model.lastSent.isEmpty ? String(localized: "Presets: \(model.lastSent.joined(separator: "; "))") : nil)
        return Group {
            if compact {
                // iPhone: what comes next above, wide buttons below.
                VStack(alignment: .leading, spacing: 8) {
                    Text(nextText).font(.system(size: size * 0.7)).lineLimit(1)
                    buttons
                }
            } else {
                HStack(spacing: 16) {
                    buttons
                    Text(nextText).font(.system(size: size * 0.8)).lineLimit(1)
                    Spacer()
                    if let status { Text(status).font(.caption).foregroundStyle(.secondary).lineLimit(1) }
                }
            }
        }
        .padding()
        .font(.system(size: size * 0.8))
    }
}

/// Icon only on narrow screens, icon and title otherwise.
struct AdaptiveLabelStyle: LabelStyle {
    let iconOnly: Bool
    func makeBody(configuration: Configuration) -> some View {
        if iconOnly {
            configuration.icon
        } else {
            HStack(spacing: 6) { configuration.icon; configuration.title }
        }
    }
}
