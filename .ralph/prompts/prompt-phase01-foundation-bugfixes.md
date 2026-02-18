# Phase 1: Foundation Bug Fixes — workspace_root, loop age, failure display

## Context

This is phase 1 of the Ralph Project Awareness implementation. All subsequent phases
depend on these bug fixes being correct. Do NOT implement any new features — only fix
the listed bugs in existing code.

## Bug 1: `find_workspace_root()` — subdirectory invocation creates `.ralph/` in wrong place

### Problem
Multiple locations resolve workspace root via bare `std::env::current_dir()` without
traversing up to the git root. Running `ralph` from `myapp/src/` creates `.ralph/` in
`myapp/src/.ralph/` instead of `myapp/.ralph/`.

| Location | Code | Bug |
|----------|------|-----|
| `main.rs:356` | `config.core.workspace_root = std::env::current_dir()` | `ralph run` from subdir |
| `main.rs:1148-1149` | same pattern (E2E test path) | same bug |
| `main.rs:1814` | `let workspace_root = std::env::current_dir()` | `ralph clean` from subdir |
| `memory.rs:217` | `PathBuf::from(".")` fallback | relative path, not even absolute |
| `loops.rs` | `LoopRegistry::new(cwd)` | `cwd = current_dir()` |

### Fix
1. Add a `find_workspace_root()` function in `main.rs` that walks up from CWD looking
   for `.git/` directory. Falls back to CWD if no `.git/` found (non-git projects).
2. Call it once early in `main()` before command dispatch.
3. Use it to set `config.core.workspace_root` instead of `current_dir()`.
4. In `tools.rs`, when calling `memory::execute()`, inject the resolved workspace root
   into `args.root` so `PathBuf::from(".")` fallback is never reached.
5. Pass the resolved root into `loops.rs` functions.

### Test
Write a test that creates a temp git repo, `cd`s into a subdirectory, calls
`find_workspace_root()`, and asserts it returns the git root, not the subdirectory.
Also test the non-git fallback case.

---

## Bug 2: Running loop age always shows `-`

### Problem
`crates/ralph-cli/src/loops.rs` around line 310:
```rust
age: None, // Registry doesn't track start time
```
This comment is wrong. `LoopEntry.started` IS a `DateTime<Utc>` that is stored in the
registry. The merge queue entries already correctly compute age at line 337:
```rust
let age = Some(format_age(now.signed_duration_since(entry.queued_at)));
```

### Fix
Replace `age: None` with:
```rust
age: Some(format_age(now.signed_duration_since(entry.started))),
```
Apply the same fix for the primary loop entry (around line 284 area where LoopRows are
built for the primary/lock holder).

### Test
No new test needed — existing display tests cover this if they exist. Verify manually
with `ralph loops list` after fix.

---

## Bug 3: `failure_reason` not displayed for `needs-review` entries

### Problem
`crates/ralph-cli/src/loops.rs` around line 329: sets `has_needs_review = true` for
`MergeState::NeedsReview` entries but never reads or displays `entry.failure_reason`.
The field exists on `MergeQueueEntry` (check `merge_queue.rs` for the struct).

### Fix
After building the `LoopRow` for a `NeedsReview` entry, if `entry.failure_reason` is
`Some(reason)`, append an indented line below the row:
```
  ↳ {reason}
```
Follow the display pattern shown in the row-printing logic.

### Test
Add a unit test that constructs a `NeedsReview` merge queue entry with a `failure_reason`,
runs it through the display logic, and asserts the reason appears in output.

---

## Bug 4: Worktree LOCATION shows only final component

### Problem
`loops.rs` line 302 calls `shorten_path()` on `entry.worktree_path`, which strips to
just the last path component (e.g., `loop-abc123` instead of `.worktrees/loop-abc123`).

### Fix
Show the full relative path from workspace root. If `worktree_path` starts with the
workspace root, strip the prefix and display the remainder. Otherwise display as-is.

---

## Verification

1. `cargo build` — compiles cleanly
2. `cargo test` — all existing tests pass
3. New `find_workspace_root()` tests pass
4. Review all changes are minimal bug fixes — no new features, no new CLI flags
