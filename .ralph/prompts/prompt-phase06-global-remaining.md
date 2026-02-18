# Phase 6: Global Scope — memory, events, emit, clean

## Context

This is phase 6. Phases 1-5 are complete. `KnownProjects` registry works, `ralph loops`
has full global scope. This phase extends `--global` to the remaining stateful commands
and adds `--project` targeting to `ralph emit`.

## Feature 1: `ralph tools memory list/search --global`

### CLI Change

In `crates/ralph-cli/src/memory.rs`, add `--global` / `-g` to `ListArgs` and `SearchArgs`:

```rust
/// List memories from all known projects
#[arg(short = 'g', long)]
pub global: bool,
```

### Behaviour

When `--global`:
1. Load projects from `KnownProjects::new().list()`
2. For each project, construct `MarkdownMemoryStore::with_default_path(&project.path)`
3. Load memories, apply filters, group output by project

### Output Format

```
/home/bart/projects/myapp  (12 memories)
  pattern  Always run cargo fmt before committing         [api, rust]
  fix      ECONNREFUSED on :5432 → run docker-compose up [postgres]

/home/bart/projects/other-project  (3 memories)
  decision  Chose JSONL over SQLite: simpler, git-friendly  [arch]
```

Skip projects where `.ralph/agent/memories.md` doesn't exist.

---

## Feature 2: `ralph events --global`

### CLI Change

In `crates/ralph-cli/src/main.rs`, add `--global` / `-g` to the events command args.

### Behaviour

When `--global`:
1. Load projects from `KnownProjects::new().list()`
2. For each project, read `.ralph/events-*.jsonl` files using the same logic as local
3. Group by project, apply `--last N` per project (not globally)

### Output Format

```
/home/bart/projects/myapp
  [14:23] loop-abc  build.done     "tests pass"
  [14:21] loop-abc  fix.complete   "auth bug fixed"

/home/bart/projects/other-project
  [14:20] (primary) build.start    "refactoring db"
```

---

## Feature 3: `ralph emit --project <name-or-path>`

### CLI Change

In `crates/ralph-cli/src/main.rs`, add `--project` to the emit command args:

```rust
/// Target a specific project (name or path) instead of CWD
#[arg(long)]
pub project: Option<String>,
```

### Behaviour

1. Resolve the project using `KnownProjects::new().find(name_or_path)`
2. Find that project's current events file (read `.ralph/current-events` or derive
   from the event file naming convention)
3. Write the event to that file instead of the local one

### Error Cases

- Project not found: error with list of known project names
- No `--global` on emit (emitting to all projects is a mistake)

---

## Feature 4: `ralph clean --global`

### CLI Change

Add `--global` / `-g` to the clean command args in `main.rs`.

### Behaviour

When `--global`:
1. Load projects from `KnownProjects::new().list()`
2. For each project, calculate artifact size (`.ralph/agent/`, `.ralph/diagnostics/`)
3. Display summary
4. With `--dry-run`: show what would be cleaned without doing it
5. Without `--dry-run`: prompt for confirmation, then clean

### Output Format

```
Would clean:
  /home/bart/projects/myapp        14 MB  (.ralph/agent/, diagnostics/)
  /home/bart/projects/other-project  2 MB  (.ralph/agent/)

Total: 16 MB across 2 projects
Run without --dry-run to clean.
```

When cleaning (not dry-run):
```
Cleaned /home/bart/projects/myapp        14 MB
Cleaned /home/bart/projects/other-project  2 MB
Total: 16 MB cleaned across 2 projects
```

### Safety

- Never clean shared corpora (they live in `~/.ralph/corpora/`, outside project dirs)
- Never clean `known-projects.json`
- Prompt for confirmation unless `--yes` / `-y` is passed

---

## Tests

### Test 1: Global memory list
Register two projects with different memories. Run `memory list --global`.
Assert both projects appear with correct memory counts.

### Test 2: Global events
Register two projects with event files. Run `events --global --last 2`.
Assert both projects appear with events.

### Test 3: Emit to named project
Register a project. Run `emit "test.event" "payload" --project <name>`.
Assert event appears in that project's events file, not the local one.

### Test 4: Global clean dry-run
Register two projects with artifacts. Run `clean --global --dry-run`.
Assert summary shows both projects with sizes, nothing deleted.

---

## Verification

1. `cargo build` — compiles
2. `cargo test` — all pass
3. `ralph tools memory list --global` — grouped across projects
4. `ralph events --global --last 5` — aggregated, grouped
5. `ralph emit "human.guidance" "test" --project myapp` — targets named project
6. `ralph clean --global --dry-run` — previews without deleting
