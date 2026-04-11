import Foundation

struct DaemonStatusReport: Equatable {
    enum State: String, Equatable {
        case running
        case stopped
    }

    let state: State
    let pid: Int32?
    let uptimeSecs: UInt64?
    let capturesToday: UInt64?
    let storageBytes: UInt64?
    let launchdInstalled: Bool
    let rollingSummary: String?

    static func parse(_ text: String) -> Self? {
        var fields: [String: String] = [:]

        for rawLine in text.split(whereSeparator: \.isNewline) {
            let line = rawLine.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !line.isEmpty, let separator = line.firstIndex(of: ":") else {
                continue
            }

            let key = String(line[..<separator]).trimmingCharacters(in: .whitespaces)
            let value = String(line[line.index(after: separator)...]).trimmingCharacters(in: .whitespaces)
            fields[key] = value
        }

        guard let rawState = fields["state"], let state = State(rawValue: rawState) else {
            return nil
        }

        return Self(
            state: state,
            pid: parseInt32(fields["pid"]),
            uptimeSecs: parseUInt64(fields["uptime_secs"]),
            capturesToday: parseUInt64(fields["captures_today"]),
            storageBytes: parseUInt64(fields["storage_bytes"]),
            launchdInstalled: parseBool(fields["launchd_installed"]) ?? false,
            rollingSummary: parseText(fields["rolling_summary"])
        )
    }

    private static func parseInt32(_ value: String?) -> Int32? {
        guard let value, value != "-" else { return nil }
        return Int32(value)
    }

    private static func parseUInt64(_ value: String?) -> UInt64? {
        guard let value, value != "-" else { return nil }
        return UInt64(value)
    }

    private static func parseBool(_ value: String?) -> Bool? {
        guard let value else { return nil }
        switch value.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
        case "true":
            return true
        case "false":
            return false
        default:
            return nil
        }
    }

    private static func parseText(_ value: String?) -> String? {
        guard let value else { return nil }
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, trimmed != "-" else { return nil }
        return trimmed
    }

    var tooltip: String {
        switch state {
        case .running:
            if let rollingSummary {
                return rollingSummary
            }
            let pidText = pid.map(String.init) ?? "-"
            let capturesText = capturesToday.map(String.init) ?? "0"
            return "Screencap running · pid \(pidText) · \(capturesText) captures today"
        case .stopped:
            return "Screencap stopped"
        }
    }
}

enum DaemonStatusSnapshot: Equatable {
    case running(DaemonStatusReport)
    case starting
    case stopped(DaemonStatusReport)
    case unavailable(String)

    var startEnabled: Bool {
        switch self {
        case .stopped:
            return true
        case .running, .starting, .unavailable:
            return false
        }
    }

    var stopEnabled: Bool {
        switch self {
        case .running, .starting:
            return true
        case .stopped, .unavailable:
            return false
        }
    }

    var tooltip: String {
        switch self {
        case .running(let report), .stopped(let report):
            return report.tooltip
        case .starting:
            return "Screencap is starting"
        case .unavailable(let message):
            return message
        }
    }
}

protocol ManagedDaemonProcess: AnyObject {
    var isRunning: Bool { get }
    func terminate()
}

final class FoundationManagedDaemonProcess: ManagedDaemonProcess {
    private let process: Process

    init(process: Process, onExit: @escaping @Sendable () -> Void) {
        self.process = process
        process.terminationHandler = { _ in onExit() }
    }

    var isRunning: Bool {
        process.isRunning
    }

    func terminate() {
        process.terminate()
    }
}

struct DaemonControllerEnvironment {
    var homeDirectoryPath: () -> String
    var isExecutable: (String) -> Bool
    var runCommand: (String, [String]) -> CommandResult
    var launchManagedProcess: (String, [String], @escaping @Sendable () -> Void) throws -> any ManagedDaemonProcess

