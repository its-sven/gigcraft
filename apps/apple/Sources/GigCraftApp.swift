import SwiftUI

@main
struct GigCraftApp: App {
    @State private var model = LibraryModel()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(model)
                .task { model.load() }
        }
    }
}
