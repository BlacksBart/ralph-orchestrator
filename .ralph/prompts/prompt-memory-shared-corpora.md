# Ralph Shared Memory Corpora — Implementation Brief

## Problem Statement

Ralph memory is project-scoped. A developer working across multiple related projects
accumulates knowledge in each project's isolated `.ralph/agent/memories.md`. That
knowledge never crosses project boundaries. Every new related project starts from zero.

**Concrete example:** Six architectural patterns discovered in PFM-DAG — auth token
lifetime, DAG retry policy, streaming thresholds — live only in PFM-DAG's memory store.
When a Ralph loop runs in PFM-IP or PFM-ERP, it has no access. It rediscovers, makes
inconsistent choices, or violates constraints it cannot see.

Workarounds (file copy, symlink) are either unmaintainable or collapse the local/shared
distinction entirely.

---

## What Was Built

### Architecture

```
~/.ralph/corpora/
  pfm-system.md             ← shared, machine-local, outside any repo
  company-conventions.md    ← another named corpus

project-A/.ralph/agent/memories.md   ← local, committed to repo
project-A/ralph.yml                  ← references shared corpora

project-B/.ralph/agent/memories.md   ← local, committed to repo
project-B/ralph.yml                  ← references same shared corpora
```

**`ralph.yml` configuration:**
```yaml
memories:
  enabled: true
  shared:
    - ~/.ralph/corpora/pfm-system.md
    - ~/.ralph/corpora/company-conventions.md
```

### Behaviour

| Operation | Behaviour |
|-----------|-----------|
| `ralph run` auto-inject | Local memories first (unlabeled), then each shared corpus as `# Memories [corpus-name]` |
| `ralph tools memory prime` | Local + `--shared <file>` flag, same labeled output |
| `ralph tools memory add` | Always writes to local `.ralph/agent/memories.md` only |
| `ralph clean` | Never touches shared corpora — they live outside the project |
| Budget | Applied per-source (local gets its budget, each shared corpus gets its budget independently) |

### Files Changed

| File | Change |
|------|--------|
| `crates/ralph-core/src/config.rs` | Added `shared: Vec<PathBuf>` to `MemoriesConfig` |
| `crates/ralph-core/src/memory_store.rs` | Added `format_memories_as_markdown_labeled(memories, source)` |
| `crates/ralph-core/src/lib.rs` | Exported `format_memories_as_markdown_labeled` |
| `crates/ralph-core/src/event_loop/mod.rs` | Loads shared corpora after local in `inject_memories_and_tools_skill()` |
| `crates/ralph-cli/src/memory.rs` | Added `--shared <FILE>` to `prime` subcommand |

---

## Known Points of Friction with Base RWO

### 1. `ralph-tools.md` not updated (MUST FIX)
`crates/ralph-core/data/ralph-tools.md` is the agent's runtime skill document — it teaches
every running hat how to use memory. It has no mention of shared corpora, `--shared`, or
the local/shared distinction. An agent today will inject shared memories without knowing
they are read-only or where they came from. Per `CLAUDE.md`: updating this file is
mandatory when memory commands change.

### 2. Budget semantics are per-source, not total
`budget: 2000` with 3 sources can inject up to 6000 tokens. The original intent was a
total cap. Currently undocumented — either fix to be a total cap with proportional
allocation, or explicitly document as per-source.

### 3. `~` path expansion is brittle
Manual `strip_prefix("~")` + `$HOME` lookup. Misses `~username`, bare `~/`, and any
shell edge cases. Should use the `shellexpand` crate. Latent bug on unusual home paths.

### 4. No `ralph doctor` validation of shared paths
Misconfigured `shared` paths fail silently — only a `debug!` log. A user who typos
`~/.ralph/corpora/pfm.md` gets no warning at startup and wonders why shared memories
never appear.

### 5. `MemoriesFilter` not applied to shared corpora in event loop
The CLI `prime` command applies filters (type, tags, recent) before loading shared corpora.
The event loop `inject_memories_and_tools_skill()` does not apply `MemoriesFilter` to
shared corpora — it loads everything. Inconsistency predates this change but is now visible.

### 6. Worktree agent confusion
Worktree loops symlink their `.ralph/agent/memories.md` to the main workspace. Their
"local" IS the main project's memories. With shared corpora, the agent sees two
`# Memories` blocks — one unlabeled (symlinked local) and one labeled. Nothing breaks,
but the agent has no guidance on this in `ralph-tools.md`. It may attempt to `memory add`
to the labeled corpus (which it cannot — writes always go to local).

---

## Folder Structure Patterns

### Correct: Machine-local corpus outside all repos
```
~/.ralph/corpora/
  pfm-architecture.md       ← system topology, service contracts
  pfm-auth.md               ← auth domain: tokens, sessions, expiry
  rust-conventions.md       ← language-level, applies to any Rust project
```
- Never committed to any repo
- Outlives any single project
- `ralph clean` never touches it
- New project subscribes with one line in `ralph.yml`

### Correct: One corpus per concern boundary
Each file covers one domain. Projects include only relevant corpora.
`pfm-ip` subscribes to `pfm-architecture` and `pfm-auth`.
A standalone Rust project subscribes only to `rust-conventions`.
Nothing subscribes to everything.

