# Gap Analysis Results

> Analysis date: 2026-01-14
> Specs analyzed: 14
> Previous analysis: 2026-01-13

## Summary

| Priority | Issue | Spec | Status |
|----------|-------|------|--------|
| 🟡 P1 | Backend flag filtering uses config default, not execution mode | interactive-mode.spec.md | NEW |
| 🟡 P1 | Reachability validation not implemented | hat-collections.spec.md | PLANNED |
| 🟡 P1 | Observer not wired to EventBus during benchmark runs | benchmark-harness.spec.md | TODO |
| 🟢 P2 | Custom hat emoji/registry integration missing | terminal-ui.spec.md | MINOR |
| 🟢 P2 | recovery_hat configuration field missing | hat-collections.spec.md | PLANNED |
| 🟢 P2 | terminal_events configuration field missing | hat-collections.spec.md | PLANNED |
| 🟢 P2 | Test tools not agent-callable | test-tools.spec.md | ~30% |
| 🟢 P2 | Benchmark export formats not implemented | benchmark-ux.spec.md | PLANNED |
| 🔵 P3 | 6 specs missing frontmatter | Various | DOCS |

### Resolved Since 2026-01-13

The following P0 issues from the previous analysis have been **fixed**:

| Issue | Resolution |
|-------|------------|
| Custom backend args/prompt_flag missing | ✅ Fixed: `CliConfig` now has `args`, `prompt_flag`, `prompt_mode` fields |
| Custom backend validation missing | ✅ Fixed: Returns `CustomBackendError` when command not specified |
| Kiro adapter settings missing | ✅ Fixed: `AdaptersConfig` now includes `kiro` field |
| Per-adapter timeout not enforced | ✅ Fixed: `cli_executor.rs` reads and enforces per-adapter timeout |
| CLI executor working directory not set | ✅ Fixed: Both CLI and PTY executors explicitly set `current_dir()` |
| Interactive idle timeout doesn't reset | ✅ Fixed: Resets on both agent output AND user input |

---

## Critical Gaps (Spec Violations)

### interactive-mode.spec.md — Backend Flag Filtering Logic Issue

- **Spec says:** "Agent flags are determined by execution mode, not config default" (lines 264-284)
- **Implementation:** `build_command()` receives `interactive` boolean from config's `default_mode`, not actual execution mode
- **Location:** `crates/ralph-adapters/src/cli_backend.rs:179`, `crates/ralph-cli/src/main.rs:1134`
- **Impact:** When Claude backend forces PTY mode (interactive) but config says `default_mode: "autonomous"`, other backends like kiro may receive incorrect flags
- **Severity:** Medium
- **Fix:** Pass the actual execution mode boolean to `build_command()`, not just the config setting

---

## Missing Features (Spec Not Implemented)

### hat-collections.spec.md — Reachability Validation

- **Acceptance criterion:** "Error 'Hat X is unreachable from entry point' when hat cannot be reached via event flow"
- **Status:** Not implemented - no DFS graph traversal exists
- **Location:** Should be in `crates/ralph-core/src/config.rs:preflight_check()`
- **Spec reference:** Lines 224-248 (detailed algorithm provided)
- **Workaround:** Orphan event detection catches most misconfigurations

### hat-collections.spec.md — Recovery Hat Configuration

- **Acceptance criterion:** "Configurable `recovery_hat` field with default to 'planner'"
- **Status:** Field does not exist in `EventLoopConfig`
- **Location:** `crates/ralph-core/src/config.rs:617-652` (EventLoopConfig struct)
- **Current behavior:** Hardcoded to "planner" at `crates/ralph-core/src/event_loop.rs:288`

### hat-collections.spec.md — Terminal Events Configuration

- **Acceptance criterion:** "Configurable `terminal_events` list"
- **Status:** Field does not exist - hardcoded to `["LOOP_COMPLETE", completion_promise]`
- **Location:** `crates/ralph-core/src/config.rs:448`

### hat-collections.spec.md — Strict Validation Bypass

- **Acceptance criterion:** "`strict_validation: false` downgrades errors to warnings"
- **Status:** Field does not exist in `EventLoopConfig`

### terminal-ui.spec.md — Custom Hat Emoji Support

- **Acceptance criterion:** "Custom hats display with configurable emoji"
- **Status:** No mechanism to assign emojis to custom hats beyond hardcoded "🎭"
- **Location:** `crates/ralph-tui/src/state.rs:44-64`
- **Severity:** Low (affects only custom hat scenarios)

### terminal-ui.spec.md — HatRegistry Integration for Custom Hats

