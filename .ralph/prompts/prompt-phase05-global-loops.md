# Phase 5: Global Scope — `ralph loops list --global` and subcommand routing

## Context

This is phase 5. Phases 1-4 are complete. `find_workspace_root()` is in place,
`KnownProjects` registry exists and is populated on every `ralph run` invocation.
This phase adds cross-project loop visibility and ID resolution.

## Feature 1: `ralph loops list --global` / `ralph loops -g`

### CLI Change

In `crates/ralph-cli/src/loops.rs`, add `--global` / `-g` flag to the list args:

```rust
/// Show loops from all known projects
#[arg(short = 'g', long)]
pub global: bool,
```

Also add `-g` to the top-level `LoopsArgs` or `LoopsCommands` if `ralph loops -g`
(without `list`) should work as a shortcut.

### Behaviour

When `--global` is set:
1. Load all projects from `KnownProjects::new().list()`
2. For each project, read its `.ralph/loops.json` and `.ralph/merge-queue.jsonl`
3. Use the same display logic as local but group output by project path
4. Include a summary line at the bottom

### Output Format

```
/home/bart/projects/myapp
  (primary)            running      14m      Implement auth feature
  loop-abc123          needs-review  2h      Fix login bug
    ↳ Merge failed: conflict in src/auth.rs

/home/bart/projects/other-project
  loop-def456          running       3m      Refactor database layer

2 projects · 3 loops (2 running, 1 needs-review)
```

### Implementation

- Create `list_loops_global()` function alongside existing `list_loops()`
- For each project: construct a `LoopRegistry` at `project.path`, call its `list()`,
  also read the merge queue from `project.path.join(".ralph/merge-queue.jsonl")`
- Use `entry.workspace` field from `LoopEntry` to cross-check project membership
- Skip projects where `.ralph/loops.json` doesn't exist (not all projects have active loops)
- Apply PID reuse detection: if `is_alive()` returns true but the project's
  `.ralph/loop.lock` does not claim that PID, treat the entry as stale

### Error Handling

If reading a specific project's registry fails (permissions, corrupt JSON), log a warning
and skip that project. Do NOT abort the entire global listing.

---

## Feature 2: Cross-Project ID Resolution for Subcommands

### Problem

`stop`, `retry`, `diff`, `attach`, `discard`, `merge` accept a `loop_id`. Currently they
search only the local project. With `--global`, they should search all known projects.

### CLI Change

Add `--global` / `-g` to the subcommand args structs (or to a shared parent):

```rust
/// Search all known projects for the loop ID
#[arg(short = 'g', long)]
pub global: bool,
```

### Behaviour

**Without `--global`:** Search local project only (existing behaviour, unchanged).

**With `--global`:** Search all known projects' registries. When found, execute the
command against that project's workspace root. Print the project path so the user
knows which project was affected:

```
Found loop-abc123 in /home/bart/projects/myapp
Stopping loop-abc123...
```

### Implementation

Add a `resolve_loop_globally()` function:
```rust
fn resolve_loop_globally(loop_id: &str) -> Result<Option<(KnownProject, LoopEntry)>> {
    let projects = KnownProjects::new().list()?;
    for project in &projects {
        let registry = LoopRegistry::new(&project.path);
        if let Ok(entries) = registry.list() {
            if let Some(entry) = entries.iter().find(|e| e.id == loop_id) {
                return Ok(Some((project.clone(), entry.clone())));
            }
        }
    }
    Ok(None)
}
```

For each subcommand that takes a loop_id:
1. If `--global` is set, call `resolve_loop_globally()` first
2. If not `--global`, try local first, then fall back to global (with a note to user)

### Same-Timestamp ID Collision

Two loops in different projects can have IDs starting with the same timestamp prefix.
When multiple matches are found globally:
- If exactly one match, use it
- If multiple matches, list all matches with their project paths and ask the user to
  specify `--project <name>` to disambiguate (future: `--project` flag)

---

## Tests

### Test 1: Global list with multiple projects
Setup two temp dirs as registered projects with different loops. Run global list.
Assert both projects appear grouped, with correct loop counts.

### Test 2: Global list with stale project
Register a project, then delete its directory. Run global list.
Assert the stale project is pruned and does not appear.

### Test 3: Cross-project ID resolution
Register two projects. Add a loop to project B. From project A, resolve the loop ID
with global search. Assert it finds the loop in project B.

### Test 4: Same-timestamp IDs across projects
Two projects each have a loop with the same timestamp prefix but different hex suffix.
Resolve each by full ID. Assert correct routing.

---

## Verification

1. `cargo build` — compiles
2. `cargo test` — all pass
3. `ralph loops list --global` shows all known projects
4. `ralph loops stop <id> --global` finds and stops a loop from any project
5. Stale projects pruned from global listing
