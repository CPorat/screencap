# Environment

Environment variables, external dependencies, and setup notes.

**What belongs here:** required env vars, external API keys/services, local-path expectations, permission notes, and setup quirks.
**What does NOT belong here:** service ports or process commands (use `.factory/services.yaml`).

---

## Required for full end-to-end validation

- `OPENROUTER_API_KEY` — required for the default real extraction/synthesis validation path.
- Screen Recording permission for the logged-in macOS user session — required for real screenshot capture.

## Optional provider-specific variables

- `OPENAI_API_KEY` — only if a direct OpenAI-compatible provider path is implemented and selected.
- `ANTHROPIC_API_KEY` — optional direct Anthropic validation path.
- `GOOGLE_API_KEY` — optional direct Gemini validation path.
- LM Studio-compatible endpoints usually use `http://127.0.0.1:1234/v1` and may not require an API key.
- Ollama-compatible endpoints usually use `http://127.0.0.1:11434` and may not require an API key.

## Local state paths

- Config: `~/.screencap/config.toml`
- SQLite DB: `~/.screencap/screencap.db`
- Screenshots: `~/.screencap/screenshots/YYYY/MM/DD/`
- Daily markdown exports: `~/.screencap/daily/`

## Handling secrets and local config

- Never commit API keys, generated user config, or captured screenshots.
- If `config.example.toml` exists, workers may copy it to `~/.screencap/config.toml` locally during setup but must not commit the generated config.
- Capture must remain functional without cloud credentials; only extraction/synthesis/semantic analysis are credential-gated.

## Native macOS notes

- Validate capture and menu bar features only in an active logged-in macOS desktop session.
- Do not modify TCC/privacy databases or use `sudo` to force permissions.
- Treat existing local services on `127.0.0.1:1234` or `5432` as user-owned unless the user explicitly asks otherwise.
