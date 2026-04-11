import AppKit
import Foundation

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    private enum CaptureState {
        case running
        case stopped
        case unavailable
    }

    private struct Command {
        let executable: String
        let prefixArguments: [String]
    }

    private struct CommandResult {
        let exitCode: Int32
        let stdout: String
        let stderr: String
    }

    private var statusItem: NSStatusItem!
    private var statusTimer: Timer?
    private var screencapCommand: Command?
    private var startItem = NSMenuItem(title: "Start Capture", action: #selector(startCapture), keyEquivalent: "")
    private var stopItem = NSMenuItem(title: "Stop Capture", action: #selector(stopCapture), keyEquivalent: "")

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        refreshScreencapCommand()
        configureStatusItem()
        refreshStatus()

        statusTimer = Timer.scheduledTimer(timeInterval: 5.0, target: self, selector: #selector(statusTimerFired), userInfo: nil, repeats: true)
    }

    func applicationWillTerminate(_ notification: Notification) {
        stopIfPossible()
        statusTimer?.invalidate()
    }

    @objc private func statusTimerFired() {
        refreshStatus()
    }

    private func configureStatusItem() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        if let button = statusItem.button {
            button.image = NSImage(systemSymbolName: "circle.fill", accessibilityDescription: "Screencap status")
            button.image?.isTemplate = false
            button.contentTintColor = .systemRed
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
        guard hasScreencapCommand() else {
            NSSound.beep()
            refreshStatus()
            return
        }

        let result = runScreencap(["start"])
        if result?.exitCode != 0 {
            NSSound.beep()
        }
        refreshStatus()
    }

    @objc private func stopCapture() {
        stopIfPossible()
        refreshStatus()
    }

    @objc private func openTimeline() {
        guard let url = URL(string: "http://localhost:7878") else { return }
        NSWorkspace.shared.open(url)
    }

    @objc private func openDataFolder() {
        let folderPath = NSString(string: "~/.screencap").expandingTildeInPath
        NSWorkspace.shared.selectFile(nil, inFileViewerRootedAtPath: folderPath)
    }

    @objc private func quitApp() {
        stopIfPossible()
        NSApp.terminate(nil)
    }

    private func refreshStatus() {
        let state: CaptureState
        if !hasScreencapCommand() {
            state = .unavailable
        } else {
            let statusResult = runScreencap(["status"])
            if statusResult?.exitCode == 0 {
                state = .running
            } else if statusResult?.exitCode == 127 {
                state = .unavailable
            } else {
                state = .stopped
            }
        }

        switch state {
        case .running:
            statusItem.button?.contentTintColor = .systemGreen
            statusItem.button?.toolTip = nil
            startItem.isEnabled = false
            stopItem.isEnabled = true
        case .stopped:
            statusItem.button?.contentTintColor = .systemRed
            statusItem.button?.toolTip = nil
            startItem.isEnabled = true
            stopItem.isEnabled = false
        case .unavailable:
            statusItem.button?.contentTintColor = .systemGray
            startItem.isEnabled = false
            stopItem.isEnabled = false
            statusItem.button?.toolTip = "screencap binary not found in PATH or ~/.cargo/bin"
        }
    }

    private func hasScreencapCommand() -> Bool {
        refreshScreencapCommand()
        return screencapCommand != nil
    }

    private func refreshScreencapCommand() {
        if let command = screencapCommand, FileManager.default.isExecutableFile(atPath: command.executable) {
            return
        }

        screencapCommand = resolveScreencapCommand()
    }


    private func stopIfPossible() {
        _ = runScreencap(["stop"])
    }

    private func runScreencap(_ arguments: [String]) -> CommandResult? {
        guard hasScreencapCommand(), let command = screencapCommand else { return nil }

        let result = runProcess(executable: command.executable, arguments: command.prefixArguments + arguments)
        if result.exitCode == 127 {
            screencapCommand = nil
        }
        return result
    }

    private func resolveScreencapCommand() -> Command? {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let knownPaths = [
            "\(home)/.cargo/bin/screencap",
            "/opt/homebrew/bin/screencap",
            "/usr/local/bin/screencap"
        ]

        for path in knownPaths where FileManager.default.isExecutableFile(atPath: path) {
            return Command(executable: path, prefixArguments: [])
        }

        let whichResult = runProcess(executable: "/usr/bin/which", arguments: ["screencap"])
        let resolvedPath = whichResult.stdout.trimmingCharacters(in: .whitespacesAndNewlines)
        if whichResult.exitCode == 0, !resolvedPath.isEmpty, FileManager.default.isExecutableFile(atPath: resolvedPath) {
            return Command(executable: resolvedPath, prefixArguments: [])
        }

        return nil
    }

    private func runProcess(executable: String, arguments: [String]) -> CommandResult {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: executable)
        process.arguments = arguments

        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe

        do {
            try process.run()
            process.waitUntilExit()
        } catch {
            return CommandResult(exitCode: 127, stdout: "", stderr: error.localizedDescription)
        }

        let stdoutData = stdoutPipe.fileHandleForReading.readDataToEndOfFile()
        let stderrData = stderrPipe.fileHandleForReading.readDataToEndOfFile()

        return CommandResult(
            exitCode: process.terminationStatus,
            stdout: String(decoding: stdoutData, as: UTF8.self),
            stderr: String(decoding: stderrData, as: UTF8.self)
        )
    }
}
