import AppKit
import Foundation

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private var statusItem: NSStatusItem!
    private var statusTimer: Timer?
    private let daemonController = ScreencapDaemonController()
    private var startItem = NSMenuItem(title: "Start Capture", action: #selector(startCapture), keyEquivalent: "")
    private var stopItem = NSMenuItem(title: "Stop Capture", action: #selector(stopCapture), keyEquivalent: "")

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        configureStatusItem()
        applyStatus(daemonController.startCaptureIfNeeded())

        statusTimer = Timer.scheduledTimer(
            timeInterval: 5.0,
            target: self,
            selector: #selector(statusTimerFired),
            userInfo: nil,
            repeats: true
        )
    }

    func applicationWillTerminate(_ notification: Notification) {
        daemonController.stopOwnedDaemonIfNeeded()
        statusTimer?.invalidate()
    }

    @objc private func statusTimerFired() {
        applyStatus(daemonController.refreshStatus())
    }

    private func configureStatusItem() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        if let button = statusItem.button {
            button.image = NSImage(systemSymbolName: "circle.fill", accessibilityDescription: "Screencap status")
            button.image?.isTemplate = false
        }

        let menu = NSMenu()

        startItem.target = self
        stopItem.target = self

        let openTimeline = NSMenuItem(title: "Open Timeline", action: #selector(openTimeline), keyEquivalent: "")
        openTimeline.target = self

        let openDataFolder = NSMenuItem(title: "Open Data Folder", action: #selector(openDataFolder), keyEquivalent: "")
        openDataFolder.target = self

        let quitItem = NSMenuItem(title: "Quit", action: #selector(quitApp), keyEquivalent: "q")
        quitItem.target = self

        menu.addItem(startItem)
        menu.addItem(stopItem)
        menu.addItem(.separator())
        menu.addItem(openTimeline)
        menu.addItem(openDataFolder)
        menu.addItem(.separator())
        menu.addItem(quitItem)

        statusItem.menu = menu
    }

    @objc private func startCapture() {
        let snapshot = daemonController.startCaptureIfNeeded()
        if case .unavailable = snapshot {
            NSSound.beep()
        }
        applyStatus(snapshot)
    }

    @objc private func stopCapture() {
        applyStatus(daemonController.stopCapture())
    }

    @objc private func openTimeline() {
        guard let url = URL(string: "http://localhost:7878") else { return }
        NSWorkspace.shared.open(url)
    }

    @objc private func openDataFolder() {
        let folderURL = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent(".screencap", isDirectory: true)
        try? FileManager.default.createDirectory(at: folderURL, withIntermediateDirectories: true)
        NSWorkspace.shared.open(folderURL)
    }

    @objc private func quitApp() {
        daemonController.stopOwnedDaemonIfNeeded()
        NSApp.terminate(nil)
    }

    private func applyStatus(_ snapshot: DaemonStatusSnapshot) {
        switch snapshot {
        case .running:
            statusItem.button?.contentTintColor = .systemGreen
        case .starting:
            statusItem.button?.contentTintColor = .systemYellow
        case .stopped:
            statusItem.button?.contentTintColor = .systemRed
        case .unavailable:
            statusItem.button?.contentTintColor = .systemGray
        }

        startItem.isEnabled = snapshot.startEnabled
        stopItem.isEnabled = snapshot.stopEnabled
        statusItem.button?.toolTip = snapshot.tooltip
    }
}
