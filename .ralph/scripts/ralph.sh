#!/bin/bash
# Ralph — Autonomous AI agent loop
# Runs a coding agent repeatedly until all PRD stories are complete.
#
# Supported tools:
#   claude   — Claude Code CLI (npm i -g @anthropic-ai/claude-code)
#   codex    — OpenAI Codex CLI (npm i -g @openai/codex)
#   opencode — OpenCode CLI (https://opencode.ai)
#   omp      — Oh My Pi / Pi Coding Agent (bun i -g @oh-my-pi/pi-coding-agent)
#
# Per-task model override:
#   Add "tool", "model", "provider", or "thinking" fields to individual stories
#   in prd.json to override the defaults for that story. Example:
#     { "id": "US-015", "tool": "omp", "model": "claude-sonnet-4", "provider": "anthropic", ... }
#
# Expected directory layout (.ralph/ at project root):
#   .ralph/
#   ├── prd.json              # Product requirements document
#   ├── progress.txt          # Append-only progress log
#   ├── scripts/
#   │   ├── ralph.sh          # This script (or symlinked from agent-workspace)
#   │   └── PROMPT.md         # Agent prompt template
#   └── archive/              # Auto-archived previous runs
#
# Usage: .ralph/scripts/ralph.sh [--tool claude|codex|opencode|omp] [--provider <provider>] [--model <model>] [--thinking <level>] [max_iterations]
# Default: claude, 30 iterations. --provider, --model, --thinking are passed to omp only.

set -e

TOOL="claude"
MAX_ITERATIONS=30
MODEL=""
THINKING=""
PROVIDER=""

while [[ $# -gt 0 ]]; do
  case $1 in
    --tool)
      TOOL="$2"
      shift 2
      ;;
    --tool=*)
      TOOL="${1#*=}"
      shift
      ;;
    --model)
      MODEL="$2"
      shift 2
      ;;
    --model=*)
      MODEL="${1#*=}"
      shift
      ;;
    --thinking)
      THINKING="$2"
      shift 2
      ;;
    --thinking=*)
      THINKING="${1#*=}"
      shift
      ;;
    --provider)
      PROVIDER="$2"
      shift 2
      ;;
    --provider=*)
      PROVIDER="${1#*=}"
      shift
      ;;
    *)
      if [[ "$1" =~ ^[0-9]+$ ]]; then
        MAX_ITERATIONS="$1"
      fi
      shift
      ;;
  esac
done

VALID_TOOLS="claude codex opencode omp"
if ! echo "$VALID_TOOLS" | grep -qw "$TOOL"; then
  echo "Error: Invalid tool '$TOOL'. Must be one of: $VALID_TOOLS"
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RALPH_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PROJECT_DIR="$(cd "$RALPH_DIR/.." && pwd)"
PRD_FILE="$RALPH_DIR/prd.json"
PROGRESS_FILE="$RALPH_DIR/progress.txt"
PROMPT_FILE="$RALPH_DIR/scripts/PROMPT.md"
ARCHIVE_DIR="$RALPH_DIR/archive"
LAST_BRANCH_FILE="$RALPH_DIR/.last-branch"

# ── Preflight checks ────────────────────────────────────────────────

if [ ! -f "$PRD_FILE" ]; then
  echo "Error: No prd.json found at $PRD_FILE"
  echo "Create a PRD first. See: https://github.com/snarktank/ralph"
  exit 1
fi

if [ ! -f "$PROMPT_FILE" ]; then
  echo "Error: No PROMPT.md found at $PROMPT_FILE"
  echo "Create a prompt template at $PROMPT_FILE"
  exit 1
fi

if ! command -v jq &> /dev/null; then
  echo "Error: jq is required. Install with: brew install jq"
  exit 1
fi

check_tool_installed() {
  local cmd="$1"
  local install_hint="$2"
  if ! command -v "$cmd" &> /dev/null; then
    echo "Error: '$cmd' not found. Install with: $install_hint"
    exit 1
  fi
}

case "$TOOL" in
  claude)   check_tool_installed "claude"   "npm install -g @anthropic-ai/claude-code" ;;
  codex)    check_tool_installed "codex"    "npm install -g @openai/codex" ;;
  opencode) check_tool_installed "opencode" "https://opencode.ai/docs/install" ;;
  omp)      check_tool_installed "omp"      "bun install -g @oh-my-pi/pi-coding-agent" ;;
esac

# ── Archive previous run if branch changed ───────────────────────────

