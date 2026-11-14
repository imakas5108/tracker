import SwiftUI
import AppKit

@main
struct TrackerApp: App {
    @StateObject private var workspace = Workspace()

    var body: some Scene {
        WindowGroup("Tracker", id: "workspace") {
            WorkspaceView(workspace: workspace)
                .frame(minWidth: 980, minHeight: 640)
                .onAppear {
                    NSApp.setActivationPolicy(.regular)
                    NSApp.activate(ignoringOtherApps: true)
                    let paths = CommandLine.arguments.dropFirst().filter { !$0.hasPrefix("-") }
                    if workspace.sessions.isEmpty { workspace.importVideos(paths.map { URL(fileURLWithPath: $0) }) }
                }
                .onOpenURL { workspace.importVideos([$0]) }
        }
        .defaultSize(width: 1280, height: 800)
        .windowToolbarStyle(.unifiedCompact)
        .commands {
            CommandGroup(replacing: .newItem) {
                Button("Open Video…", action: workspace.openVideos).keyboardShortcut("o")
            }
        }
    }
}
