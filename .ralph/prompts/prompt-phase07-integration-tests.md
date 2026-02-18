# Phase 7: Integration Tests — Project Awareness End-to-End

## Context

This is phase 7 (final). All features are implemented. This phase adds integration tests
that exercise the full project awareness system end-to-end: multiple simultaneous projects,
no cross-project interference, global commands returning correct results.

## Test Environment Setup

Each test creates isolated temp directories simulating multiple Ralph projects:

```rust
fn setup_multi_project() -> (TempDir, TempDir, TempDir) {
    // project_a: git init, .ralph/ dir, ralph.yml, memories, loops
    // project_b: git init, .ralph/ dir, ralph.yml, memories, loops
    // home_dir: temp ~/.ralph/ with known-projects.json
    // Set HOME env var to home_dir for test isolation
}
```

Use `tempfile::TempDir` for automatic cleanup. Override `HOME` env var in tests
to use an isolated `~/.ralph/` directory (avoid polluting real user state).

---

## Test 1: Two Projects — No Cross-Interference

```
Setup:
  project_a: register, add 3 memories, create a loop entry
  project_b: register, add 2 memories, create a loop entry

Assert:
  project_a memory list shows exactly 3 (not 5)
  project_b memory list shows exactly 2 (not 5)
  project_a loops list shows exactly 1 loop (not 2)
  project_b loops list shows exactly 1 loop (not 2)
  Loop IDs are different despite potentially same timestamp
  Neither project's .ralph/ contains the other project's state
```

---

## Test 2: Global Loops List — Aggregation Correct

```
Setup:
  project_a: 1 running loop, 1 needs-review merge entry
  project_b: 2 running loops

Assert:
  ralph loops list --global shows:
    project_a: 2 entries (1 running, 1 needs-review)
    project_b: 2 entries (2 running)
  Summary: "2 projects · 4 loops (3 running, 1 needs-review)"
```

---

## Test 3: Global Stop — Routes to Correct Project

```
Setup:
  project_a: loop "loop-aaa-1111"
  project_b: loop "loop-bbb-2222"

Assert:
  From project_a, `loops stop loop-bbb-2222 --global` finds it in project_b
  Loop in project_b is stopped (PID killed or marked)
  Loop in project_a is unaffected
```

---

## Test 4: Global Memory List — Aggregation with Filters

```
Setup:
  project_a: 3 pattern memories, 2 fix memories, tags [api, rust]
  project_b: 1 decision memory, tags [arch]

Assert:
  memory list --global shows all 6 grouped by project
  memory list --global -t pattern shows 3 (all from project_a)
  memory search --global "api" shows only matching memories from project_a
```

---

## Test 5: Emit to Named Project

```
Setup:
  project_a: registered as "project-a"
  project_b: registered as "project-b"

Assert:
  From project_a, `emit "human.guidance" "test" --project project-b`
  project_b's events file contains the event
  project_a's events file does NOT contain the event
```

---

## Test 6: Global Clean Dry-Run

```
Setup:
  project_a: create .ralph/agent/ with 5 files (100KB total)
  project_b: create .ralph/agent/ with 3 files (50KB total)
  project_a: create .ralph/diagnostics/ with 2 files (200KB)

Assert:
  clean --global --dry-run outputs:
    project_a: shows size for agent/ + diagnostics/
    project_b: shows size for agent/
    Total: correct sum
  NO files are deleted (verify all still exist)
```

---

## Test 7: Stale Project Pruning During Global Operations

```
Setup:
  Register project_a and project_b
  Delete project_b's directory from disk

Assert:
  loops list --global only shows project_a
  known-projects.json no longer contains project_b
  No error emitted for the missing project
```

---

## Test 8: Shared Corpus + Global — Distinct Concepts

```
Setup:
  shared_corpus: ~/.ralph/corpora/test-corpus.md with 2 memories
  project_a: ralph.yml with memories.shared = [shared_corpus]
  project_b: no shared corpora configured

Assert:
  project_a `prime` output includes labeled shared corpus
  project_b `prime` output does NOT include the shared corpus
  `memory list --global` shows only local memories from both projects
    (shared corpora are NOT included in global listing — they're config, not project state)
```

---

## Test 9: Subdirectory Invocation — Correct Project Detected

```
Setup:
  project_a: git init, .ralph/ at root
  Create subdirectory project_a/src/lib/

Assert:
  Running from project_a/src/lib/:
    find_workspace_root() returns project_a (git root)
    memory add writes to project_a/.ralph/agent/memories.md
    loops list reads project_a/.ralph/loops.json
    NOT project_a/src/lib/.ralph/ (which should not exist)
```

---

## Test 10: Concurrent Access — No Data Corruption

```
Setup:
  project_a: registered

Test:
  Spawn 5 threads, each registering a different project simultaneously
  All registrations complete without error
  known-projects.json contains all 5 projects
  File is valid JSON (not corrupted by concurrent writes)
```

---

## Verification

1. `cargo test` — all 10 integration tests pass
2. No existing tests break
3. Each test is fully isolated (temp dirs, overridden HOME)
4. Tests clean up automatically via TempDir drop
5. Run `cargo test -p ralph-core -- --nocapture` to see test output during development