if [ -f "$PRD_FILE" ] && [ -f "$LAST_BRANCH_FILE" ]; then
  CURRENT_BRANCH=$(jq -r '.branchName // empty' "$PRD_FILE" 2>/dev/null || echo "")
  LAST_BRANCH=$(cat "$LAST_BRANCH_FILE" 2>/dev/null || echo "")

  if [ -n "$CURRENT_BRANCH" ] && [ -n "$LAST_BRANCH" ] && [ "$CURRENT_BRANCH" != "$LAST_BRANCH" ]; then
    DATE=$(date +%Y-%m-%d)
    FOLDER_NAME=$(echo "$LAST_BRANCH" | sed 's|^ralph/||')
    ARCHIVE_FOLDER="$ARCHIVE_DIR/$DATE-$FOLDER_NAME"

    echo "Archiving previous run: $LAST_BRANCH"
    mkdir -p "$ARCHIVE_FOLDER"
    [ -f "$PRD_FILE" ] && cp "$PRD_FILE" "$ARCHIVE_FOLDER/"
    [ -f "$PROGRESS_FILE" ] && cp "$PROGRESS_FILE" "$ARCHIVE_FOLDER/"
    echo "  Archived to: $ARCHIVE_FOLDER"

    echo "# Ralph Progress Log" > "$PROGRESS_FILE"
    echo "Started: $(date)" >> "$PROGRESS_FILE"
    echo "---" >> "$PROGRESS_FILE"
  fi
fi

# Track current branch
if [ -f "$PRD_FILE" ]; then
  CURRENT_BRANCH=$(jq -r '.branchName // empty' "$PRD_FILE" 2>/dev/null || echo "")
  if [ -n "$CURRENT_BRANCH" ]; then
    echo "$CURRENT_BRANCH" > "$LAST_BRANCH_FILE"
  fi
fi

# Initialize progress file if missing
if [ ! -f "$PROGRESS_FILE" ]; then
  echo "# Ralph Progress Log" > "$PROGRESS_FILE"
  echo "Started: $(date)" >> "$PROGRESS_FILE"
  echo "---" >> "$PROGRESS_FILE"
fi

# ── Run the loop ─────────────────────────────────────────────────────

PROJECT_NAME=$(jq -r '.project // "project"' "$PRD_FILE")
REMAINING=$(jq '[.userStories[] | select(.passes == false)] | length' "$PRD_FILE")
TOTAL=$(jq '[.userStories[]] | length' "$PRD_FILE")

MODEL_DISPLAY="${MODEL:-default}"
THINKING_DISPLAY="${THINKING:-default}"

echo ""
echo "╔═══════════════════════════════════════════════════╗"
echo "║  Ralph — Autonomous Agent Loop                    ║"
echo "║  Project: $PROJECT_NAME"
echo "║  Tool: $TOOL | Model: $MODEL_DISPLAY | Thinking: $THINKING_DISPLAY"
echo "║  Max iterations: $MAX_ITERATIONS"
echo "║  Stories: $((TOTAL - REMAINING))/$TOTAL complete, $REMAINING remaining"
echo "╚═══════════════════════════════════════════════════╝"
echo ""

run_agent() {
  local prompt_file="$1"
  local use_tool="$2"
  local use_model="$3"
  local use_provider="$4"
  local use_thinking="$5"
  local prompt_content
  prompt_content=$(cat "$prompt_file")

  case "$use_tool" in
    claude)
      claude --dangerously-skip-permissions --print < "$prompt_file" 2>&1 | tee /dev/stderr
      ;;
    codex)
      codex exec --full-auto -C "$PROJECT_DIR" "$prompt_content" 2>&1 | tee /dev/stderr
      ;;
    opencode)
      opencode run "$prompt_content" 2>&1 | tee /dev/stderr
      ;;
    omp)
      local omp_args=(--mode json -p)
      [ -n "$use_provider" ] && omp_args+=(--provider "$use_provider")
      [ -n "$use_model" ] && omp_args+=(--model "$use_model")
      [ -n "$use_thinking" ] && omp_args+=(--thinking "$use_thinking")
      omp "${omp_args[@]}" @"$prompt_file" \
        > >(tee "$ITER_LOG_DIR/events.jsonl") \
        2>"$ITER_LOG_DIR/stderr.log"
      ;;
  esac
}

