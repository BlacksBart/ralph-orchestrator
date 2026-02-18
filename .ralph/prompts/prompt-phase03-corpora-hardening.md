# Phase 3: Shared Corpora Hardening — tilde, filter, doctor, budget, docs

## Context

This is phase 3. Phase 1 (bug fixes) and Phase 2 (known-projects registry) are complete.
The shared memory corpora feature was added to the codebase (config.rs `memories.shared`,
event_loop injection, CLI `prime --shared`). This phase fixes known issues and updates
documentation so the feature is production-ready.

## What Exists (already built, do NOT rebuild)

- `MemoriesConfig.shared: Vec<PathBuf>` in `config.rs`
- `format_memories_as_markdown_labeled(memories, source)` in `memory_store.rs`
- Shared corpus loading loop in `event_loop/mod.rs::inject_memories_and_tools_skill()`
- `--shared <FILE>` flag on `prime` in `memory.rs`

## Fix 1: Tilde Expansion — use `shellexpand` crate

### Problem
Both `event_loop/mod.rs` and `memory.rs` manually do `strip_prefix("~")` + `$HOME` env
lookup. This misses `~username`, handles edge cases poorly, and is duplicated code.

### Fix
1. Add `shellexpand` to `Cargo.toml` for `ralph-core` and `ralph-cli` (or just `ralph-core`
   if the CLI can use it through re-export).
2. Create a utility function (in `ralph-core`, perhaps in a `paths.rs` or `text.rs` module):
   ```rust
   pub fn expand_path(path: &Path) -> PathBuf {
       let s = path.to_string_lossy();
       let expanded = shellexpand::tilde(&s);
       PathBuf::from(expanded.as_ref())
   }
   ```
3. Replace the manual tilde expansion in both `event_loop/mod.rs` and `memory.rs`
   `prime_command` with calls to this function.

---

## Fix 2: Apply `MemoriesFilter` to shared corpora in event loop

### Problem
The event loop `inject_memories_and_tools_skill()` loads shared corpora without applying
the `MemoriesFilter` (types, tags, recent). The CLI `prime` command does apply filters.
This creates inconsistency.

### Fix
After loading shared memories in the event loop's shared corpus loop, apply the same
filter logic that the local memories would get. Extract the filter logic into a helper
if needed:

```rust
fn apply_filter(memories: &mut Vec<Memory>, filter: &MemoriesFilter) {
    if !filter.types.is_empty() {
        let types: Vec<MemoryType> = filter.types.iter()
            .filter_map(|s| s.parse().ok()).collect();
        memories.retain(|m| types.contains(&m.memory_type));
    }
    if !filter.tags.is_empty() {
        memories.retain(|m| m.has_any_tag(&filter.tags));
    }
    if filter.recent > 0 {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(i64::from(filter.recent));
        let cutoff_str = cutoff.format("%Y-%m-%d").to_string();
        memories.retain(|m| m.created >= cutoff_str);
    }
}
```

Apply to both local and shared memories before formatting.

---

## Fix 3: `ralph doctor` check for shared memory paths

### Problem
Misconfigured `memories.shared` paths fail silently — only a `debug!` log. Users get
no visible warning.

### Fix
In `crates/ralph-cli/src/doctor.rs`, add a check in the existing diagnostics flow:
- For each path in `config.memories.shared`, verify it exists.
- If missing, emit a warning (non-fatal): `"Shared memory corpus not found: {path}"`
- Use the same expand_path() utility for tilde expansion before checking existence.

---

## Fix 4: Document budget semantics

### Problem
`budget: 2000` with 3 sources currently allows up to 6000 tokens (per-source cap).

### Decision
Document this as **per-source cap** (the current behavior). Total-cap with proportional
allocation is more complex and can be added later if needed. The per-source behavior is
actually reasonable — it ensures each source gets a chance to contribute without being
starved by a large local store.

### Fix
1. In `config.rs`, update the doc comment on `budget` in `MemoriesConfig`:
   ```
   /// Maximum tokens to inject per memory source (0 = unlimited).
   /// Applied independently to local memories and each shared corpus.
   ```
2. In `ralph-tools.md`, add a note in the memory section about budget per-source behavior.

---

## Fix 5: Update `ralph-tools.md` (MANDATORY)

### Problem
`crates/ralph-core/data/ralph-tools.md` is the agent's runtime skill document. It has
no mention of shared corpora. Agents will inject shared memories without understanding
what the labeled `# Memories [corpus-name]` blocks mean.

### Fix
Add a new section after "Memory Best Practices" in `ralph-tools.md`:

```markdown
### Shared Memory Corpora

Projects can include shared memory corpora — read-only markdown files that contain
cross-project knowledge (architectural decisions, team conventions, domain patterns).

**Configuration in ralph.yml:**
```yaml
memories:
  shared:
    - ~/.ralph/corpora/pfm-system.md
    - ~/.ralph/corpora/team-conventions.md
```

**What you'll see in your context:**
- `# Memories` — your project's local memories (you can read and write these)
- `# Memories [pfm-system]` — a shared corpus (read-only, do NOT try to modify)

**Rules:**
- `ralph tools memory add` always writes to LOCAL memories only
- Shared corpora appear with `[source-name]` labels — treat them as authoritative context
- If a shared memory contradicts a local memory, the local memory takes precedence
- Budget is applied per-source — each corpus gets its own budget allocation

**Worktree loops:** Your local memories are symlinked from the main workspace. Shared
corpora are resolved from `ralph.yml` config. Both are available in every iteration.
```

Also add `--shared <FILE>` to the `prime` command listing:
```bash
ralph tools memory prime --budget 2000 --shared ~/.ralph/corpora/pfm-system.md
```

---

## Verification

1. `cargo build` — compiles (shellexpand dependency resolves)
2. `cargo test` — all pass
3. `ralph doctor` in a project with misconfigured `memories.shared` path — shows warning
4. Tilde paths in `ralph.yml` `memories.shared` resolve correctly
5. `ralph-tools.md` contains the shared corpora section
6. Budget doc comment in `config.rs` is accurate
