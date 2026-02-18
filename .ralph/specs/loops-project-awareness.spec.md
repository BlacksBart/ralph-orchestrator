# Spec: Ralph Project Awareness — Local and Global Scope

## Summary

Every Ralph CLI operation that reads or writes project-scoped state must gain awareness
of **which project** it is operating on and support both local (current project) and
global (all known projects) scope. This applies uniformly across loops, memories,
events, clean, and emit — not just the `loops` subcommand.

Today all commands silently assume CWD is the project root and have no concept of
global scope. A developer running multiple Ralph projects on the same machine has no
unified view and must `cd` into each project to inspect or manage its state.

## The Principle

**Every stateful Ralph command has two scope modes:**

| Mode | Flag | Meaning |
|------|------|---------|
| Local (default) | _(none)_ | Operate on the project in or containing the CWD |
| Global | `--global` / `-g` | Operate across all known projects on this machine |

**Affected commands and their stateful objects:**

| Command | Local State | Global State |
|---------|-------------|--------------|
| `ralph loops list` | Loops in CWD project | All loops across all projects |
| `ralph loops stop/retry/diff/attach` | Resolve loop in CWD project | Resolve loop in any project |
| `ralph tools memory list/search/prime` | Memories in CWD project | Memories across all projects |
| `ralph events` | Events in CWD project | Events across all projects |
| `ralph emit` | Emit to CWD project's event bus | Emit to a named project's event bus |
| `ralph clean` | Clean CWD project artifacts | Clean artifacts across all/selected projects |

---

## Existing Foundations (from Code Review)

Before implementing new features, understand what is already in place — this reduces
implementation work and avoids duplication.

### `LoopEntry.workspace` — Already Stored, Not Yet Used for Routing

`crates/ralph-core/src/loop_registry.rs:62`:
```rust
pub workspace: String,
```

Every `LoopEntry` already persists the workspace root path in `.ralph/loops.json`.
`LoopEntry::new()` (line 74) captures `std::env::current_dir()` at creation time.
`list_loops()` in `loops.rs` never reads this field — it uses the registry's own
working directory instead. For global scope, this field IS the cross-project pointer.
No schema migration needed — the field is already populated in all live registries.

### `LoopEntry.started` — Already Stored, Ignored in Display

`crates/ralph-core/src/loop_registry.rs:52`:
```rust
pub started: DateTime<Utc>,
```

`loops.rs:310` has a comment: `age: None, // Registry doesn't track start time` — this
comment is **wrong**. The registry does track start time. The code simply never reads
`entry.started`. Fix: replace `age: None` with
`age: Some(format_age(now.signed_duration_since(entry.started)))`.

### `MergeQueueEntry.queued_at` — Already Used for Age (model for loops)

`loops.rs:337` correctly computes age for merge queue entries:
```rust
let age = Some(format_age(now.signed_duration_since(entry.queued_at)));
```
Apply the same pattern to running loop entries using `entry.started`.

### `failure_reason` in `MergeQueueEntry` — Already Stored

The `MergeQueueEntry` struct has a `failure_reason` field. `loops.rs:329` sets
`has_needs_review = true` when a `NeedsReview` entry is encountered but never displays
the reason. The field must be read and printed inline beneath the entry.

---

## Critical Pre-requisite: `workspace_root` Subdirectory Bug

**This is a correctness bug that must be fixed before implementing project awareness.**
**It affects loops, memories, events, clean, and emit equally.**

### The Problem

Multiple locations resolve workspace root by CWD without traversing to the git root:

| Location | Code | Behavior |
|----------|------|----------|
| `main.rs:356` | `config.core.workspace_root = std::env::current_dir()` | `ralph run` from subdir creates `.ralph/` in subdir |
| `main.rs:1814` | `let workspace_root = std::env::current_dir()` | `ralph clean` from subdir cleans wrong `.ralph/` |
| `memory.rs:217` | `root = args.root.unwrap_or_else(\|\| PathBuf::from("."))` | Memories read/write relative `.` — breaks from any subdir |
| `loops.rs` | `LoopRegistry::new(cwd)` where `cwd = current_dir()` | Loop registry opened in subdir, not project root |

Memory has an additional issue: it uses `PathBuf::from(".")` (a bare relative path),
not even `current_dir()`. Any process that changes directory before calling it would
corrupt the path. The `--root` flag on `MemoryArgs` exists to override this, but
`tools.rs` never passes a resolved root — it forwards args as-is from the CLI.

