---
name: web-worker
description: Implements the embedded Svelte SPA and browser-facing validation for Screencap.
---

# Web Worker

NOTE: Startup and cleanup are handled by `worker-base`. This skill defines the WORK PROCEDURE.

## When to Use This Skill

Use this skill for the Svelte application and the browser-served UI: static asset bootstrapping, SPA routing, Timeline, Insights, Search, Stats, API integration, and embedded-asset build wiring that affects browser behavior.

## Required Skills

- `agent-browser` — required after implementation for browser verification of every changed flow. Use Safari against `http://localhost:7878` because this mission validated Safari as the supported browser path.

## Work Procedure

1. Read `mission.md`, mission `AGENTS.md`, `.factory/library/architecture.md`, and `.factory/library/user-testing.md` before editing.
2. Add failing frontend/component/browser-facing tests first for each changed UI behavior. Prefer focused component tests plus API-contract mocks/fixtures that match the mission contract.
3. Implement the UI change in Svelte and any minimal API-integration glue needed for the claimed assertions.
4. Run targeted frontend tests and build checks before manual validation.
5. Invoke `agent-browser` and manually verify every changed browser flow in Safari, including empty/degraded states when the contract requires them.
6. Confirm the embedded asset pipeline still serves the correct static output from the daemon entrypoint.
7. Run the manifest validation commands relevant to the web surface (`test`, `typecheck`, `lint`, and `build` when the feature touches bundling or routing).
8. Record exact browser flows and observations in the handoff; do not write "looks good" without step-by-step evidence.

## Example Handoff

```json
{
  "salientSummary": "Built the Svelte Timeline and Insights views with real API wiring and verified them in Safari. Added component tests for empty-state, filter, and detail-panel behavior, then confirmed static asset serving still worked from the daemon.",
  "whatWasImplemented": "Implemented the embedded SPA shell, day navigation, timeline grouping, screenshot/detail expansion, and insights cards backed by the local API contract. The feature also handles first-run empty states and preserves navigation between Timeline and Insights without full-page reloads.",
  "whatWasLeftUndone": "",
  "verification": {
    "commandsRun": [
      {
        "command": "npm --prefix web run test -- --runInBand timeline insights",
        "exitCode": 0,
        "observation": "Timeline grouping, empty-state, and insights rendering cases all passed."
      },
      {
        "command": "npm --prefix web run build",
        "exitCode": 0,
        "observation": "Static assets compiled successfully for embedding into the Rust binary."
      }
    ],
    "interactiveChecks": [
      {
        "action": "Used agent-browser in Safari to open http://localhost:7878, navigate Timeline -> Insights, apply a date change, and open a timeline detail panel.",
        "observed": "The SPA shell loaded without a full refresh, the date change reloaded both views, and the detail panel showed screenshot + extraction metadata from the seeded API payload."
      },
      {
        "action": "Loaded the app against an empty database fixture.",
        "observed": "Timeline and Insights rendered explicit empty states instead of blank panels or console errors."
      }
    ]
  },
  "tests": {
    "added": [
      {
        "file": "web/src/lib/timeline.spec.ts",
        "cases": [
          {
            "name": "renders_hour_groups_and_detail_panel",
            "verifies": "Timeline groups captures by hour and exposes detail content for a selected segment."
          },
          {
            "name": "shows_first_run_empty_state",
            "verifies": "A clean database renders a deterministic empty-state card instead of a broken layout."
          }
        ]
      }
    ]
  },
  "discoveredIssues": []
}
```

## When to Return to Orchestrator

- The required API contract or payload shape is missing or contradicts the validation contract.
- Embedded static assets cannot be served from the daemon within the mission boundaries.
- Browser verification requires a new tool or non-Safari path not approved in the mission.
