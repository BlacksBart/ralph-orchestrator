# Phase 4: Shared Corpora Tests — 8 test scenarios

## Context

This is phase 4. Phases 1-3 are complete. The shared memory corpora feature is built and
hardened (tilde expansion via shellexpand, filter applied, doctor check, docs updated).
This phase adds comprehensive test coverage for every real use case.

## What Exists (do NOT modify — only test)

- `MemoriesConfig.shared: Vec<PathBuf>` in `crates/ralph-core/src/config.rs`
- `format_memories_as_markdown_labeled(memories, source)` in `crates/ralph-core/src/memory_store.rs`
- `expand_path()` utility for tilde expansion
- Shared corpus loading in `crates/ralph-core/src/event_loop/mod.rs`
- `--shared <FILE>` flag on `prime` in `crates/ralph-cli/src/memory.rs`
- `MemoriesFilter` applied to both local and shared corpora

## Test File Location

Add tests to `crates/ralph-core/src/memory_store.rs` (unit tests in the existing
`#[cfg(test)] mod tests` block) and to `crates/ralph-cli/tests/` for CLI integration
tests (create `integration_memory_shared.rs` if needed, or add to
`integration_memory.rs` if it exists).

## Test 1: Local + Shared Prime Output

```rust
#[test]
fn test_prime_local_and_shared_corpora() {
    // Setup: create two temp dirs
    // Dir A: local memories.md with a pattern memory
    // Dir B: shared corpus with a decision memory
    // Call prime with --shared pointing to Dir B's file
    // Assert: output contains "# Memories" (unlabeled, local)
    // Assert: output contains "# Memories [corpus-name]" (labeled, shared)
    // Assert: both memories' content appears in output
}
```

## Test 2: Budget Priority — Local Gets Full Budget

```rust
#[test]
fn test_shared_corpora_budget_per_source() {
    // Setup: local store with 50 pattern memories (each ~20 words)
    // Shared corpus with 50 decision memories (each ~20 words)
    // Budget: 100 tokens (400 chars) — enough for ~5 memories per source
    // Call format with budget
    // Assert: local output is truncated with "<!-- truncated:" marker
    // Assert: shared output is independently truncated with "<!-- truncated:" marker
    // Assert: both blocks are present (not just local)
}
```

## Test 3: Missing Shared File — Warning, Not Error

```rust
#[test]
fn test_shared_corpus_missing_file_is_nonfatal() {
    // Setup: local store with one memory
    // Shared path points to nonexistent file
    // Call prime with --shared /nonexistent/path.md
    // Assert: exits Ok(())
    // Assert: local memories still appear in output
    // Assert: stderr contains warning about missing file (capture stderr)
}
```

## Test 4: Symlinked Shared File

```rust
#[test]
fn test_shared_corpus_via_symlink() {
    // Setup: create canonical corpus file with 3 memories
    // Create two project temp dirs, symlink corpus into each as local memories.md
    // Load from both stores
    // Assert: both return identical 3 memories
    // Append a memory via store A
    // Load from store B
    // Assert: store B now has 4 memories (same underlying file)
}
```

## Test 5: Event Loop Injection with Config

```rust
#[test]
fn test_event_loop_injects_shared_corpora() {
    // This tests inject_memories_and_tools_skill() directly or via a test harness.
    // Setup: create a RalphConfig with memories.shared = [path/to/corpus.md]
    // Create corpus file with 2 memories
    // Create local memories with 1 memory
    // Run the injection logic (may need to extract into a testable function)
    // Assert: prefix contains "# Memories" (local, unlabeled)
    // Assert: prefix contains "# Memories [corpus-name]" (shared, labeled)
    // Assert: shared memories content is present
}
```

**Note:** If `inject_memories_and_tools_skill()` is hard to test directly (it's `&self`
on EventLoop), extract the shared corpus loading logic into a standalone function
that can be unit tested.

## Test 6: Tilde Expansion

```rust
#[test]
fn test_expand_path_tilde() {
    // Uses the expand_path() utility
    let home = std::env::var("HOME").unwrap();
    let expanded = expand_path(Path::new("~/test/file.md"));
    assert_eq!(expanded, PathBuf::from(format!("{}/test/file.md", home)));

    // Non-tilde path unchanged
    let unchanged = expand_path(Path::new("/absolute/path.md"));
    assert_eq!(unchanged, PathBuf::from("/absolute/path.md"));

    // Relative path unchanged
    let relative = expand_path(Path::new("relative/path.md"));
    assert_eq!(relative, PathBuf::from("relative/path.md"));
}
```

## Test 7: Two Corpora with Different Labels

```rust
#[test]
fn test_multiple_shared_corpora_labeled_distinctly() {
    // Setup: two separate corpus files
    //   architecture.md — 2 decision memories
    //   conventions.md — 2 pattern memories
    // Prime with --shared for both
    // Assert: output contains "# Memories [architecture]"
    // Assert: output contains "# Memories [conventions]"
    // Assert: both sets of memories appear
    // Assert: sections are in order (local first, then shared in order)
}
```

## Test 8: Duplicate Memory IDs Across Sources

```rust
#[test]
fn test_duplicate_ids_across_local_and_shared_both_appear() {
    // Setup: local store with memory ID "mem-123-abcd"
    // Shared corpus also has a memory with ID "mem-123-abcd" (different content)
    // Prime with --shared
    // Assert: BOTH memories appear in output (no dedup)
    // Assert: they appear under different headers (unlabeled vs labeled)
    // Assert: content from both memories is present
}
```

## Verification

1. `cargo test` — all 8 new tests pass
2. No existing tests break
3. Each test is independent (uses temp dirs, no shared state between tests)
4. Tests clean up after themselves (TempDir drops automatically)