Git subprocess calls (`git worktree list`, etc.) automatically traverse up to the git
root, so they operate on the correct repo. This creates a **split-brain**: `.ralph/`
is in one location while git worktrees are anchored at another.

### The Fix

Resolve `workspace_root` once at CLI startup by traversing up from CWD to find the
git root. Pass it explicitly into all commands.

```rust
fn find_workspace_root() -> PathBuf {
    // Walk up from CWD looking for .git/ directory
    let mut dir = std::env::current_dir().expect("cannot read CWD");
    loop {
        if dir.join(".git").exists() {
            return dir;
        }
        if !dir.pop() {
            // No .git found — fall back to CWD (non-git project)
            return std::env::current_dir().expect("cannot read CWD");
        }
    }
}
```

**Where to fix:**
- `crates/ralph-cli/src/main.rs` — resolve once at CLI entry, set into
  `config.core.workspace_root` before any dispatch (replaces lines 356 and 1814)
- `crates/ralph-cli/src/loops.rs` — pass workspace root in; remove internal `current_dir()`
- `crates/ralph-cli/src/memory.rs` — populate `args.root` from resolved workspace root
  before calling `execute()`, so `PathBuf::from(".")` fallback is never hit
- `crates/ralph-cli/src/tools.rs` — pass resolved workspace root when calling
  `memory::execute()`, so memory always uses the absolute git-root path

---

## Known Projects Registry

The foundation of global scope. A user-level registry tracking all directories that
have ever run Ralph.

**Location:** `~/.ralph/known-projects.json`

```json
{
  "projects": [
    {
      "path": "/home/bart/projects/myapp",
      "name": "myapp",
      "last_seen": "2026-02-17T14:23:00Z"
    },
    {
      "path": "/home/bart/projects/other-project",
      "name": "other-project",
      "last_seen": "2026-02-16T09:10:00Z"
    }
  ]
}
```

**Written by:** Every `ralph run` invocation registers the workspace root on startup.
The `workspace_root` written is the git-root-resolved path (after subdirectory fix),
not raw CWD.

**Pruned:** Non-existent paths removed on every read. `ralph clean --global` can
selectively remove stale entries.

**Name:** Derived from the last component of `path` by default; overridable via
`ralph.yml` `core.project_name` field.

**Locking:** Uses a file lock on `~/.ralph/known-projects.json.lock` (same pattern
as `LoopRegistry`). All reads AND writes acquire the lock. Never write directly without
the lock — POSIX flock() is advisory and bypassed by direct writes.

New module: `crates/ralph-core/src/known_projects.rs`

---

## Per-Command Design

### `ralph loops` — Local and Global

**Local (default):**
- Fix age for running loops — use `entry.started` (already stored, line 310 comment is wrong)
- Show `failure_reason` inline under `needs-review` entries (field exists, not displayed)
- Worktree LOCATION: full relative path from workspace root, not just final component

```
ID                   STATUS       AGE      LOCATION                  PROMPT
────────────────────────────────────────────────────────────────────────────────
(primary)            running      14m      (in-place)                Implement auth...
loop-abc123          running       3m      .worktrees/loop-abc123    Add cache layer...
loop-def456          needs-review  2h      -                         Fix login bug...
  ↳ Merge failed: conflict in src/auth.rs line 42
```

**Global (`ralph loops -g` / `ralph loops list --global`):**

```
/home/bart/projects/myapp
  (primary)            running      14m      Implement auth feature
  loop-abc123          needs-review  2h      Fix login bug
    ↳ Merge failed: conflict in src/auth.rs

/home/bart/projects/other-project
  loop-def456          running       3m      Refactor database layer

2 projects · 3 loops (2 running, 1 needs-review)
```

For each project in `~/.ralph/known-projects.json`, read its `.ralph/loops.json` and
`.ralph/merge-queue.jsonl` using the same logic as local display. The `entry.workspace`
field identifies which project each entry belongs to — use it to cross-check and route.

**Subcommand global resolution:**
`stop`, `retry`, `diff`, `attach`, `discard`, `merge` all accept a `loop_id`. With
`--global` (or when the ID is not found locally), search all known project registries.

```bash
ralph loops stop loop-abc123           # local first, then global fallback
ralph loops diff loop-abc123 --global  # explicit global search
```

---

### `ralph tools memory` — Local and Global