print_omp_summary() {
  local log="$1"
  [ ! -f "$log" ] && return

  local tool_count error_count
  tool_count=$(jq -s '[.[] | select(.type == "tool_execution_end")] | length' "$log" 2>/dev/null || echo 0)
  error_count=$(jq -s '[.[] | select(.type == "tool_execution_end" and .isError)] | length' "$log" 2>/dev/null || echo 0)

  echo ""
  echo "  ┌─ Iteration Summary ─────────────────────────────"

  jq -rs '
    [.[] | select(.type == "turn_end") | .message.usage] |
    if length > 0 then
      { turns: length,
        input: (map(.input // 0) | add),
        output: (map(.output // 0) | add),
        cost: (map(.cost.total // 0) | add) } |
      "  │ Cost: $\(.cost | tostring | .[0:8])  Tokens: \(.input)in / \(.output)out  Turns: \(.turns)"
    else "  │ Cost: (unavailable)" end
  ' "$log" 2>/dev/null

  echo "  │ Tools: $tool_count calls, $error_count errors"

  if [ "$error_count" -gt 0 ]; then
    echo "  │"
    echo "  │ Errors:"
    jq -r 'select(.type == "tool_execution_end" and .isError)
      | "  │   \(.toolName): \(.result.content[0].text[:150] // "unknown")"' "$log" 2>/dev/null
  fi

  echo "  │ Log: $log"
  echo "  └─────────────────────────────────────────────────"
  echo ""
}

LOGS_DIR="$RALPH_DIR/logs"

for i in $(seq 1 $MAX_ITERATIONS); do
  INCOMPLETE=$(jq '[.userStories[] | select(.passes == false)] | length' "$PRD_FILE")

  if [ "$INCOMPLETE" -eq 0 ]; then
    echo ""
    echo "All stories complete!"
    exit 0
  fi

  NEXT_STORY=$(jq -r '[.userStories[] | select(.passes == false)] | sort_by(.priority) | .[0] | "\(.id): \(.title)"' "$PRD_FILE")
  NEXT_STORY_ID=$(jq -r '[.userStories[] | select(.passes == false)] | sort_by(.priority) | .[0] | .id' "$PRD_FILE")
  TASK_TOOL=$(jq -r '[.userStories[] | select(.passes == false)] | sort_by(.priority) | .[0] | .tool // empty' "$PRD_FILE")
  TASK_MODEL=$(jq -r '[.userStories[] | select(.passes == false)] | sort_by(.priority) | .[0] | .model // empty' "$PRD_FILE")
  TASK_PROVIDER=$(jq -r '[.userStories[] | select(.passes == false)] | sort_by(.priority) | .[0] | .provider // empty' "$PRD_FILE")
  TASK_THINKING=$(jq -r '[.userStories[] | select(.passes == false)] | sort_by(.priority) | .[0] | .thinking // empty' "$PRD_FILE")

  USE_TOOL="${TASK_TOOL:-$TOOL}"
  USE_MODEL="${TASK_MODEL:-$MODEL}"
  USE_PROVIDER="${TASK_PROVIDER:-$PROVIDER}"
  USE_THINKING="${TASK_THINKING:-$THINKING}"

  if ! echo "$VALID_TOOLS" | grep -qw "$USE_TOOL"; then
    echo "Warning: Story specifies invalid tool '$USE_TOOL', falling back to '$TOOL'"
    USE_TOOL="$TOOL"
  fi

  ITER_LOG_DIR="$LOGS_DIR/$(date +%Y-%m-%dT%H%M%S)-iter${i}-${NEXT_STORY_ID}"
  mkdir -p "$ITER_LOG_DIR"
  export ITER_LOG_DIR

  echo "═══════════════════════════════════════════════════"
  echo " Iteration $i/$MAX_ITERATIONS — Next: $NEXT_STORY"
  if [ -n "$TASK_TOOL" ] || [ -n "$TASK_MODEL" ]; then
    echo " Override — Tool: ${USE_TOOL} | Model: ${USE_MODEL:-default} | Provider: ${USE_PROVIDER:-default}"
  fi
  echo "═══════════════════════════════════════════════════"

  if [ "$USE_TOOL" = "omp" ]; then
    run_agent "$PROMPT_FILE" "$USE_TOOL" "$USE_MODEL" "$USE_PROVIDER" "$USE_THINKING" || true

    OUTPUT=$(jq -rs '
      [.[] | select(.type == "agent_end") | .messages[]
       | select(.role == "assistant") | .content[]
       | select(.type == "text") | .text] | last // ""
    ' "$ITER_LOG_DIR/events.jsonl" 2>/dev/null)

    print_omp_summary "$ITER_LOG_DIR/events.jsonl"
  else
    OUTPUT=$(run_agent "$PROMPT_FILE" "$USE_TOOL" "$USE_MODEL" "$USE_PROVIDER" "$USE_THINKING") || true
  fi

  if echo "$OUTPUT" | grep -q "<promise>COMPLETE</promise>"; then
    echo ""
    echo "Ralph completed all tasks after $i iterations!"
    exit 0
  fi

  echo ""
  echo "Iteration $i complete. Continuing..."
  sleep 2
done

echo ""
echo "Ralph reached max iterations ($MAX_ITERATIONS) without completing all tasks."
echo "Check $PROGRESS_FILE for status."
exit 1
