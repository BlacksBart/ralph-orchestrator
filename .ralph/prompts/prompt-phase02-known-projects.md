# Phase 2: Known Projects Registry — `known_projects.rs`

## Context

This is phase 2 of the Ralph Project Awareness implementation. Phase 1 (foundation bug
fixes) is complete — `find_workspace_root()` exists and is used everywhere. This phase
adds the user-level project registry that enables all global-scope features in later phases.

## What to Build

A new module `crates/ralph-core/src/known_projects.rs` that manages
`~/.ralph/known-projects.json` — a registry of all directories that have ever run Ralph.

### Data Structure

```json
{
  "projects": [
    {
      "path": "/home/bart/projects/myapp",
      "name": "myapp",
      "last_seen": "2026-02-17T14:23:00Z"
    }
  ]
}
```

### API

```rust
pub struct KnownProject {
    pub path: PathBuf,
    pub name: String,
    pub last_seen: DateTime<Utc>,
}

pub struct KnownProjects { /* internal path to ~/.ralph/known-projects.json */ }

impl KnownProjects {
    /// Creates a new registry at the default location (~/.ralph/known-projects.json).
    pub fn new() -> Self;

    /// Registers a project path. Updates `last_seen` if already present.
    /// Prunes non-existent paths before writing.
    pub fn register(&self, workspace_root: &Path) -> Result<()>;

    /// Lists all known projects. Prunes non-existent paths on every read.
    pub fn list(&self) -> Result<Vec<KnownProject>>;

    /// Finds a project by name or path. Name matches last component of path
    /// (case-insensitive).
    pub fn find(&self, name_or_path: &str) -> Result<Option<KnownProject>>;

    /// Removes a project entry by path.
    pub fn remove(&self, path: &Path) -> Result<bool>;
}
```

### File Locking

Use the same advisory `flock()` pattern as `LoopRegistry`. Lock file:
`~/.ralph/known-projects.json.lock`. Every read AND write acquires the lock.
Use `crate::file_lock::FileLock` if available, otherwise implement the same
pattern from `loop_registry.rs`.

### Name Derivation

`name` defaults to the last component of `path` (e.g., `/home/bart/projects/myapp` → `myapp`).
For now, this is the only source. Future: overridable via `ralph.yml` `core.project_name`.

### Pruning

Every `list()` and `register()` call prunes entries where the `path` no longer exists
on disk. Write the pruned result back. This is the same pattern as
`loop_registry.rs::clean_stale()` — prune on every lock acquisition.

### `~/.ralph/` Directory

Create `~/.ralph/` if it doesn't exist on first `register()` call.

## Integration Points

### 1. Register at `ralph run` startup

In `crates/ralph-cli/src/main.rs`, after `config.core.workspace_root` is resolved,
call `KnownProjects::new().register(&config.core.workspace_root)`. This should be
non-fatal — if it fails (e.g., permissions on `~/.ralph/`), log a warning and continue.

### 2. Register in event loop startup

In `crates/ralph-core/src/event_loop/mod.rs`, at the top of the event loop startup
(before iteration begins), call `KnownProjects::new().register(workspace_root)`.
This catches worktree loops that bypass the CLI startup path.

### 3. Export from lib.rs

Add `pub mod known_projects;` and export `KnownProjects` and `KnownProject` from
`crates/ralph-core/src/lib.rs`.

## Tests

1. **Register and list:** Register a temp dir, list, assert it appears with correct name.
2. **Register updates last_seen:** Register same path twice, assert `last_seen` is updated.
3. **Prune removes non-existent:** Register two paths, delete one from disk, list, assert
   only the remaining one is returned.
4. **Find by name:** Register `/tmp/myapp`, find by `"myapp"`, assert found.
5. **Find by path:** Register `/tmp/myapp`, find by `"/tmp/myapp"`, assert found.
6. **Empty registry:** List on fresh registry returns empty vec.
7. **Concurrent register:** Two threads register different paths, both appear in final list.

## Verification

1. `cargo build` — compiles
2. `cargo test` — all pass (including new tests)
3. No existing tests break
4. Manual: `ralph run -p "test" --dry-run` in a project → `~/.ralph/known-projects.json`
   contains the project path
