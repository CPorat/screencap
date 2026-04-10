# User Testing

Testing surface findings, setup notes, and resource-cost guidance for validation workers.

**What belongs here:** how to validate each surface, required tools, setup gates, and safe concurrency.

---

## Validation Surface

### CLI and REST API

- Primary tools: shell commands + `curl`
- Surfaces: daemon lifecycle, `status`, `now`, capture listing/detail, screenshot serving, stats, search, export, and health endpoints
- Entry URL: `http://localhost:7878`
- This is the lightest validation surface and can run in parallel when the checks are read-only.

### Browser UI

- Primary tool: `agent-browser`
- Browser: Safari only for this mission
- Surfaces: Svelte shell, Timeline, Insights, Search, Stats, and embedded static asset serving
- Validators must exercise both seeded-data and first-run empty/degraded states when the claimed assertions require them.

### Native macOS surfaces

- Primary tools: deterministic build/test commands plus manual verification in the logged-in user session
- Surfaces: Screen Recording permission handling, timer/event capture, menu bar interactions, launch-on-login UX
- These flows are permission-sensitive and must remain serial.

### MCP

- Primary tool: deterministic stdio integration checks
- Surfaces: tool listing, current context, search, screenshots, summaries, project/app activity, grounded Q&A
- Validate response schema and no-data behavior, not just success cases.

### Real AI-backed paths

- Extraction, synthesis, semantic search, and ad-hoc analysis require `OPENROUTER_API_KEY` for the default end-to-end path.
- These validations also require real captures from milestones 2–3. Do not silently replace them with mocks.
- If the key or screenshots are missing, report the setup gap as blocked rather than passing a synthetic substitute.

## Validation Concurrency

Machine baseline measured during planning:
- 10 logical CPUs
- 32 GiB RAM
- load average around 4.15 at planning time
- usable CPU headroom under the 70% rule was about 2.85 CPU

Recommended max concurrent validators by surface:
- Browser / `agent-browser`: **1**
- Native macOS / menu bar / permission flows: **1**
- CLI / API / curl flows: **2**
- MCP stdio flows: **2**

Rationale:
- Browser and native macOS surfaces are the heaviest and most stateful; keep them serial.
- CLI/API and MCP checks are light enough to run two-at-a-time on this machine without overshooting conservative CPU headroom.

## Setup Notes

- Use port `7878` only.
- Native validations must never use `sudo` or modify TCC/privacy databases.
- Safari is the approved browser path; do not switch browsers unless the user changes the plan.
- For early milestones, validate only the assertions claimed by `fulfills`; do not expect future surfaces before their milestone exists.
