import SwiftUI
import UIKit

/// Moves the scroll view it sits in downwards while `running`: as fast as
/// `speed` says (points per second, from the content and visible heights).
/// A finger on the chart pauses it; it goes on from the new place. At the end
/// it stops and sets `running` back to false.
struct AutoScrollDriver: UIViewRepresentable {
    @Binding var running: Bool
    var speed: (_ content: CGFloat, _ visible: CGFloat) -> CGFloat

    func makeUIView(context: Context) -> DriverView {
        let view = DriverView()
        view.isUserInteractionEnabled = false
        view.isAccessibilityElement = false
        return view
    }

    func updateUIView(_ view: DriverView, context: Context) {
        view.speed = speed
        view.onEnd = { running = false }
        view.running = running
    }

    static func dismantleUIView(_ view: DriverView, coordinator: ()) { view.running = false }

    final class DriverView: UIView {
        var speed: (CGFloat, CGFloat) -> CGFloat = { _, _ in 0 }
        var onEnd: () -> Void = {}
        var running = false {
            didSet { if running != oldValue { running ? start() : stop() } }
        }
        private var link: CADisplayLink?
        private var last: CFTimeInterval = 0
        /// The exact position: a step is often less than a pixel, which
        /// UIKit would round away.
        private var exact: CGFloat = 0
        private weak var scroll: UIScrollView?

        private func enclosingScrollView() -> UIScrollView? {
            var view = superview
            while let v = view {
                if let s = v as? UIScrollView { return s }
                view = v.superview
            }
            return nil
        }

        private func start() {
            guard link == nil, window != nil else { return }
            last = 0
            exact = -1000
            let l = CADisplayLink(target: self, selector: #selector(tick))
            l.add(to: .main, forMode: .common)
            link = l
        }

        private func stop() {
            link?.invalidate()
            link = nil
        }

        override func didMoveToWindow() {
            super.didMoveToWindow()
            if window == nil { stop() } else if running { start() }
        }

        @objc private func tick(_ l: CADisplayLink) {
            defer { last = l.timestamp }
            guard let s = scroll ?? enclosingScrollView() else { return }
            scroll = s
            // Hands on the chart: wait.
            guard last > 0, !s.isTracking, !s.isDecelerating else { return }
            let dt = min(0.1, l.timestamp - last)
            let end = s.contentSize.height + s.adjustedContentInset.bottom - s.bounds.height
            if s.contentOffset.y >= end - 0.5 {
                stop()
                let done = onEnd
                DispatchQueue.main.async { done() }
                return
            }
            // Moved by hand (or by the app): go on from there.
            if abs(s.contentOffset.y - exact) > 1 { exact = s.contentOffset.y }
            exact = min(end, exact + speed(s.contentSize.height, s.bounds.height) * dt)
            s.contentOffset.y = exact
        }
    }
}

/// Auto-scroll speed: the chart goes by in the song's length (with a little
/// left at the end); without a length, about a line every two seconds.
enum AutoScrollSpeed {
    static let factors: [Double] = [0.5, 0.6, 0.75, 0.9, 1.0, 1.15, 1.3, 1.6, 2.0]

    static func pointsPerSecond(content: CGFloat, visible: CGFloat, lengthSecs: Int?, fontSize: Double, factor: Double) -> CGFloat {
        let base: CGFloat
        if let secs = lengthSecs, secs > 20 {
            base = max(4, (content - visible * 0.6) / CGFloat(secs))
        } else {
            base = fontSize * 0.6
        }
        return base * factor
    }

    static func step(_ factor: Double, up: Bool) -> Double {
        let i = factors.firstIndex { abs($0 - factor) < 0.01 } ?? 4
        return factors[up ? min(i + 1, factors.count - 1) : max(i - 1, 0)]
    }

    /// "3:45" → 225.
    static func seconds(_ text: String?) -> Int? {
        guard let parts = text?.split(separator: ":").compactMap({ Int($0) }), !parts.isEmpty else { return nil }
        return parts.reduce(0) { $0 * 60 + $1 }
    }
}

/// Start/stop and speed, for a toolbar or a menu.
struct AutoScrollControls: View {
    @Binding var running: Bool
    @AppStorage("gc.scroll.factor") private var factor = 1.0

    var body: some View {
        Button { running.toggle() } label: {
            Label(running ? "Stop scrolling" : "Auto-scroll",
                  systemImage: running ? "pause.circle.fill" : "arrow.down.circle")
        }
        .accessibilityHint("The chart scrolls by itself, as fast as the song is long. Touch the chart to pause.")
        Button { factor = AutoScrollSpeed.step(factor, up: false) } label: {
            Label("Scroll slower", systemImage: "tortoise")
        }
        Button { factor = AutoScrollSpeed.step(factor, up: true) } label: {
            Label("Scroll faster", systemImage: "hare")
        }
    }
}
