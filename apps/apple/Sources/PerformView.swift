import SwiftUI
import UIKit

/// The stage view of a running setlist: big type, next/previous by tap,
/// buttons or a Bluetooth page turner, the screen stays on.
struct PerformView: View {
    @Environment(LibraryModel.self) private var model
    @FocusState private var focused: Bool
    @State private var breakEnds: Date?
    @State private var startedAt = Date()

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
        .onAppear {
            focused = true
            UIApplication.shared.isIdleTimerDisabled = true
        }
        .onDisappear { UIApplication.shared.isIdleTimerDisabled = false }
        .onChange(of: (cur["type"] as? String) ?? "") { _, type in
            if type == "break", let m = cur["minutes"] as? Int { breakEnds = Date().addingTimeInterval(Double(m) * 60) } else { breakEnds = nil }
        }
        .statusBarHidden()
    }

    private func header(_ st: [String: Any]) -> some View {
        HStack {
            Text(st["name"] as? String ?? "").bold()
            let position = ((st["index"] as? Int) ?? 0) + 1
            let count = st["count"] as? Int ?? 0
            Text(verbatim: "\(position) / \(count)").foregroundStyle(.secondary)
                .accessibilityLabel("Item \(position) of \(count)")
            TimelineView(.periodic(from: .now, by: 1)) { ctx in
                let s = Int(ctx.date.timeIntervalSince(startedAt))
                Text("on stage \(String(format: "%d:%02d", s / 60, s % 60))").foregroundStyle(.secondary).monospacedDigit()
            }
            Spacer()
            Button("Panic", role: .destructive) { model.perform("panic") }
            Button("Exit") { model.stopPerform() }.bold()
        }
        .padding(.horizontal).padding(.vertical, 8)
    }

    private func song(_ cur: [String: Any], st: [String: Any], size: Double) -> some View {
        let songId = cur["song"] as? String ?? ""
        let gigKey = cur["key"] as? String
        let songKey = model.song(songId)?.key
        let semis = model.semitones(songKey: songKey, gigKey: gigKey)
        let parts = cur["part_names"] as? [String] ?? []
        let part = st["part"] as? Int ?? 0
        let index = st["index"] as? Int ?? 0
        return ScrollView {
            VStack(alignment: .leading, spacing: 10) {
                HStack(alignment: .firstTextBaseline, spacing: 16) {
                    Text(cur["title"] as? String ?? "").font(.system(size: scaled(size * 1.6), weight: .bold))
                        .accessibilityAddTraits(.isHeader)
                    if let k = gigKey { Text("Key \(k)").font(.system(size: size)).foregroundStyle(.tint) }
                    if let t = cur["tempo_bpm"] as? Double { Text(verbatim: "\(Int(t)) BPM").font(.system(size: size * 0.8)).foregroundStyle(.secondary) }
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
                if let chart = model.chart(songId, transpose: semis), !chart.blocks.isEmpty {
                    ChartView(chart: chart, fontSize: size)
                } else if let pdf = model.attachments(songId).first(where: { $0.kind == "pdf" }) {
                    PDFKitView(url: pdf.url).frame(minHeight: 900)
                }
            }
            .padding()
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
        return HStack(spacing: 16) {
            Button { model.perform("previous") } label: { Label("Previous", systemImage: "chevron.left").frame(minWidth: 140, minHeight: 44) }
                .buttonStyle(.bordered)
            Button { model.perform("next") } label: { Label("Next", systemImage: "chevron.right").frame(minWidth: 140, minHeight: 44) }
                .buttonStyle(.borderedProminent)
            Text(nextText).font(.system(size: size * 0.8)).lineLimit(1)
            Spacer()
            if !model.rig.status.isEmpty {
                Text(model.rig.status.joined(separator: "   ")).font(.caption).foregroundStyle(.secondary).lineLimit(1)
            } else if !model.lastSent.isEmpty {
                Text("Presets: \(model.lastSent.joined(separator: "; "))").font(.caption).foregroundStyle(.secondary).lineLimit(1)
            }
        }
        .padding()
        .font(.system(size: size * 0.8))
    }
}
