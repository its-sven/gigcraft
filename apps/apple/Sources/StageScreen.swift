import SwiftUI
import UIKit

/// What a connected external display (AirPlay, HDMI) shows: the current song
/// in very large type on black, following the show and the backing track.
struct StageScreenView: View {
    let model: LibraryModel
    @AppStorage("gc.stage.lyricsOnly") private var lyricsOnly = false

    var body: some View {
        GeometryReader { geo in
            let size = min(72, max(24, geo.size.height / 20))
            ZStack(alignment: .topLeading) {
                Color.black.ignoresSafeArea()
                content(size: size)
                    .padding(32)
            }
        }
        .preferredColorScheme(.dark)
    }

    @ViewBuilder
    private func content(size: Double) -> some View {
        let st = model.performState
        let cur = st?["current"] as? [String: Any]
        if let cur, cur["type"] as? String == "song", let songId = cur["song"] as? String {
            let gigKey = cur["key"] as? String
            let semis = model.semitones(songKey: model.song(songId)?.key, gigKey: gigKey)
            let chart = model.chart(songId, transpose: semis)
            let section = chart.flatMap { model.playingSection(songId: songId, chart: $0) }
            ScrollViewReader { proxy in
                ScrollView(showsIndicators: false) {
                    VStack(alignment: .leading, spacing: size * 0.4) {
                        HStack(spacing: 16) {
                            Text(cur["title"] as? String ?? "").font(.system(size: size * 1.4, weight: .bold))
                            if !lyricsOnly, let k = gigKey { Readout(label: "Key", value: k, size: size * 0.6) }
                        }
                        if let chart { ChartView(chart: chart, fontSize: size, highlight: section, lyricsOnly: lyricsOnly, followsTrack: true) }
                    }
                }
                .onChange(of: section) { _, new in
                    if let new { withAnimation(.easeInOut(duration: 0.4)) { proxy.scrollTo(new, anchor: .top) } }
                }
            }
        } else if let cur, cur["type"] as? String == "break" {
            let label = (cur["label"] as? String).flatMap { $0.isEmpty ? nil : $0 } ?? String(localized: "Break")
            Text(label).font(.system(size: size * 2, weight: .bold)).frame(maxWidth: .infinity, maxHeight: .infinity)
        } else {
            Text(verbatim: "GigCraft").font(.system(size: size * 1.5, weight: .semibold)).foregroundStyle(.secondary)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}

/// Puts the stage view on an external display when one connects.
final class ExternalDisplayDelegate: NSObject, UIWindowSceneDelegate {
    var window: UIWindow?

    func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options connectionOptions: UIScene.ConnectionOptions) {
        guard let windowScene = scene as? UIWindowScene, let model = LibraryModel.current else { return }
        let window = UIWindow(windowScene: windowScene)
        window.rootViewController = UIHostingController(rootView: StageScreenView(model: model))
        window.isHidden = false
        self.window = window
    }
}

final class AppDelegate: NSObject, UIApplicationDelegate {
    func application(_ application: UIApplication, configurationForConnecting session: UISceneSession,
                     options: UIScene.ConnectionOptions) -> UISceneConfiguration {
        if session.role == .windowExternalDisplayNonInteractive {
            let config = UISceneConfiguration(name: "Stage", sessionRole: session.role)
            config.delegateClass = ExternalDisplayDelegate.self
            return config
        }
        return UISceneConfiguration(name: nil, sessionRole: session.role)
    }
}
