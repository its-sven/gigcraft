import SwiftUI

struct SongDetailView: View {
    @Environment(LibraryModel.self) private var model
    let songId: String
    var onEdit: () -> Void = {}
    @State private var transpose: Int8 = 0

    var body: some View {
        let song = model.song(songId)
        let chart = model.chart(songId, transpose: transpose)
        ScrollView {
            VStack(alignment: .leading, spacing: 12) {
                if let song {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(song.title).font(.system(size: model.fontSize * 1.5, weight: .semibold))
                        Text(facts(song, chart: chart)).font(.subheadline).foregroundStyle(.secondary)
                    }
                }
                if let chart {
                    ChartView(chart: chart, fontSize: model.fontSize)
                } else {
                    Text("No chart.").foregroundStyle(.secondary)
                }
            }
            .padding()
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .toolbar {
            ToolbarItemGroup {
                Button("Edit", action: onEdit)
                Button(role: .destructive) { model.trashSong(songId) } label: { Image(systemName: "trash") }
                    .accessibilityLabel("Move to trash")
                Button { step(-1) } label: { Image(systemName: "minus") }
                    .accessibilityLabel("Transpose down")
                Text(transpose == 0 ? "±0" : String(format: "%+d", transpose)).monospacedDigit()
                Button { step(1) } label: { Image(systemName: "plus") }
                    .accessibilityLabel("Transpose up")
            }
        }
        .navigationBarTitleDisplayMode(.inline)
    }

    private func step(_ d: Int8) {
        let n = transpose + d
        transpose = (n > 11 || n < -11) ? 0 : n
    }

    private func facts(_ s: SongSummary, chart: Chart?) -> String {
        var parts: [String] = []
        if !s.artist.isEmpty { parts.append(s.artist) }
        if let k = chart?.key ?? s.key { parts.append("Key \(k)") }
        if let t = s.tempoBpm { parts.append("\(Int(t)) BPM") }
        if let d = s.duration { parts.append(d) }
        return parts.joined(separator: " · ")
    }
}

/// Chords above lyrics; sections with headings; the chorus indented.
struct ChartView: View {
    let chart: Chart
    let fontSize: Double

    var body: some View {
        VStack(alignment: .leading, spacing: fontSize * 0.3) {
            ForEach(Array(chart.blocks.enumerated()), id: \.offset) { _, block in
                if let label = block.label {
                    Text(label)
                        .font(.system(size: fontSize * 0.8, weight: .bold))
                        .italic(block.kind == "chorus_recall")
                        .padding(.top, fontSize * 0.4)
                }
                VStack(alignment: .leading, spacing: fontSize * 0.25) {
                    ForEach(Array(block.lines.enumerated()), id: \.offset) { _, line in
                        lineView(line)
                    }
                }
                .padding(.leading, block.kind == "chorus" ? fontSize : 0)
            }
        }
    }

    @ViewBuilder
    private func lineView(_ line: ChartLine) -> some View {
        switch line {
        case .empty:
            Spacer().frame(height: fontSize * 0.4)
        case .comment(let text):
            Text(text).font(.system(size: fontSize * 0.85)).italic().foregroundStyle(.secondary)
        case .lyrics(let segments):
            let hasChords = segments.contains { $0.chord != nil }
            FlowLayout {
                ForEach(Array(segments.enumerated()), id: \.offset) { _, seg in
                    VStack(alignment: .leading, spacing: 0) {
                        if hasChords {
                            Text((seg.chord ?? " ") + " ")
                                .font(.system(size: fontSize, weight: .bold))
                                .foregroundStyle(.tint)
                        }
                        Text(seg.text.isEmpty ? " " : seg.text).font(.system(size: fontSize))
                    }
                }
            }
        }
    }
}

/// Lays out children left to right and wraps to the next row when full.
struct FlowLayout: Layout {
    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? .infinity
        var x: CGFloat = 0, y: CGFloat = 0, row: CGFloat = 0, maxX: CGFloat = 0
        for s in subviews {
            let size = s.sizeThatFits(.unspecified)
            if x + size.width > width, x > 0 { x = 0; y += row; row = 0 }
            x += size.width; row = max(row, size.height); maxX = max(maxX, x)
        }
        return CGSize(width: min(maxX, width), height: y + row)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x = bounds.minX, y = bounds.minY, row: CGFloat = 0
        for s in subviews {
            let size = s.sizeThatFits(.unspecified)
            if x + size.width > bounds.maxX, x > bounds.minX { x = bounds.minX; y += row; row = 0 }
            s.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
            x += size.width; row = max(row, size.height)
        }
    }
}
