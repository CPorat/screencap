import Testing
@testable import ScreencapMenu

private final class TestManagedProcess: ManagedDaemonProcess {
    var isRunning = true
    private(set) var terminateCallCount = 0

    func terminate() {
        terminateCallCount += 1
        isRunning = false
    }
}

private final class TestHarness {
    let screencapPath = "/Users/test/.cargo/bin/screencap"
    let managedProcess = TestManagedProcess()
    var statusResults: [CommandResult] = []
    private(set) var launchArguments: [[String]] = []
    private(set) var stopCallCount = 0
    private(set) var configCallCount = 0
    private(set) var installCallCount = 0
    private(set) var uninstallCallCount = 0

    lazy var environment = DaemonControllerEnvironment(
        homeDirectoryPath: { "/Users/test" },
        isExecutable: { [screencapPath] path in path == screencapPath },
        runCommand: { executable, arguments in
            guard executable == self.screencapPath else {
                return CommandResult(exitCode: 127, stdout: "", stderr: "unexpected executable")
            }

            switch arguments {
            case ["status"]:
                if self.statusResults.isEmpty {
                    return CommandResult(exitCode: 0, stdout: stoppedStatusOutput(), stderr: "")
                }
                return self.statusResults.removeFirst()
            case ["stop"]:
                self.stopCallCount += 1
                self.managedProcess.isRunning = false
                return CommandResult(exitCode: 0, stdout: "", stderr: "")
            case ["config"]:
                self.configCallCount += 1
                return CommandResult(exitCode: 0, stdout: "", stderr: "")
            case ["start", "--install"]:
                self.installCallCount += 1
                return CommandResult(exitCode: 0, stdout: "installed launchd agent", stderr: "")
            case ["stop", "--uninstall"]:
                self.uninstallCallCount += 1
                self.managedProcess.isRunning = false
                return CommandResult(exitCode: 0, stdout: "removed launchd agent", stderr: "")
            default:
                return CommandResult(exitCode: 127, stdout: "", stderr: "unexpected arguments")
            }
        },
        launchManagedProcess: { _, arguments, _ in
            self.launchArguments.append(arguments)
            self.managedProcess.isRunning = true
            return self.managedProcess
        }
    )
}

private func runningStatusOutput(
    launchdInstalled: Bool = true,
    rollingSummary: String? = "Focused menu bar implementation."
) -> String {
    let summary = rollingSummary ?? "-"
    return """
state: running
pid: 4242
uptime_secs: 12
captures_today: 7
storage_bytes: 2048
launchd_installed: \(launchdInstalled)
rolling_summary: \(summary)
"""
}

private func stoppedStatusOutput(launchdInstalled: Bool = false) -> String {
    """
state: stopped
pid: -
uptime_secs: 0
captures_today: 0
storage_bytes: 0
launchd_installed: \(launchdInstalled)
rolling_summary: -
"""
}

@Test func parseRunningStatusReportUsesRollingSummaryTooltip() {
    let report = DaemonStatusReport.parse(runningStatusOutput())

    #expect(report?.state == .running)
    #expect(report?.pid == 4242)
    #expect(report?.uptimeSecs == 12)
    #expect(report?.capturesToday == 7)
    #expect(report?.storageBytes == 2048)
    #expect(report?.launchdInstalled == true)
    #expect(report?.rollingSummary == "Focused menu bar implementation.")
    #expect(report?.tooltip == "Focused menu bar implementation.")
}

@Test @MainActor func refreshStatusUsesReportedStateNotExitCode() {
    let harness = TestHarness()
    harness.statusResults = [CommandResult(exitCode: 0, stdout: stoppedStatusOutput(), stderr: "")]
    let controller = ScreencapDaemonController(environment: harness.environment)

    let snapshot = controller.refreshStatus()

    switch snapshot {
    case .stopped(let report):
        #expect(report.state == .stopped)
        #expect(snapshot.startEnabled)
        #expect(!snapshot.stopEnabled)
        #expect(report.launchdInstalled == false)
    default:
        Issue.record("expected stopped snapshot, got \(snapshot)")
    }
}

@Test @MainActor func startCaptureLaunchesManagedForegroundDaemon() {
    let harness = TestHarness()
    harness.statusResults = [
        CommandResult(exitCode: 0, stdout: stoppedStatusOutput(), stderr: ""),
        CommandResult(exitCode: 0, stdout: stoppedStatusOutput(), stderr: "")
    ]
    let controller = ScreencapDaemonController(environment: harness.environment)

    let snapshot = controller.startCaptureIfNeeded()

    #expect(snapshot == .starting)
    #expect(harness.launchArguments == [["__daemon-child"]])
    #expect(!snapshot.startEnabled)
    #expect(snapshot.stopEnabled)
}

@Test @MainActor func openPreferencesRunsConfigCommand() {
    let harness = TestHarness()
    let controller = ScreencapDaemonController(environment: harness.environment)

    #expect(controller.openPreferences())
    #expect(harness.configCallCount == 1)
}

@Test @MainActor func toggleLaunchAtLoginInstallsWhileStoppedAndRestoresState() {
    let harness = TestHarness()
    harness.statusResults = [
        CommandResult(exitCode: 0, stdout: stoppedStatusOutput(launchdInstalled: false), stderr: ""),
        CommandResult(exitCode: 0, stdout: stoppedStatusOutput(launchdInstalled: true), stderr: "")
    ]
    let controller = ScreencapDaemonController(environment: harness.environment)

    let snapshot = controller.toggleLaunchAtLogin()

    switch snapshot {
    case .stopped(let report):
        #expect(report.launchdInstalled)
    default:
        Issue.record("expected stopped snapshot after install, got \(snapshot)")
    }
    #expect(harness.installCallCount == 1)
    #expect(harness.stopCallCount == 1)
}

@Test @MainActor func toggleLaunchAtLoginUninstallsWhenStopped() {
    let harness = TestHarness()
    harness.statusResults = [
        CommandResult(exitCode: 0, stdout: stoppedStatusOutput(launchdInstalled: true), stderr: ""),
        CommandResult(exitCode: 0, stdout: stoppedStatusOutput(launchdInstalled: false), stderr: "")
    ]
    let controller = ScreencapDaemonController(environment: harness.environment)

    let snapshot = controller.toggleLaunchAtLogin()

    switch snapshot {
    case .stopped(let report):
        #expect(!report.launchdInstalled)
    default:
        Issue.record("expected stopped snapshot after uninstall, got \(snapshot)")
    }
    #expect(harness.uninstallCallCount == 1)
}

@Test @MainActor func quitOnlyStopsOwnedDaemon() {
    let harness = TestHarness()
    let controller = ScreencapDaemonController(environment: harness.environment)

    controller.stopOwnedDaemonIfNeeded()
    #expect(harness.stopCallCount == 0)

    harness.statusResults = [
        CommandResult(exitCode: 0, stdout: stoppedStatusOutput(), stderr: ""),
        CommandResult(exitCode: 0, stdout: runningStatusOutput(), stderr: "")
    ]
    _ = controller.startCaptureIfNeeded()
    controller.stopOwnedDaemonIfNeeded()

    #expect(harness.stopCallCount == 1)
}
