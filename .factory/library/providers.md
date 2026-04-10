# Providers

Provider-boundary facts for Screencap workers.

**What belongs here:** provider selection rules, trait expectations, request/response invariants, and normalization expectations.

---

## Shared abstraction

All model backends sit behind a single `LlmProvider` boundary.

- Extraction uses a multimodal completion path: prompt + ordered image inputs.
- Synthesis/search/export analysis uses a text completion path.
- Pipeline code should depend on the trait, not on provider-specific request/response shapes.

## Provider families

### OpenAI-compatible family

Use one shared client for providers that speak the OpenAI chat-completions style API.

Expected support:
- OpenRouter (default real validation path)
- OpenAI-compatible direct endpoints if added
- LM Studio-style local endpoints (`base_url` override)

The client must preserve configured model IDs and support nullable usage/accounting fields.

### Native direct clients

Keep separate native clients for:
- Anthropic
- Google/Gemini
- Ollama

These should adapt into the same trait without leaking provider-specific details into capture/extraction/synthesis scheduling code.

## Normalization rules

- Preserve raw provider responses where the schema expects `raw_response` for debugging.
- Persist typed structured fields for activity type, sentiment, topics, people, and project names.
- Treat missing usage/cost metadata as nullable accounting, not as extraction/synthesis failure.

## Mission defaults

- Real validation path: OpenRouter + `OPENROUTER_API_KEY`
- Extraction must work with ordered screenshot batches.
- Provider failures must never corrupt capture/extraction/synthesis state transitions.
