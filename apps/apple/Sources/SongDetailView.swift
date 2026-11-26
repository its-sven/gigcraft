import SwiftUI

struct SongDetailView: View {
    @Environment(LibraryModel.self) private var model
    let songId: String
    var onEdit: () -> Void = {}
    @State private var transpose: Int8 = 0
    @State private var showingAttachment: AttachmentInfo?
    @Environment(\.horizontalSizeClass) private var sizeClass
    @AppStorage("gc.scroll.factor") private var scrollFactor = 1.0

    var body: some View {
        @Bindable var model = model
        let song = model.song(songId)
        let chart = model.chart(songId, transpose: transpose)
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if let song {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(song.title).font(.system(size: scaled(model.fontSize * 1.5), weight: .semibold))
                            .accessibilityAddTraits(.isHeader)
                        if !song.artist.isEmpty { Text(song.artist).font(.title3).foregroundStyle(.secondary) }
                        // Wraps on the iPhone.
                        FlowLayout(spacing: 8) {
                            if let k = chart?.key ?? song.key { Readout(label: "Key", value: k, size: 15) }
                            if transpose != 0 {
                                Readout(label: "Transpose", value: String(format: "%+d", transpose), size: 15)
                            }
                            if let c = song.capo { Readout(label: "Capo", value: "\(c)", size: 15) }
                            if let t = song.tempoBpm { Readout(label: "BPM", value: "\(Int(t))", size: 15) }
                            if let d = song.duration { Readout(label: "Length", value: d, size: 15) }
                            if model.autoScrolling {
                                Readout(label: "Scroll", value: "\(Int(scrollFactor * 100))%", size: 15)
                            }
                        }
                        if let tuning = song.tuning {
                            Label(tuning, systemImage: "tuningfork").font(.callout).foregroundStyle(.secondary)
                        }
                        if let link = song.ultimateGuitar, let url = URL(string: link) {
                            Link(destination: url) {
                                Label("Open on Ultimate Guitar", systemImage: "link")
                            }
                            .buttonStyle(.bordered)
                            .tint(Color("ChordColor"))
                            .contextMenu {
                                Button { UIPasteboard.general.url = url } label: { Label("Copy link", systemImage: "doc.on.doc") }
                                ShareLink(item: url) { Label("Share link", systemImage: "square.and.arrow.up") }
                            }
                            .accessibilityHint("The chart stays on Ultimate Guitar; GigCraft keeps the link")
                        }
                    }
                }
                AttachmentBar(songId: songId, showing: $showingAttachment)
                // Without a chart, the first PDF is the chart.
                let shown = showingAttachment ?? ((chart?.blocks.isEmpty ?? true) ? model.attachments(songId).first { $0.kind == "pdf" } : nil)
                if let a = shown, a.kind == "pdf" {
                    PDFKitView(url: a.url).frame(minHeight: 700)
                } else if let a = shown, a.kind == "image", let img = UIImage(contentsOfFile: a.url.path(percentEncoded: false)) {
                    Image(uiImage: img).resizable().scaledToFit()
                } else if let chart, !chart.blocks.isEmpty {
                    ChartView(chart: chart, fontSize: model.fontSize)
                } else {
                    if song?.ultimateGuitar != nil {
                        Text("The chart is on Ultimate Guitar (button above). To have it here too, paste its text in the editor.").foregroundStyle(.secondary)
                    } else {
                        Text("No chart. Edit the song to add one, or attach a PDF.").foregroundStyle(.secondary)
                    }
                }
            }
            .padding()
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(AutoScrollDriver(running: $model.autoScrolling) { content, visible in
                AutoScrollSpeed.pointsPerSecond(content: content, visible: visible,
                                                lengthSecs: AutoScrollSpeed.seconds(song?.duration),
                                                fontSize: scaled(model.fontSize), factor: scrollFactor)
            })
        }
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                if sizeClass == .regular {
                    // iPad: transpose right in the bar.
                    Button { step(-1) } label: { Image(systemName: "minus") }
                        .accessibilityLabel("Transpose down")
                    Text(verbatim: transpose == 0 ? "±0" : String(format: "%+d", transpose)).monospacedDigit()
                        .accessibilityLabel("Transposition")
                        .accessibilityValue(transpose == 0 ? String(localized: "original key") : String(localized: "\(Int(transpose)) semitones"))
                    Button { step(1) } label: { Image(systemName: "plus") }
                        .accessibilityLabel("Transpose up")
                }
                Button { model.autoScrolling.toggle() } label: {
                    Image(systemName: model.autoScrolling ? "pause.circle.fill" : "arrow.down.circle")
                }
                .accessibilityLabel(model.autoScrolling ? "Stop scrolling" : "Auto-scroll")
                .accessibilityHint("The chart scrolls by itself, as fast as the song is long. Touch the chart to pause.")
                moreMenu(song)
            }
        }
        .navigationBarTitleDisplayMode(.inline)
        .onChange(of: model.autoScrolling) { _, on in UIApplication.shared.isIdleTimerDisabled = on }
        .onDisappear {
            model.autoScrolling = false
            UIApplication.shared.isIdleTimerDisabled = false
        }
    }

    /// Everything else behind "…", with icons (fits the iPhone).
    private func moreMenu(_ song: SongSummary?) -> some View {
        Menu {
            Button(action: onEdit) { Label("Edit", systemImage: "pencil") }
            Section("Transpose") {
                Button { step(1) } label: { Label("Transpose up", systemImage: "plus") }
                Button { step(-1) } label: { Label("Transpose down", systemImage: "minus") }
                if transpose != 0 {
                    Button { transpose = 0 } label: { Label("Original key", systemImage: "arrow.uturn.backward") }
                }
            }
            Section("Auto-scroll") {
                AutoScrollControls(running: Binding(get: { model.autoScrolling }, set: { model.autoScrolling = $0 }))
            }
            if let link = song?.ultimateGuitar, let url = URL(string: link) {
                Section("Ultimate Guitar") {
                    Link(destination: url) { Label("Open on Ultimate Guitar", systemImage: "safari") }
                    Button { UIPasteboard.general.url = url } label: { Label("Copy link", systemImage: "doc.on.doc") }
                    ShareLink(item: url) { Label("Share link", systemImage: "square.and.arrow.up") }
                }
            }
            Section {
                Button(role: .destructive) { model.trashSong(songId) } label: { Label("Move to trash", systemImage: "trash") }
            }
        } label: {
            Label("More", systemImage: "ellipsis.circle")
        }
        .accessibilityLabel("More")
    }

    private func step(_ d: Int8) {
        let n = transpose + d
        transpose = (n > 11 || n < -11) ? 0 : n
    }
}

