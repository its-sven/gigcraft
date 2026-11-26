import SwiftUI

@main
struct GigCraftApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @State private var model = LibraryModel()

    var body: some Scene {
        WindowGroup {
            Group {
                // `-gcStagePreview YES`: show the external-display view in the main window (screenshots).
                if UserDefaults.standard.bool(forKey: "gcStagePreview") {
                    StageScreenView(model: model)
                } else {
                    RootView()
                }
            }
                .environment(model)
                .task {
                    model.load()
                    while !Task.isCancelled {
                        try? await Task.sleep(for: .seconds(3))
                        model.watchDisk()
                    }
                }
        }
    }
}
