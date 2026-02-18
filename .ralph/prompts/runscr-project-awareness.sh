#!/usr/bin/env bash
set -euo pipefail

#
# runscr-project-awareness.sh
#
# Executes the 7-phase Ralph Project Awareness implementation.
# Each phase runs as a single `ralph run` invocation with a dedicated
# prompt file and preset suited to its task type.
#
# Dependency Graph:
#
#   Phase 1 (foundation bug fixes)
#      │
#      ▼
#   Phase 2 (known-projects registry)
#      │
#      ├──────────────┐
#      ▼              ▼
#   Phase 3        Phase 5
#   (corpora       (global loops)
#    hardening)       │
#      │              ▼
#      ▼           Phase 6
#   Phase 4        (global mem/events/emit/clean)
#   (corpora          │
#    tests)           │
#      │              │
#      └──────┬───────┘
#             ▼
#          Phase 7
#          (integration tests)
#
# Phases 3-4 and 5-6 are independent branches that can run in parallel.
# Phase 7 depends on both branches completing.
#

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
LOG_DIR="$PROJECT_ROOT/.ralph/phase-logs"

# Config
BACKEND="${RALPH_BACKEND:-claude}"
MAX_ITER="${RALPH_MAX_ITER:-50}"
RALPH="${RALPH_BIN:-ralph}"
PARALLEL="${RALPH_PARALLEL:-false}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

mkdir -p "$LOG_DIR"

log() { echo -e "${CYAN}[$(date +%H:%M:%S)]${NC} $*"; }
ok()  { echo -e "${GREEN}[$(date +%H:%M:%S)] ✓${NC} $*"; }
err() { echo -e "${RED}[$(date +%H:%M:%S)] ✗${NC} $*"; }
warn(){ echo -e "${YELLOW}[$(date +%H:%M:%S)] ⚠${NC} $*"; }

run_phase() {
    local phase="$1"
    local preset="$2"
    local prompt_file="$3"
    local description="$4"
    local logfile="$LOG_DIR/phase-${phase}.log"

    log "Phase ${phase}: ${description}"
    log "  Preset: builtin:${preset}"
    log "  Prompt: ${prompt_file}"
    log "  Log:    ${logfile}"

    cd "$PROJECT_ROOT"

    if $RALPH -c "builtin:${preset}" run \
        -b "$BACKEND" \
        -P "$prompt_file" \
        --max-iterations "$MAX_ITER" \
        -a \
        2>&1 | tee "$logfile"; then
        ok "Phase ${phase} complete"
    else
        err "Phase ${phase} FAILED — see ${logfile}"
        return 1
    fi

    # Verify build + tests after each phase
    log "  Verifying cargo build..."
    if ! cargo build 2>>"$logfile"; then
        err "Phase ${phase} broke the build — see ${logfile}"
        return 1
    fi

    log "  Verifying cargo test..."
    if ! cargo test 2>>"$logfile"; then
        err "Phase ${phase} broke tests — see ${logfile}"
        return 1
    fi

    ok "Phase ${phase} verified (build + tests pass)"
    echo ""
}

# ─────────────────────────────────────────────────────────
# Phase 1: Foundation bug fixes
#   Preset: bugfix — scientific method: find it, fix it, verify it
#   No dependencies
# ─────────────────────────────────────────────────────────
run_phase 1 "bugfix" \
    "$SCRIPT_DIR/prompt-phase01-foundation-bugfixes.md" \
    "Foundation bug fixes (workspace_root, loop age, failure display)"

# ─────────────────────────────────────────────────────────
# Phase 2: Known-projects registry
#   Preset: code-assist — new module from scratch with tests
#   Depends on: Phase 1
# ─────────────────────────────────────────────────────────
run_phase 2 "code-assist" \
    "$SCRIPT_DIR/prompt-phase02-known-projects.md" \
    "Known-projects registry (new module)"

# ─────────────────────────────────────────────────────────
# Branch A: Corpora hardening + tests (Phases 3-4)
# Branch B: Global scope (Phases 5-6)
#
# These branches are independent — Phase 3 touches memory/corpora,
# Phase 5 touches loops/registry. No file overlap.
# ─────────────────────────────────────────────────────────

if [ "$PARALLEL" = "true" ]; then
    log "Running Branch A (corpora) and Branch B (global loops) in parallel..."

    # Branch A: Phase 3 → Phase 4
    (
        run_phase 3 "bugfix" \
            "$SCRIPT_DIR/prompt-phase03-corpora-hardening.md" \
            "Shared corpora hardening (tilde, filter, doctor, docs)" && \
        run_phase 4 "code-assist" \
            "$SCRIPT_DIR/prompt-phase04-corpora-tests.md" \
            "Shared corpora tests (8 scenarios)"
    ) &
    PID_A=$!

    # Branch B: Phase 5 → Phase 6
    (
        run_phase 5 "code-assist" \
            "$SCRIPT_DIR/prompt-phase05-global-loops.md" \
            "Global scope — loops list + subcommand routing" && \
        run_phase 6 "code-assist" \
            "$SCRIPT_DIR/prompt-phase06-global-remaining.md" \
            "Global scope — memory, events, emit, clean"
    ) &
    PID_B=$!

    # Wait for both branches
    FAIL=0
    wait $PID_A || { err "Branch A (corpora) failed"; FAIL=1; }
    wait $PID_B || { err "Branch B (global scope) failed"; FAIL=1; }

    if [ "$FAIL" -ne 0 ]; then
        err "One or more parallel branches failed. Cannot proceed to Phase 7."
        exit 1
    fi

    ok "Both branches complete"
else
    # Sequential execution (default — safer, uses one context at a time)

    # Branch A
    run_phase 3 "bugfix" \
        "$SCRIPT_DIR/prompt-phase03-corpora-hardening.md" \
        "Shared corpora hardening (tilde, filter, doctor, docs)"

    run_phase 4 "code-assist" \
        "$SCRIPT_DIR/prompt-phase04-corpora-tests.md" \
        "Shared corpora tests (8 scenarios)"

    # Branch B
    run_phase 5 "code-assist" \
        "$SCRIPT_DIR/prompt-phase05-global-loops.md" \
        "Global scope — loops list + subcommand routing"

    run_phase 6 "code-assist" \
        "$SCRIPT_DIR/prompt-phase06-global-remaining.md" \
        "Global scope — memory, events, emit, clean"
fi

# ─────────────────────────────────────────────────────────
# Phase 7: Integration tests
#   Preset: code-assist — test-focused, verifies everything works together
#   Depends on: ALL prior phases (both branches)
# ─────────────────────────────────────────────────────────
run_phase 7 "code-assist" \
    "$SCRIPT_DIR/prompt-phase07-integration-tests.md" \
    "Integration tests — project awareness end-to-end"

# ─────────────────────────────────────────────────────────
# Final verification
# ─────────────────────────────────────────────────────────
echo ""
log "Final verification..."
cd "$PROJECT_ROOT"

if cargo build && cargo test; then
    echo ""
    ok "═══════════════════════════════════════════════════"
    ok " All 7 phases complete. Build and tests pass."
    ok "═══════════════════════════════════════════════════"
    echo ""
    log "Phase logs: $LOG_DIR/"
    log "Next: manual smoke test with two real projects"
else
    err "Final verification failed!"
    exit 1
fi
