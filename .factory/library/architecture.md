# Screencap Architecture

## Purpose
Screencap is a macOS-only screen memory system that captures lightweight visual state, converts it into structured activity records, and synthesizes reusable context for local inspection and assistant-facing queries. The architecture is deliberately local-first: capture stays offline, higher-level understanding is isolated behind provider boundaries, and every user-facing surface reads from the same durable state model.

## V1 Commitments
Version 1 is intentionally narrow. The system is anchored by a Rust daemon that owns orchestration, storage, scheduling, and local serving. The embedded web UI is built with **Svelte** and served from the daemon on **`localhost:7878`**. Capture starts with a **timer-first MVP** rather than a complex event matrix: the product guarantees bounded temporal coverage first, then layers smarter triggering on top without changing the storage or pipeline contracts. AI integrations remain behind a **generic provider boundary** so extraction and synthesis flows do not encode vendor-specific assumptions.

Those commitments define the shape of the product: one local daemon, one local database, one embedded UI, one stable API port, and one provider abstraction that keeps model choice configurable rather than architectural.

## Layered Runtime Model
Screencap is organized as three durable layers.

1. **Capture** records screenshots and active-window metadata locally.
2. **Extraction** converts pending captures into typed frame understanding and batch summaries.
3. **Synthesis** turns extracted activity into rolling, hourly, and daily memory artifacts.

The key architectural choice is that each layer writes durable outputs that the next layer consumes later. Capture does not stream directly into extraction, and synthesis does not depend on ephemeral in-memory state. This keeps recovery, replay, backfill, and inspection straightforward while preserving a single source of truth for every surface.

## Runtime Surfaces
### Rust daemon
The Rust daemon is the operational center of the system. It owns the scheduler, SQLite access, screenshot file coordination, provider calls above the capture layer, and the local HTTP server. All mutation of pipeline state flows through this process.

### Native bridge
Swift code exists only as a thin bridge to Apple-native APIs such as ScreenCaptureKit and foreground-window metadata. Native policy does not live there; Rust remains responsible for filtering, scheduling, persistence, and state transitions.

### Local UI and control surfaces
The minimum read and control surfaces for v1 are intentionally small:
- the **CLI** for lifecycle, status, and read-oriented workflows
- the **HTTP API** on port `7878` for structured local access
- the **embedded Svelte web UI** served by that API for timeline, search, and insight views
- the **menu bar app** as a lightweight native control surface over daemon state

These surfaces are readers and operators over the daemon's persisted model, not alternative processing engines.

## Capture Architecture
The capture layer is continuous, cheap, and offline. In v1, its behavior is defined by a small set of rules that trade sophistication for predictability.

Capture is **timer-first**: the system guarantees a screenshot opportunity on a bounded cadence even if richer event triggers are added later. The minimum idle fallback remains **every 5 minutes**, which creates baseline temporal coverage without requiring user interaction. When capture is triggered by observed activity, the system waits **500 ms of settle time** before taking the screenshot so transient app-switch and window-animation states do not dominate the dataset. Capture applies **deduplication** before persistence, skipping frames that do not materially change the foreground context within the bounded window. Persisted screenshots follow a canonical date-partitioned filename convention under `~/.screencap/screenshots/YYYY/MM/DD/`, with filenames shaped as **`HHMMSS-{display_id}.jpg`** so on-disk artifacts remain predictable and sortable.

Capture persists only what is needed for downstream understanding: screenshot files plus active app and window metadata. It never performs OCR, never performs network I/O, and never bypasses privacy filters.

## Extraction and Synthesis
Extraction is the first layer allowed to talk to model providers. It reads pending captures in chronological order, batches them with their metadata, submits them through the shared provider interface, and turns model responses into typed records rather than opaque blobs. The important architectural boundary is not which provider is used but that the pipeline speaks to providers generically and persists normalized results.

Synthesis operates only on extracted activity, never on raw screenshots. Its job is to produce reusable time-window artifacts rather than ad hoc summaries for individual screens. The three canonical artifacts are:
- **rolling context** for "what is happening now"
- **hourly digests** as the canonical record of each hour
- **daily summaries** as the canonical record of a day

Because these artifacts are stored, every read surface can answer the same question from the same underlying record instead of recomputing divergent views.

## Storage Model
Screencap uses a split storage model: filesystem state for large binary artifacts and SQLite for structured system state.

The filesystem stores JPEG screenshots under the canonical date-partitioned layout. SQLite stores the durable entities that define the pipeline:
- **captures** for raw observations and capture lifecycle state
- **extraction batches** for grouped provider calls, raw responses, and batch-level accounting
- **extractions** for typed frame-level understanding
- **insights** for rolling, hourly, and daily synthesized artifacts
- **FTS indexes** for deterministic local search over extracted and synthesized narratives
- **usage and cost metadata** attached to provider-backed stages so operational spend is queryable from the same system of record

This model preserves a clean boundary between raw evidence, interpreted frames, and synthesized memory while still making the entire pipeline inspectable through one local database.

## Read Model
The architecture favors deterministic reads over live recomputation. The daemon exposes captures, search, current context, hourly views, daily views, analytics, and system health from persisted records. The CLI, API, web UI, and menu bar all converge on those same stored entities. Search is backed by FTS over normalized extraction and insight content, while timeline and summary surfaces are backed by canonical capture and insight records.

That read model matters as much as the write pipeline: the product is useful only if repeated queries over the same time range return stable, explainable answers.

## Provider Boundary
All model-backed behavior is isolated behind a generic provider interface capable of multimodal extraction and text synthesis. The boundary exists so OpenRouter, OpenAI-compatible local servers, and direct providers can be swapped by configuration without changing capture behavior, storage contracts, or user-facing read surfaces. Provider choice may affect cost, latency, or quality, but it must not alter the architectural shape of the system.

## Validation-Critical Invariants
Several invariants are architecture-defining rather than implementation details.

The **capture layer is permanently offline**: screenshot acquisition and window metadata collection never initiate network activity. **Privacy filtering happens before persistence** so excluded apps and excluded window-title patterns do not leak into screenshot files or capture rows. **Extraction persistence is transactional**: a batch is only considered committed when batch metadata, parsed extractions, status transitions, and usage/cost accounting are written coherently enough that the system can recover without ambiguous partial state. **Rolling, hourly, and daily outputs are canonical stored artifacts**, not ephemeral convenience summaries. And **reads and search must be deterministic** over persisted state: chronological ordering, time-window boundaries, and FTS-backed retrieval should return stable results for the same query inputs.

These invariants protect the product's core promises: privacy at the point of capture, trustworthy recovery semantics, and repeatable local memory retrieval.

## Architectural Summary
Screencap v1 is a local-first, macOS-native system centered on a Rust daemon, a Svelte UI on port `7878`, a timer-first capture pipeline, and a provider-agnostic AI boundary. Its architecture is successful only if capture remains offline and privacy-safe, extraction and synthesis persist typed canonical records, and every read surface resolves against the same deterministic storage model.