### Correct: Team corpus as a git repo
```
~/projects/team-memories/   ← standalone git repo, no .ralph/, just .md files
  architecture.md
  conventions.md
  fixes.md
```
```yaml
memories:
  shared:
    - ~/projects/team-memories/architecture.md
```
Pull to get teammates' discoveries. Push to share yours. No Ralph infrastructure needed —
it's just a markdown file in a repo.

---

## Folder Structure Anti-Patterns

### Anti-pattern: Symlink corpus AS local memories.md
```bash
# WRONG
ln -sf ~/.ralph/corpora/pfm-system.md .ralph/agent/memories.md
```
Collapses local/shared distinction. Every `memory add` writes into the shared corpus,
polluting it with project-specific knowledge. No local store remains. Use `memories.shared`
in `ralph.yml` instead — it preserves both.

### Anti-pattern: Corpus inside a project repo with relative path
```yaml
# project-B/ralph.yml — WRONG
memories:
  shared:
    - ../project-A/shared-memories.md
```
Hardcodes the assumption that project-A is a sibling directory. Breaks for anyone who
clones project-B alone, or who has a different directory layout. Corpus belongs in
`~/.ralph/corpora/`, not inside any project.

### Anti-pattern: One monolithic corpus for everything
```
~/.ralph/corpora/everything.md   ← 500 memories, all domains, all projects
```
Injected in full into every project. Consumes the entire memory budget with mostly
irrelevant entries. Defeats the budget mechanism. Keep corpora narrow: everything in a
corpus should be relevant to any project that subscribes to it.

### Anti-pattern: Using `--global` (future) instead of shared corpora
`ralph tools memory list --global` will read ALL known projects' memories. That includes
unrelated projects. A PFM project should not have a Node.js side-project's memories
injected into it. Global is for inspection; shared corpora are for curated cross-project
knowledge.

---

## Tests Not Yet Written

The following test scenarios cover the real use cases and should be added to
`crates/ralph-core/src/memory_store.rs` and `crates/ralph-cli/tests/`:

1. **Local + shared prime**: setup local `memories.md` + shared corpus, assert `prime`
   output contains both `# Memories` (unlabeled) and `# Memories [corpus-name]` (labeled).
2. **Budget priority — local first**: local 50 memories + shared 50, budget=500 tokens,
   assert local memories present, shared truncated if budget exceeded, never reversed.
3. **Missing shared file**: `prime --shared /nonexistent.md` exits `Ok(())`, prints
   warning to stderr, does not error.
4. **Symlinked shared file (Option 3)**: shared.md symlinked as two different projects'
   local memories.md, assert both stores load same content, write from one appears in both.
5. **Event loop injection with config**: `MemoriesConfig.shared = [path]`, run
   `inject_memories_and_tools_skill()`, assert prefix contains labeled corpus block.
6. **Tilde expansion**: `--shared ~/.ralph/corpora/test.md` resolves to
   `$HOME/.ralph/corpora/test.md`, not literal `~`.
7. **Two corpora with different labels**: assert both labeled sections appear in output
   with distinct source names.
8. **Duplicate IDs across local and shared**: local and shared both have `mem-123-abcd`,
   assert both appear (no silent dedup), assert labels distinguish source.

---

## Immediate Next Steps

1. **Update `ralph-tools.md`** — add shared corpus section to memory commands, explain
   local vs. shared distinction, explain label in injected prompt. This is MANDATORY per
   CLAUDE.md before this feature is usable by agents.

2. **Fix tilde expansion** — use `shellexpand` crate or equivalent to handle all
   shell path forms. One-line fix in both `event_loop/mod.rs` and `memory.rs`.

3. **Add `ralph doctor` check** — warn on startup if any `memories.shared` path does
   not exist. Non-fatal, but visible.

4. **Write the 8 tests above** — especially #3 (missing file), #4 (symlink), and #6
   (tilde expansion). These are the scenarios most likely to break silently.

5. **Clarify budget semantics** — decide: per-source cap (current) or total cap. Document
   whichever is chosen in both `config.rs` and `ralph-tools.md`.

---

## How to Use This Feature Today

```bash
# 1. Create the corpus directory
mkdir -p ~/.ralph/corpora

# 2. Move or create your shared memories file
# Option A: move existing memories from a project
mv ~/projects/PFM-DAG/.ralph/agent/memories.md ~/.ralph/corpora/pfm-system.md
# Then in PFM-DAG's ralph.yml, add memories.shared (see below)

# Option B: start fresh
touch ~/.ralph/corpora/pfm-system.md
ralph tools memory add "content" -t pattern --root ~/.ralph/corpora/

# 3. Subscribe each project in ralph.yml
# memories:
#   enabled: true
#   shared:
#     - ~/.ralph/corpora/pfm-system.md

# 4. Verify injection
ralph tools memory prime --shared ~/.ralph/corpora/pfm-system.md

# 5. Add to shared corpus explicitly
ralph tools memory add "PFM auth tokens expire after 15m" \
  -t context --tags pfm,auth \
  --root ~/.ralph/corpora/
```