**Local (default):** Reads/writes `.ralph/agent/memories.md` in CWD project.

**Global (`--global`):**
- `ralph tools memory list --global` — list memories across all projects, grouped by project
- `ralph tools memory search "query" --global` — search memories in all projects
- `ralph tools memory prime --global` — not meaningful (prime is per-iteration, per-project)

```
ralph tools memory list --global

/home/bart/projects/myapp  (12 memories)
  pattern  Always run cargo fmt before committing         [api, rust]
  fix      ECONNREFUSED on :5432 → run docker-compose up [postgres]

/home/bart/projects/other-project  (3 memories)
  decision  Chose JSONL over SQLite: simpler, git-friendly  [arch]
```

---

### `ralph events` — Local and Global

**Local (default):** Reads `.ralph/events-*.jsonl` files in CWD project.

**Global (`ralph events --global`):**
- Aggregates events from all known projects
- Groups output by project
- Most useful for monitoring: `ralph events --global --last 5`

```
ralph events --global --last 5

/home/bart/projects/myapp
  [14:23] loop-abc  build.done     "tests pass"
  [14:21] loop-abc  fix.complete   "auth bug fixed"

/home/bart/projects/other-project
  [14:20] (primary) build.start    "refactoring db"
```

---

### `ralph emit` — Local and Named Project

**Local (default):** Emits to CWD project's current events file.

**Named project (`--project <name-or-path>`):**
```bash
ralph emit "human.guidance" "focus on auth" --project myapp
ralph emit "human.guidance" "focus on auth" --project /home/bart/projects/myapp
```

Resolves the project's current events file from `~/.ralph/known-projects.json`,
writes the event there. This enables CC (running in one directory) to steer a Ralph
loop running in a different project directory.

Global emit (`--global`) does not make sense — emitting the same event to all projects
simultaneously is likely a mistake. `--project <name>` is the explicit targeting mechanism.

---

### `ralph clean` — Local and Global

**Local (default):** Cleans `.ralph/agent/` artifacts in CWD project (existing behavior).

**Global (`ralph clean --global`):**
- Lists all known projects with their artifact sizes
- Prompts for confirmation before cleaning
- `--dry-run` to preview without deleting

```
ralph clean --global --dry-run

Would clean:
  /home/bart/projects/myapp        14 MB  (.ralph/agent/, diagnostics/)
  /home/bart/projects/other-project  2 MB  (.ralph/agent/)

Total: 16 MB across 2 projects
Run without --dry-run to clean.
```

---

## Implementation Order

0. **`find_workspace_root()`** — Fix subdirectory invocation bug. Git-root traversal.
   Used everywhere `current_dir()` is called as workspace root.
1. **`known_projects.rs`** — New core module. Read/write `~/.ralph/known-projects.json`.
   Register workspace root in `ralph run` startup path. Prune stale paths on every read.
2. **`ralph loops list` local fixes** — Age (use `entry.started`), `failure_reason`
   display, full worktree path. No new flags. These are bugs, fix them first.
3. **`ralph loops list --global`** — Aggregated view using known projects.
4. **`ralph loops <subcommand> --global`** — Cross-project ID resolution using
   `entry.workspace` to route stop/retry/diff/attach to the correct project.
5. **`ralph tools memory` global** — `--global` flag on list and search.
6. **`ralph events --global`** — Aggregated event view.
7. **`ralph emit --project`** — Named project targeting.
8. **`ralph clean --global`** — Global artifact cleanup.

---

## Files to Create/Modify

| File | Change |
|------|--------|
| `crates/ralph-core/src/known_projects.rs` | **New.** `KnownProjects` struct. Read/write `~/.ralph/known-projects.json`. Register, list, prune. File-locked. |
| `crates/ralph-core/src/lib.rs` | Export `known_projects` module |
| `crates/ralph-cli/src/main.rs` | Add `find_workspace_root()`. Register workspace root in known projects on `ralph run` start; add `--global` to events, clean; add `--project` to emit |
| `crates/ralph-cli/src/loops.rs` | Fix age (use `entry.started`), show `failure_reason`, full worktree path; add `--global` to `ListArgs`; global list and ID resolution |
| `crates/ralph-core/src/event_loop/mod.rs` | Call `KnownProjects::register(workspace_root)` at loop startup |
| `crates/ralph-cli/src/tools.rs` | Pass resolved workspace root into `memory::execute()` |
| `crates/ralph-cli/src/memory.rs` | Receive workspace root (stop using `PathBuf::from(".")`); add `--global` to list/search subcommands |

