#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCREENCAP_HOME="${HOME}/.screencap"

mkdir -p "${SCREENCAP_HOME}/screenshots" "${SCREENCAP_HOME}/daily"

if [ -f "${ROOT}/config.example.toml" ] && [ ! -f "${SCREENCAP_HOME}/config.toml" ]; then
  cp "${ROOT}/config.example.toml" "${SCREENCAP_HOME}/config.toml"
fi

if [ -f "${ROOT}/Cargo.toml" ]; then
  cargo fetch --manifest-path "${ROOT}/Cargo.toml" || true
fi

if [ -f "${ROOT}/web/package.json" ]; then
  npm --prefix "${ROOT}/web" install
fi

if [ -z "${OPENROUTER_API_KEY:-}" ]; then
  echo "OPENROUTER_API_KEY is not set; real extraction/synthesis validation will remain blocked until the user provides it."
fi