- **Acceptance criterion:** "TUI initialized with HatRegistry to look up subscriptions" (spec line 99)
- **Status:** TUI hardcodes mappings for planner and builder only
- **Location:** `crates/ralph-tui/src/state.rs`
- **Severity:** Medium (custom hats with non-standard subscriptions won't display correctly)

### benchmark-harness.spec.md — Observer Wiring

- **Acceptance criterion:** "Observer subscribes to EventBus::publish() and logs all events"
- **Status:** SessionRecorder created but not wired to EventBus during task execution
- **Location:** `crates/ralph-bench/src/main.rs:397` (TODO comment exists)
- **Impact:** Events aren't actually recorded to JSONL during benchmark runs
- **Fix:** Wire `SessionRecorder::make_observer()` to `EventLoop::set_observer()`

### benchmark-ux.spec.md — Export Formats

- **Acceptance criterion:** "Export to asciinema cast, VHS tape, and SVG formats"
- **Status:** Not implemented - no `ralph-bench export` subcommand
- **What exists:** SessionPlayer can replay JSONL with timing and ANSI colors

### benchmark-ux.spec.md — Snapshot Testing

- **Acceptance criterion:** "Snapshot testing with insta integration"
- **Status:** Not implemented - no snapshot comparison infrastructure

### test-tools.spec.md — Agent-Callable Test Tools

- **Acceptance criteria:** 9 test tools callable by agents
- **Status:** ~30% implemented - infrastructure exists but tools not exposed
- **What exists:**
  - SessionRecorder (JSONL capture) ✓
  - SessionPlayer (replay with timing) ✓
  - TaskWorkspace (isolation) ✓
  - EventLoop (orchestration) ✓
- **What's missing:**
  - Agent-callable tool wrappers
  - Mock backend for deterministic testing
  - 14 assertion types engine
  - VCR cassette format (YAML)
  - LLM-as-judge evaluation runner
  - JUnit/TAP report generation

---

## Undocumented Behavior (Implementation Without Spec)

### ralph-bench crate

- **Behavior:** Full benchmarking binary with run/list/replay/show subcommands
- **Should be documented in:** benchmark-harness.spec.md or new ralph-bench.spec.md
- **Location:** `crates/ralph-bench/src/main.rs`

### Task abandonment event

- **Behavior:** Emits `build.task.abandoned` event after 3 consecutive blocks
- **Should be documented in:** event-loop.spec.md (implicit but event name not specified)
- **Location:** `crates/ralph-core/src/event_loop.rs:496-501`

---

## Spec Improvements Needed

### Missing Frontmatter (6 specs)

The following specs lack standard YAML frontmatter (status, gap_analysis, related):

- feature-parity.spec.md
- v1-v2-feature-parity.spec.md
- benchmark-tasks.spec.md
- benchmark-harness.spec.md
- benchmark-ux.spec.md
- homebrew-tap.spec.md

### hat-collections.spec.md — Stale gap_analysis

- **Problem:** `gap_analysis: null` despite partial implementation existing
- **Suggestion:** Update to `gap_analysis: 2026-01-14`

---

## Verification Summary by Spec

| Spec | Status | Conformance | Notes |
|------|--------|-------------|-------|
| cli-adapters.spec.md | review | **100%** | All acceptance criteria pass ✅ |
| interactive-mode.spec.md | review | **95%** | Backend flag filtering issue |
| event-loop.spec.md | approved | **100%** | All acceptance criteria pass ✅ |
| scaffolding.spec.md | implemented | **100%** | All acceptance criteria pass ✅ |
| terminal-ui.spec.md | implemented | **98%** | Custom hat emoji/registry minor |
| hat-collections.spec.md | draft | **60%** | 5 validations done, 5 missing |
| behavioral-verification.spec.md | draft | N/A | Not analyzed (draft) |
| test-tools.spec.md | review | **30%** | Infrastructure exists, tools not callable |
| feature-parity.spec.md | N/A | N/A | Missing frontmatter |
| v1-v2-feature-parity.spec.md | N/A | **100%** | Checklist complete ✅ |
| benchmark-tasks.spec.md | N/A | **95%** | Nearly complete |
| benchmark-harness.spec.md | N/A | **85%** | Observer wiring needed |
| benchmark-ux.spec.md | N/A | **80%** | Export formats missing |
| homebrew-tap.spec.md | N/A | N/A | External distribution |

---

## Recommended Priority

### P0 — None Currently

All previous P0 issues have been resolved.

### P1 — Fix Before Release

1. **Interactive mode backend flag filtering** (medium impact, small fix)
2. **Benchmark observer wiring** (enables actual session recording)

### P2 — Implement When Needed

1. Hat-collections reachability validation
2. Hat-collections recovery_hat/terminal_events configuration
3. Terminal-ui custom hat registry integration
4. Test-tools agent-callable wrappers
5. Benchmark export formats

### P3 — Future Enhancement

1. Test-tools full implementation (mock backend, cassettes, LLM-as-judge)
2. Homebrew tap setup
3. cargo-dist distribution pipeline
4. Add missing frontmatter to 6 specs