---

## Reliability: Locking and Boundary Conditions

This section is critical. The system must be correct when multiple Ralph loops run
simultaneously across different projects on the same machine.

### File Locking — Advisory, Not Enforced

All `flock()` calls in Ralph are POSIX advisory locks. They are bypassed by:
- Code that opens files without calling `flock()` (the smoke test at
  `loop_registry.rs:544-556` demonstrates this directly)
- External tools (`cat`, `jq`, editors) that don't participate in the locking protocol

**Rule:** Every read AND write to `known-projects.json` MUST go through `KnownProjects`
methods that acquire the lock. Never write the file directly, even in tests. Document
this in the module.

### PID Reuse (`is_alive()`)

`loop_registry.rs` uses `kill(pid, 0)` to detect live processes. PID reuse is a known
OS behavior: a crashed loop's PID can be recycled by a new unrelated process, making
`is_alive()` return `true` for a dead loop.

**Mitigation already considered:** The loop ID (`loop-{unix_timestamp}-{4_hex_chars}`)
embeds a timestamp. For global display, cross-check: if `is_alive()` returns true but
the loop's `workspace` path has no `.ralph/loop.lock` claiming that PID, treat it as
stale. Document this check in the global listing code.

**Test requirement:** Add a test that verifies stale detection when a PID from a
dead loop is the same as a live unrelated process (mock `is_alive()` to return true
for a loop where the workspace no longer has a matching lock file).

### `clean_stale()` — Runs on Every Lock Acquisition

`loop_registry.rs` calls `clean_stale()` inside `with_lock()`. This means every
`LoopRegistry` operation (register, deregister, list) also prunes dead entries. This
is correct and intentional. `KnownProjects::register()` should do the same: prune
non-existent paths on every write.

### TOCTOU in Merge Queue

The merge queue is read-then-process: (1) read all entries, (2) display. Between steps,
another loop may append a new entry. This is benign for display (stale reads are
acceptable for `loops list`). Do not add synchronization here — the JSONL append
model is intentionally loose.

### Same-Timestamp Loop IDs Across Projects

Loop IDs embed `unix_timestamp`. Two loops starting in the same second in different
projects get IDs like `loop-1708300800-a1b2` and `loop-1708300800-c3d4`. The 4-hex
suffix provides enough entropy (65536 values) that collisions within one second are
unlikely. For global scope, always qualify a loop ID with its `workspace` path when
routing commands — never assume uniqueness across projects.

**Test requirement:** Add a test for global ID resolution that has two loops with
IDs starting with the same timestamp prefix in different projects, and verifies the
correct one is selected by workspace.

### Cross-Project Interference — Confirmed Absent

Code review confirmed: all state files in production code are **explicitly path-scoped**
to `workspace_root`. There is no global shared mutable state. The only risk is the
subdirectory invocation bug (wrong `workspace_root` value), which is fixed by
`find_workspace_root()`.

---

## Verification

1. `cargo build` — compiles
2. `cargo test` — all pass
3. `ralph run` in a subdirectory of a project → `.ralph/` created at git root, not in
   subdirectory. `~/.ralph/known-projects.json` updated with git root path.
4. `ralph loops list` — running loops show real age (not `-`)
5. `ralph loops list` — `needs-review` entries show failure reason inline
6. `ralph loops list --global` — grouped output across projects
7. `ralph loops stop <id>` — resolves from any project when ID not found locally
8. `ralph tools memory list --global` — grouped across projects
9. `ralph events --global --last 5` — aggregated, grouped
10. `ralph emit "topic" "payload" --project myapp` — targets named project
11. `ralph clean --global --dry-run` — previews without deleting
12. `ralph loops prune` — also prunes known-projects of stale paths
13. Two simultaneous `ralph run` invocations in different project directories — no
    interference in `.ralph/loops.json`, `.ralph/merge-queue.jsonl`, or
    `~/.ralph/known-projects.json`
14. `ralph loops list --global` with a dead loop whose PID is reused — correctly shows
    as stale, not running

## Non-Goals

- Loops/events from other OS users on the same machine
- Remote project awareness (SSH, containers, CI)
- Real-time watch mode (`ralph loops watch`) — separate spec
- Strong isolation guarantees beyond POSIX advisory locking (flock() is sufficient)
