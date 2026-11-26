import SwiftUI
import UIKit

/// A place in a chart that means the same on every screen: a section and how
/// far into it (the core's `gigcraft_live::Spot`).
struct ChartSpot: Equatable, Codable {
    var block: Int
    var fraction: Double

    /// The spot at `offset` in a chart whose sections start at `tops`.
    static func at(_ offset: CGFloat, tops: [CGFloat], end: CGFloat) -> ChartSpot {
        guard let block = tops.lastIndex(where: { $0 <= offset }) else { return ChartSpot(block: 0, fraction: 0) }
        let next = block + 1 < tops.count ? tops[block + 1] : end
        let span = max(1, next - tops[block])
        return ChartSpot(block: block, fraction: Double(min(1, max(0, (offset - tops[block]) / span))))
    }

    /// Where this spot is in a chart whose sections start at `tops`. The very
    /// top (start of the first section) is the top of the page, with the title.
    func offset(tops: [CGFloat], end: CGFloat) -> CGFloat? {
        if block == 0 && fraction <= 0 { return 0 }
        guard tops.indices.contains(block) else { return nil }
        let next = block + 1 < tops.count ? tops[block + 1] : end
        return tops[block] + max(0, next - tops[block]) * CGFloat(min(1, max(0, fraction)))
    }
}

/// A place marked for the whole band in a live session.
struct BandMark: Equatable, Decodable {
    var song: String
    var spot: ChartSpot
    var from: String
    var text: String

    var label: String {
        (text.isEmpty ? String(localized: "From here") : text) + " – " + from
    }
}

/// Where the leader's chart is in a live session.
struct LiveScroll: Equatable, Decodable {
    var song: String
    var spot: ChartSpot
}

/// Where the sections of a chart start, in the scroll content
/// (`.coordinateSpace(name: "gcScroll")`), and where the chart ends.
struct ChartLayout: Equatable {
    var tops: [CGFloat] = []
    var end: CGFloat = 0
}

/// Reads the position of the scroll view it sits in and scrolls it on request
/// (`target`, in content points; set back to nil once done).
struct ScrollBridge: UIViewRepresentable {
    var onScroll: (CGFloat) -> Void
    @Binding var target: CGFloat?

    func makeUIView(context: Context) -> BridgeView {
        let view = BridgeView()
        view.isUserInteractionEnabled = false
        view.isAccessibilityElement = false
        return view
    }

    func updateUIView(_ view: BridgeView, context: Context) {
        view.onScroll = onScroll
        if let y = target {
            view.scroll(to: y)
            DispatchQueue.main.async { target = nil }
        }
    }

    final class BridgeView: UIView {
        var onScroll: (CGFloat) -> Void = { _ in }
        private var observation: NSKeyValueObservation?
        private weak var scrollView: UIScrollView?

        override func didMoveToWindow() {
            super.didMoveToWindow()
            observation = nil
            guard window != nil else { return }
            var view = superview
            while let v = view, !(v is UIScrollView) { view = v.superview }
            guard let s = view as? UIScrollView else { return }
            scrollView = s
            observation = s.observe(\.contentOffset, options: [.new]) { [weak self] s, _ in
                let y = s.contentOffset.y + s.adjustedContentInset.top
                DispatchQueue.main.async { self?.onScroll(y) }
            }
        }

        func scroll(to y: CGFloat) {
            guard let s = scrollView else { return }
            let top = -s.adjustedContentInset.top
            let maxY = max(top, s.contentSize.height + s.adjustedContentInset.bottom - s.bounds.height)
            s.setContentOffset(CGPoint(x: s.contentOffset.x, y: min(maxY, max(top, y + top))), animated: true)
        }
    }
}

/// Section positions reported by `ChartView`.
struct ChartLayoutKey: PreferenceKey {
    static let defaultValue: [Int: CGFloat] = [:]
    static func reduce(value: inout [Int: CGFloat], nextValue: () -> [Int: CGFloat]) {
        value.merge(nextValue()) { $1 }
    }
}

/// "Mark here" and "Clear mark" in a live session, for menus.
struct MarkControls: View {
    let canMark: Bool
    let hasMark: Bool
    let mark: () -> Void
    let clear: () -> Void

    var body: some View {
        Button(action: mark) { Label("Mark here", systemImage: "mappin.and.ellipse") }
            .disabled(!canMark)
            .accessibilityHint("Mark the place at the top of your chart for the whole band, e.g. where a rehearsal goes on")
        if hasMark {
            Button(role: .destructive, action: clear) { Label("Clear mark", systemImage: "mappin.slash") }
        }
    }
}

/// Section tops in the scroll content (key -1: the chart's end).
struct ScrollTopsKey: PreferenceKey {
    static let defaultValue: [Int: CGFloat] = [:]
    static func reduce(value: inout [Int: CGFloat], nextValue: () -> [Int: CGFloat]) {
        value.merge(nextValue()) { $1 }
    }
}
