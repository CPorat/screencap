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
                    return CommandResult(exitCode: 0, stdout: stoppedStatusOutput, stderr: "")
                }
                return self.statusResults.removeFirst()
            case ["stop"]:
                self.stopCallCount += 1
                self.managedProcess.isRunning = false
                return CommandResult(exitCode: 0, stdout: "", stderr: "")
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

private let runningStatusOutput = """
state: running
pid: 4242
uptime_secs: 12
captures_today: 7
storage_bytes: 2048
"""

private let stoppedStatusOutput = """
state: stopped
pid: -
uptime_secs: 0
captures_today: 0
storage_bytes: 0
"""

@Test func parseRunningStatusReport() {
    let report = DaemonStatusReport.parse(runningStatusOutput)

    #expect(report?.state == .running)
    #expect(report?.pid == 4242)
    #expect(report?.uptimeSecs == 12)
    #expect(report?.capturesToday == 7)
    #expect(report?.storageBytes == 2048)
    #expect(report?.tooltip == "Screencap running · pid 4242 · 7 captures today")
}

@Test @MainActor func refreshStatusUsesReportedStateNotExitCode() {
    let harness = TestHarness()
    harness.statusResults = [CommandResult(exitCode: 0, stdout: stoppedStatusOutput, stderr: "")]
    let controller = ScreencapDaemonController(environment: harness.environment)

    let snapshot = controller.refreshStatus()

    switch snapshot {
    case .stopped(let report):
        #expect(report.state == .stopped)
        #expect(snapshot.startEnabled)
        #expect(!snapshot.stopEnabled)
    default:
        Issue.record("expected stopped snapshot, got \(snapshot)")
    }
}

@Test @MainActor func startCaptureLaunchesManagedForegroundDaemon() {
    let harness = TestHarness()
    harness.statusResults = [
        CommandResult(exitCode: 0, stdout: stoppedStatusOutput, stderr: ""),
        CommandResult(exitCode: 0, stdout: stoppedStatusOutput, stderr: "")
    ]
    let controller = ScreencapDaemonController(environment: harness.environment)

    let snapshot = controller.startCaptureIfNeeded()

    #expect(snapshot == .starting)
    #expect(harness.launchArguments == [["__daemon-child"]])
    #expect(!snapshot.startEnabled)
    #expect(snapshot.stopEnabled)
}

@Test @MainActor func quitOnlyStopsOwnedDaemon() {
    let harness = TestHarness()
    let controller = ScreencapDaemonController(environment: harness.environment)

    controller.stopOwnedDaemonIfNeeded()
    #expect(harness.stopCallCount == 0)

    harness.statusResults = [
        CommandResult(exitCode: 0, stdout: stoppedStatusOutput, stderr: ""),
        CommandResult(exitCode: 0, stdout: runningStatusOutput, stderr: "")
    ]
    _ = controller.startCaptureIfNeeded()
    controller.stopOwnedDaemonIfNeeded()

    #expect(harness.stopCallCount == 1)
}