/// Chords above lyrics; sections with headings; the chorus indented.
struct ChartView: View {
    let chart: Chart
    /// The size the user picked; larger accessibility text sizes scale it up.
    let baseSize: Double
    /// The section the backing track is in (marked with an amber bar).
    var highlight: Int?
    /// Lyrics without chords (singers, audience screens).
    var lyricsOnly = false
    /// Leave room on the left for the section marker (stage views).
    var followsTrack = false
    /// The band's mark in a live session: section, how far into it, label.
    var mark: (block: Int, fraction: Double, label: String)?
    /// Where the sections start in the scroll content (`gcScroll` space).
    var onLayout: ((ChartLayout) -> Void)?
    /// Section tops in this view, for drawing the mark.
    @State private var localTops: [Int: CGFloat] = [:]
    @State private var height: CGFloat = 0

    init(chart: Chart, fontSize: Double, highlight: Int? = nil, lyricsOnly: Bool = false, followsTrack: Bool = false,
         mark: (block: Int, fraction: Double, label: String)? = nil, onLayout: ((ChartLayout) -> Void)? = nil) {
        self.chart = chart
        self.baseSize = fontSize
        self.highlight = highlight
        self.lyricsOnly = lyricsOnly
        self.followsTrack = followsTrack
        self.mark = mark
        self.onLayout = onLayout
    }

    /// Where the mark goes in this view.
    private var markY: CGFloat? {
        guard let mark, let top = localTops[mark.block] else { return nil }
        let next = localTops[mark.block + 1] ?? height
        return top + max(0, next - top) * CGFloat(min(1, max(0, mark.fraction)))
    }

    private var fontSize: Double { scaled(baseSize) }

