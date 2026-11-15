import SwiftUI
import AppKit

@main
struct TrackerApp: App {
    @StateObject private var workspace = Workspace()
    @NSApplicationDelegateAdaptor(TrackerLifecycle.self) private var lifecycle

    var body: some Scene {
        WindowGroup("Tracker", id: "workspace") {
            WorkspaceView(workspace: workspace)
                .frame(minWidth: 980, minHeight: 640)
                .onAppear {
                    lifecycle.workspace = workspace
                    workspace.automation.start(enabled: CommandLine.arguments.contains("--agent-control"))
                    DispatchQueue.main.async { lifecycle.attachWindow() }
                    NSApp.setActivationPolicy(.regular)
                    if !CommandLine.arguments.contains("--benchmark-background") { NSApp.activate(ignoringOtherApps: true) }
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
                Button("Open Project…", action: workspace.chooseProject).keyboardShortcut("o", modifiers: [.command, .shift])
                Button("Save Project") { Task { _ = await workspace.selected?.saveProject() } }.keyboardShortcut("s")
                    .disabled(workspace.selected == nil)
            }
            CommandGroup(replacing: .undoRedo) {
                Button("Undo") { workspace.selected?.undo() }.keyboardShortcut("z")
                Button("Redo") { workspace.selected?.redo() }.keyboardShortcut("z", modifiers: [.command, .shift])
            }
        }
    }
}

@MainActor
final class TrackerLifecycle: NSObject, NSApplicationDelegate, NSWindowDelegate {
    weak var workspace: Workspace?
    private weak var previousWindowDelegate: NSWindowDelegate?
    func attachWindow() {
        guard let window = NSApp.windows.first(where: { $0.title == "Tracker" }), window.delegate !== self else { return }
        previousWindowDelegate = window.delegate
        window.delegate = self
    }
    override func responds(to selector: Selector!) -> Bool {
        super.responds(to: selector) || (previousWindowDelegate?.responds(to: selector) ?? false)
    }
    override func forwardingTarget(for selector: Selector!) -> Any? {
        if previousWindowDelegate?.responds(to: selector) == true { return previousWindowDelegate }
        return super.forwardingTarget(for: selector)
    }
    func windowShouldClose(_ sender: NSWindow) -> Bool {
        guard let workspace else { return true }
        Task { if await workspace.confirmQuit() { sender.close() } }
        return false
    }
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard let workspace else { return .terminateNow }
        Task { let allowed = await workspace.confirmQuit(); if allowed { workspace.automation.shutdown() }; sender.reply(toApplicationShouldTerminate: allowed) }
        return .terminateLater
    }
}
