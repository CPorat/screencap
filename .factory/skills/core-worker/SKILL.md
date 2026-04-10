---
name: core-worker
description: Implements Rust core features: storage, daemon, API, pipelines, search, export, and MCP surfaces.
---

# Core Worker

NOTE: Startup and cleanup are handled by `worker-base`. This skill defines the WORK PROCEDURE.

## When to Use This Skill

Use this skill for Rust-first features in the Screencap daemon: repository scaffolding, config loading, SQLite/FTS storage, schedulers, AI provider plumbing, CLI commands, REST API endpoints, search, analytics, synthesis, export, and MCP server work.

## Required Skills

None.

## Work Procedure

1. Read `mission.md`, mission `AGENTS.md`, `.factory/library/architecture.md`, `.factory/library/environment.md`, `.factory/library/providers.md`, and `.factory/library/user-testing.md` before editing.
2. Write or extend failing Rust tests first for the behavior this feature claims in `fulfills`. Prefer unit tests for pure logic and integration tests for SQLite, CLI, API, MCP, and scheduler behavior.
3. Implement the minimum production changes needed to make the new tests pass. Keep pipeline state typed; do not store structured fields as untyped blobs except where the contract explicitly allows raw responses for debugging.
4. If the feature touches HTTP, CLI, export, or MCP surfaces, manually verify those surfaces after tests pass:
   - CLI: run the exact command(s) added by the feature.
   - API: use `curl` against `http://localhost:7878`.
   - MCP: run a deterministic stdio/tool contract check.
5. Run the relevant fast checks first, then the manifest commands needed for the touched surface (`test`, `typecheck`, `lint`, and `build` only when required by the feature).
6. Update shared knowledge when you discover durable facts:
   - `.factory/services.yaml` for broken commands/service control facts
   - `.factory/library/*.md` for environment, provider, architecture, or testing facts
7. Do not fake real integrations. If a feature needs `OPENROUTER_API_KEY`, real screenshots, or another external dependency that is missing, return to the orchestrator instead of silently mocking.
8. End with a precise handoff that lists tests added, commands run, manual checks, and any discovered issues.

## Example Handoff

```json
{
  "salientSummary": "Implemented transactional extraction batching for pending captures and wired OpenRouter-compatible requests through the shared provider boundary. Added persistence tests for batch rows, per-frame rows, and retry-safe status transitions; verified the API health/read surfaces still behaved normally.",
  "whatWasImplemented": "Added extraction scheduler selection over pending captures, transactional persistence for extraction_batches and extractions, capture status transitions from pending to processed, and retry-safe failure handling. The feature also records nullable usage and cost metadata without blocking persistence when providers omit accounting fields.",
  "whatWasLeftUndone": "",
  "verification": {
    "commandsRun": [
      {
        "command": "cargo test extraction::tests::transactional_persistence -- --test-threads=1",
        "exitCode": 0,
        "observation": "Covers success, provider failure, malformed JSON, and retry-without-duplication paths."
      },
      {
        "command": "cargo test api::tests::health_and_stats -- --test-threads=1",
        "exitCode": 0,
        "observation": "Confirmed extraction changes did not regress local read surfaces."
      },
      {
        "command": "cargo check --workspace --all-targets",
        "exitCode": 0,
        "observation": "Workspace typechecks cleanly after the new scheduler and storage code."
      }
    ],
    "interactiveChecks": [
      {
        "action": "Started the daemon and POSTed a seeded extraction batch through the local test harness, then queried GET /api/stats and GET /api/captures/:id.",
        "observed": "Batch/accounting fields persisted, capture status flipped to processed, and the read surfaces showed the linked extraction metadata."
      }
    ]
  },
  "tests": {
    "added": [
      {
        "file": "src/pipeline/extraction/tests.rs",
        "cases": [
          {
            "name": "persists_batch_and_frame_rows_transactionally",
            "verifies": "No partial extraction state is committed when any write in the success path fails."
          },
          {
            "name": "provider_failure_keeps_captures_pending",
            "verifies": "Provider and parse failures never mark captures processed."
          }
        ]
      }
    ]
  },
  "discoveredIssues": []
}
```

## When to Return to Orchestrator

- The feature needs user action for `OPENROUTER_API_KEY`, Screen Recording permission, or another real integration prerequisite.
- A contract assertion cannot be implemented without changing mission boundaries or another pending milestone first.
- A command in `.factory/services.yaml` is wrong in a way you cannot confidently fix within the current feature.
- A native/macOS-specific blocker requires Swift or launchd changes better handled by the native worker.