    var body: some View {
        VStack(alignment: .leading, spacing: fontSize * 0.3) {
            ForEach(Array(chart.blocks.enumerated()), id: \.offset) { index, block in
                VStack(alignment: .leading, spacing: fontSize * 0.3) {
                    if let label = block.label {
                        Text(label)
                            .font(.system(size: fontSize * 0.8, weight: .bold))
                            .italic(block.kind == "chorus_recall")
                            .padding(.top, fontSize * 0.4)
                            .accessibilityAddTraits(.isHeader)
                    }
                    VStack(alignment: .leading, spacing: fontSize * 0.25) {
                        ForEach(Array(block.lines.enumerated()), id: \.offset) { _, line in
                            lineView(line)
                        }
                    }
                    .padding(.leading, block.kind == "chorus" ? fontSize : 0)
                }
                .padding(.leading, followsTrack ? 14 : 0)
                .overlay(alignment: .leading) {
                    if highlight == index {
                        RoundedRectangle(cornerRadius: 2).fill(Color.accentColor).frame(width: 4)
                            .accessibilityHidden(true)
                    }
                }
                .id(index)
                .background {
                    if mark != nil || onLayout != nil {
                        GeometryReader { g in
                            Color.clear
                                .preference(key: ChartLayoutKey.self, value: [index: g.frame(in: .named("gcChart")).minY])
                                .preference(key: ScrollTopsKey.self, value: [index: g.frame(in: .named("gcScroll")).minY])
                        }
                    }
                }
            }
        }
        .coordinateSpace(name: "gcChart")
        .background {
            if mark != nil || onLayout != nil {
                GeometryReader { g in
                    Color.clear
                        .preference(key: ScrollTopsKey.self, value: [-1: g.frame(in: .named("gcScroll")).maxY])
                        .onAppear { height = g.size.height }
                        .onChange(of: g.size.height) { _, h in height = h }
                }
            }
        }
        .onPreferenceChange(ChartLayoutKey.self) { localTops = $0 }
        .onPreferenceChange(ScrollTopsKey.self) { value in
            guard let onLayout else { return }
            let tops = value.filter { $0.key >= 0 }.sorted { $0.key < $1.key }.map(\.value)
            onLayout(ChartLayout(tops: tops, end: value[-1] ?? tops.last ?? 0))
        }
        .overlay(alignment: .topLeading) {
            if let mark, let y = markY {
                ZStack(alignment: .bottomTrailing) {
                    Rectangle().fill(Color.accentColor).frame(height: 2)
                    Text(verbatim: mark.label)
                        .font(.caption.bold())
                        .foregroundStyle(.black)
                        .padding(.horizontal, 8).padding(.vertical, 2)
                        .background(Color.accentColor, in: .rect(cornerRadius: 6))
                        .offset(y: -4)
                }
                .offset(y: y)
                .accessibilityElement(children: .combine)
                .accessibilityLabel(Text("Band mark: \(mark.label)"))
            }
        }
    }

    @ViewBuilder
    private func lineView(_ line: ChartLine) -> some View {
        switch line {
        case .empty:
            Spacer().frame(height: fontSize * 0.4).accessibilityHidden(true)
        case .comment(let text):
            Text(text).font(.system(size: fontSize * 0.85)).italic().foregroundStyle(.secondary)
        case .lyrics(let segments) where lyricsOnly:
            Text(segments.map(\.text).joined().trimmingCharacters(in: .whitespaces)).font(.system(size: fontSize))
        case .lyrics(let segments):
            let hasChords = segments.contains { $0.chord != nil }
            FlowLayout {
                ForEach(Array(segments.enumerated()), id: \.offset) { _, seg in
                    VStack(alignment: .leading, spacing: 0) {
                        if hasChords {
                            Text((seg.chord ?? " ") + " ")
                                .font(.system(size: fontSize, weight: .bold))
                                .foregroundStyle(Color.chord)
                        }
                        Text(seg.text.isEmpty ? " " : seg.text).font(.system(size: fontSize))
                    }
                }
            }
            // VoiceOver reads the line as one sentence, the chords after it.
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(segments.map(\.text).joined())
            .accessibilityValue(hasChords ? String(localized: "Chords: \(segments.compactMap(\.chord).joined(separator: ", "))") : "")
        }
    }
}

/// `size` scaled for the user's text size setting (unchanged at the default size).
func scaled(_ size: Double) -> Double {
    Double(UIFontMetrics(forTextStyle: .body).scaledValue(for: CGFloat(size)))
}

/// Lays out children left to right and wraps to the next row when full.
struct FlowLayout: Layout {
    /// Room between children and between rows.
    var spacing: CGFloat = 0

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? .infinity
        var x: CGFloat = 0, y: CGFloat = 0, row: CGFloat = 0, maxX: CGFloat = 0
        for s in subviews {
            let size = s.sizeThatFits(.unspecified)
            if x + size.width > width, x > 0 { x = 0; y += row + spacing; row = 0 }
            x += size.width; row = max(row, size.height); maxX = max(maxX, x)
            x += spacing
        }
        return CGSize(width: min(maxX, width), height: y + row)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x = bounds.minX, y = bounds.minY, row: CGFloat = 0
        for s in subviews {
            let size = s.sizeThatFits(.unspecified)
            if x + size.width > bounds.maxX, x > bounds.minX { x = bounds.minX; y += row + spacing; row = 0 }
            s.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
            x += size.width + spacing; row = max(row, size.height)
        }
    }
}