    @MainActor static let live = Self(
        homeDirectoryPath: { FileManager.default.homeDirectoryForCurrentUser.path },
        isExecutable: { FileManager.default.isExecutableFile(atPath: $0) },
        runCommand: runProcess,
        launchManagedProcess: { executable, arguments, onExit in
            let process = Process()
            process.executableURL = URL(fileURLWithPath: executable)
            process.arguments = arguments
            process.standardInput = FileHandle.nullDevice
            process.standardOutput = FileHandle.nullDevice
            process.standardError = FileHandle.nullDevice
            try process.run()
            return FoundationManagedDaemonProcess(process: process, onExit: onExit)
        }
    )
}

struct CommandResult: Equatable {
    let exitCode: Int32
    let stdout: String
    let stderr: String
}

@MainActor
final class ScreencapDaemonController {
    private struct Command {
        let executable: String
        let prefixArguments: [String]
    }

    private static let internalDaemonSubcommand = "__daemon-child"
    private static let missingCommandMessage = "screencap binary not found in PATH or ~/.cargo/bin"
    private static let unreadableStatusMessage = "failed to read screencap daemon status"

    private let environment: DaemonControllerEnvironment
    private var screencapCommand: Command?
    private var managedProcess: (any ManagedDaemonProcess)?
    private var ownsManagedProcess = false

    init(environment: DaemonControllerEnvironment = .live) {
        self.environment = environment
        refreshScreencapCommand()
    }

    func startCaptureIfNeeded() -> DaemonStatusSnapshot {
        switch refreshStatus() {
        case .running, .starting:
            return refreshStatus()
        case .unavailable(let message):
            return .unavailable(message)
        case .stopped:
            do {
                try launchManagedDaemon()
            } catch {
                clearManagedProcess()
                return .unavailable("failed to launch screencap daemon: \(error.localizedDescription)")
            }

            return refreshStatus()
        }
    }

    func stopCapture() -> DaemonStatusSnapshot {
        switch refreshStatus() {
        case .unavailable(let message):
            return .unavailable(message)
        case .stopped:
            return refreshStatus()
        case .starting:
            terminateManagedProcess()
        case .running:
            if ownsManagedProcess {
                _ = runScreencap(["stop"])
                terminateManagedProcessIfStillRunning()
            } else {
                _ = runScreencap(["stop"])
            }
        }

        return refreshStatus()
    }

    func stopOwnedDaemonIfNeeded() {
        guard ownsManagedProcess else { return }
        _ = runScreencap(["stop"])
        terminateManagedProcessIfStillRunning()
        clearManagedProcess()
    }

    func openPreferences() -> Bool {
        guard let result = runScreencap(["config"]) else { return false }
        return result.exitCode == 0
    }

    func toggleLaunchAtLogin() -> DaemonStatusSnapshot {
        switch refreshStatus() {
        case .unavailable(let message):
            return .unavailable(message)
        case .starting:
            return .starting
        case .running(let report):
            if report.launchdInstalled {
                return .running(report)
            }
            return installLaunchAtLogin(stopAfterInstall: false)
        case .stopped(let report):
            if report.launchdInstalled {
                return uninstallLaunchAtLogin()
            }
            return installLaunchAtLogin(stopAfterInstall: true)
        }
    }

    func refreshStatus() -> DaemonStatusSnapshot {
        clearExitedManagedProcess()

        guard hasScreencapCommand() else {
            return .unavailable(Self.missingCommandMessage)
        }

        guard let result = runScreencap(["status"]), result.exitCode == 0 else {
            return ownsManagedProcess && managedProcess?.isRunning == true
                ? .starting
                : .unavailable(Self.unreadableStatusMessage)
        }

        guard let report = DaemonStatusReport.parse(result.stdout) else {
            return ownsManagedProcess && managedProcess?.isRunning == true
                ? .starting
                : .unavailable(Self.unreadableStatusMessage)
        }

        if report.state == .stopped, ownsManagedProcess, managedProcess?.isRunning == true {
            return .starting
        }

        switch report.state {
        case .running:
            return .running(report)
        case .stopped:
            return .stopped(report)
        }
    }

