---
name: native-macos-worker
description: Implements macOS-native Swift bridge, capture integration, menu bar behavior, and launchd-facing features.
---

# Native macOS Worker

NOTE: Startup and cleanup are handled by `worker-base`. This skill defines the WORK PROCEDURE.

## When to Use This Skill

Use this skill for ScreenCaptureKit, NSWorkspace, CGEventTap, Swift-to-Rust FFI, Screen Recording permission handling, the Swift menu bar app, and launchd/menu-driven lifecycle integration.

## Required Skills

None.

## Work Procedure

1. Read `mission.md`, mission `AGENTS.md`, `.factory/library/architecture.md`, `.factory/library/environment.md`, and `.factory/library/user-testing.md` before editing.
2. Keep Swift code thin: native API calls and lifecycle glue only. Put durable business logic, persistence, and policy in Rust unless the platform API forces Swift ownership.
3. Add failing tests first where feasible:
   - Rust tests for policy, scheduling, dedup, and persistence rules.
   - Swift/Xcode tests for isolated native helpers when practical.
   - For permission- or menu-driven flows that cannot be fully automated, add the strongest deterministic harnesses available and then do manual verification.
4. Implement the smallest native bridge/menu change that satisfies the contract, preserving the offline capture boundary and pre-persistence privacy filtering.
5. Verify native builds (`xcodebuild` / Rust build integration) and any Rust integration tests covering the bridge boundary.
6. For capture/menu features, manually validate the concrete flow the contract requires, including permission-degraded behavior and daemon-state reflection. Do not claim success without an observed native outcome.
7. If the task would require TCC database tampering, privileged escalation, or changing system privacy settings outside normal user prompts, return to the orchestrator instead.

## Example Handoff

```json
{
  "salientSummary": "Implemented the timer-first ScreenCaptureKit bridge integration and menu-bar daemon-state indicator. Added Rust tests for dedup/offline guarantees and verified the Swift bridge compiled and returned real screenshot + window metadata in a logged-in macOS session.",
  "whatWasImplemented": "Wired the Swift screenshot and active-window bridge into the Rust capture pipeline, including JPEG persistence, metadata propagation, and permission-aware error mapping. Also updated the menu bar status item to reflect running/processing/stopped daemon states using the daemon as the source of truth.",
  "whatWasLeftUndone": "",
  "verification": {
    "commandsRun": [
      {
        "command": "cargo test capture::tests::timer_fallback_and_dedup -- --test-threads=1",
        "exitCode": 0,
        "observation": "Timer-floor, settle-delay, and dedup policy checks passed with mocked time and bridge fixtures."
      },
      {
        "command": "xcodebuild -project menubar/ScreencapMenu.xcodeproj -scheme ScreencapMenu -configuration Debug build",
        "exitCode": 0,
        "observation": "Swift menu bar target compiled successfully with the updated bridge bindings."
      }
    ],
    "interactiveChecks": [
      {
        "action": "Started the app in a logged-in macOS session with Screen Recording already granted and waited for the timer fallback capture to fire.",
        "observed": "A JPEG appeared under ~/.screencap/screenshots/... and the corresponding capture row contained app/window metadata with extraction_status=pending."
      },
      {
        "action": "Revoked/withheld Screen Recording permission and started capture again.",
        "observed": "No bogus screenshot rows were created and the native/UI-visible status indicated capture was blocked by permission."
      }
    ]
  },
  "tests": {
    "added": [
      {
        "file": "src/capture/tests.rs",
        "cases": [
          {
            "name": "timer_fallback_still_captures_when_no_events_arrive",
            "verifies": "The timer-first MVP captures on the configured floor even without CGEventTap activity."
          },
          {
            "name": "excluded_window_title_blocks_disk_write",
            "verifies": "Sensitive windows are filtered before JPEG persistence or DB insertion."
          }
        ]
      }
    ]
  },
  "discoveredIssues": []
}
```

## When to Return to Orchestrator

- A required native flow needs user approval or system settings outside normal in-session prompts.
- The feature requires a new mission boundary (new port, privileged helper, non-local service, or forbidden resource).
- The Rust/core side lacks a prerequisite API or data model the native integration depends on.