    private func hasScreencapCommand() -> Bool {
        refreshScreencapCommand()
        return screencapCommand != nil
    }

    private func refreshScreencapCommand() {
        if let command = screencapCommand, environment.isExecutable(command.executable) {
            return
        }

        screencapCommand = resolveScreencapCommand()
    }

    private func launchManagedDaemon() throws {
        guard let command = screencapCommand ?? resolveScreencapCommand() else {
            throw NSError(domain: "ScreencapMenu", code: 127, userInfo: [NSLocalizedDescriptionKey: Self.missingCommandMessage])
        }

        let process = try environment.launchManagedProcess(
            command.executable,
            command.prefixArguments + [Self.internalDaemonSubcommand],
            { [weak self] in
                Task { @MainActor in
                    self?.clearManagedProcess()
                }
            }
        )

        managedProcess = process
        ownsManagedProcess = true
        screencapCommand = command
    }

    private func terminateManagedProcessIfStillRunning() {
        if managedProcess?.isRunning == true {
            managedProcess?.terminate()
        }
    }

    private func terminateManagedProcess() {
        terminateManagedProcessIfStillRunning()
        clearManagedProcess()
    }

    private func clearExitedManagedProcess() {
        guard ownsManagedProcess else { return }
        if managedProcess?.isRunning != true {
            clearManagedProcess()
        }
    }

    private func clearManagedProcess() {
        managedProcess = nil
        ownsManagedProcess = false
    }

    private func runScreencap(_ arguments: [String]) -> CommandResult? {
        guard hasScreencapCommand(), let command = screencapCommand else { return nil }

        let result = environment.runCommand(command.executable, command.prefixArguments + arguments)
        if result.exitCode == 127 {
            screencapCommand = nil
        }
        return result
    }

    private func installLaunchAtLogin(stopAfterInstall: Bool) -> DaemonStatusSnapshot {
        guard let installResult = runScreencap(["start", "--install"]) else {
            return .unavailable(Self.missingCommandMessage)
        }
        guard installResult.exitCode == 0 else {
            return .unavailable(commandFailureMessage(action: "install launch at login", result: installResult))
        }

        if stopAfterInstall {
            guard let stopResult = runScreencap(["stop"]) else {
                return .unavailable(Self.missingCommandMessage)
            }
            guard stopResult.exitCode == 0 else {
                return .unavailable(commandFailureMessage(action: "restore stopped state", result: stopResult))
            }
        }

        return refreshStatus()
    }

    private func uninstallLaunchAtLogin() -> DaemonStatusSnapshot {
        guard let result = runScreencap(["stop", "--uninstall"]) else {
            return .unavailable(Self.missingCommandMessage)
        }
        guard result.exitCode == 0 else {
            return .unavailable(commandFailureMessage(action: "uninstall launch at login", result: result))
        }

        return refreshStatus()
    }

    private func commandFailureMessage(action: String, result: CommandResult) -> String {
        let detail = [result.stderr, result.stdout]
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first(where: { !$0.isEmpty })

        if let detail {
            return "failed to \(action): \(detail)"
        }

        return "failed to \(action)"
    }

    private func resolveScreencapCommand() -> Command? {
        let home = environment.homeDirectoryPath()
        let knownPaths = [
            "\(home)/.cargo/bin/screencap",
            "/opt/homebrew/bin/screencap",
            "/usr/local/bin/screencap"
        ]

        for path in knownPaths where environment.isExecutable(path) {
            return Command(executable: path, prefixArguments: [])
        }

        let whichResult = environment.runCommand("/usr/bin/which", ["screencap"])
        let resolvedPath = whichResult.stdout.trimmingCharacters(in: .whitespacesAndNewlines)
        if whichResult.exitCode == 0, !resolvedPath.isEmpty, environment.isExecutable(resolvedPath) {
            return Command(executable: resolvedPath, prefixArguments: [])
        }

        return nil
    }
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
