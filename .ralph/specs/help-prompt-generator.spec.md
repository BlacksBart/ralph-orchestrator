# Spec: `ralph help --prompt` — SDLC-Aware Prompt Generator

## Summary

Add a `--prompt [scenario]` flag to `ralph help` that emits tailored system prompts teaching an LLM agent how to leverage Ralph for a specific software development scenario. Without a scenario argument, lists available scenarios. Each prompt is a self-contained instruction block that can be pasted into a CLAUDE.md, used as a system prompt, or piped into a session.

## Motivation

Ralph is invisible to Claude. When a developer opens Claude Code in a Ralph-enabled repo, Claude doesn't know Ralph exists — it doesn't know about hats, presets, events, memories, or the loop lifecycle. It will try to do everything itself: plan, build, review, commit, all in one shot. This is the exact anti-pattern Ralph was designed to prevent.

The gap isn't documentation — it's **situated knowledge**. Claude needs to know:
1. That Ralph exists and what it does
2. Which Ralph workflow fits the current task
3. What commands to run and in what order
4. What to expect (events, iteration, hat handoffs)
5. How to steer/monitor/recover when things go wrong

This knowledge is scenario-dependent. "Fix a bug" and "build a feature from scratch" require completely different Ralph workflows, presets, and mental models. A single generic prompt would be either too vague to be useful or too long to fit in context.

## Design

### Command Interface

```bash
# List available scenarios
ralph help --prompt

# Emit prompt for a specific scenario
ralph help --prompt pdd-to-code-assist
ralph help --prompt bugfix
ralph help --prompt refactor

# Pipe into clipboard
ralph help --prompt pdd-to-code-assist | pbcopy

# Append to CLAUDE.md
ralph help --prompt bugfix >> CLAUDE.md
```

### Scenario Inventory

Scenarios map 1:1 to shipped presets. No invented names — every scenario name IS a preset name (usable as `builtin:<name>` or `ralph init --preset <name>`). Three operational scenarios are added for non-preset Ralph capabilities.

#### Preset Scenarios (1:1 with shipped presets)

| Scenario | Preset | Description (from preset) | Hats |
|----------|--------|---------------------------|------|
| `pdd-to-code-assist` | pdd-to-code-assist.yml | Full PDD + Code-Assist autonomous workflow from idea to commit | inquisitor → architect → design_critic → explorer → planner → task_writer → builder → validator → committer |
| `code-assist` | code-assist.yml | Flexible TDD implementation from any starting point | planner → builder → validator → committer |
| `feature` | feature.yml | Enhanced default workflow with integrated code review | builder → reviewer |
| `bugfix` | bugfix.yml | Scientific method for bug reproduction, fix, and verification | reproducer → fixer → verifier → committer |
| `debug` | debug.yml | Bug investigation and root cause analysis | investigator → tester → fixer → verifier |
| `refactor` | refactor.yml | Safe code refactoring with verification at each step | refactorer → verifier |
| `spec-driven` | spec-driven.yml | Specification-first development pipeline | spec_writer → spec_reviewer → implementer → verifier |
| `review` | review.yml | Code review without modifications | reviewer → analyzer |
| `pr-review` | pr-review.yml | Multi-perspective code review for pull requests | correctness_reviewer → security_reviewer → architecture_reviewer → synthesizer |
| `fresh-eyes` | fresh-eyes.yml | Implementation with enforced repeated fresh-eyes self-review | builder → fresh_eyes_auditor → fresh_eyes_gatekeeper |
| `gap-analysis` | gap-analysis.yml | Deep comparison of specs against implementation | analyzer → verifier → reporter |
| `research` | research.yml | Deep exploration without code changes or commits | researcher → synthesizer |
| `docs` | docs.yml | Documentation writing with writer/editor/reviewer cycle | writer → reviewer |
| `deploy` | deploy.yml | Deployment workflow with validation and rollback support | builder → deployer → verifier |

Excluded from prompt scenarios (infrastructure presets, not user-facing workflows):
- `merge-loop` — internal preset for merge-ralph; users don't invoke directly
- `hatless-baseline` — testing control; no hats, no workflow to teach
- `minimal/*` — backend test configs

#### Operational Scenarios (non-preset Ralph capabilities)

| Scenario | What It Teaches |
|----------|-----------------|
| `steering` | Course-correcting a running loop: TUI guidance (`:` and `!`), `ralph emit "human.guidance"`, Telegram, `ralph loops stop`, `ralph run --continue` |
| `parallel` | Running multiple loops simultaneously: worktree isolation, merge queue, `ralph loops list --all`, how primary/worktree loops coordinate |
| `memories` | Teaching Ralph about your codebase: memory types (pattern, decision, fix, context), `ralph tools memory add/search/prime`, `.ralph/agent/memories.md` |

### Prompt Structure (per scenario)

Each emitted prompt follows a consistent structure:

```markdown
# Using Ralph: <preset-name>

## What This Does
[1-2 sentences: what this workflow accomplishes and why the hat separation matters]

## Quick Start
[The exact commands to run, in order — always using the real preset name]

## Hat Flow
[Hat topology and event flow so the agent understands what's orchestrating]

## Steering & Recovery
[How to monitor, inject guidance, handle failures, resume]

## Anti-Patterns
[What NOT to do — the mistakes agents make without this knowledge]
```

### Prompt Content Guidelines

1. **Prompts are for agents, not humans.** Write in second person imperative ("Run `ralph run`..."), not tutorial prose ("Ralph is a framework that...").
2. **Include exact commands.** No hand-waving. Every step has a runnable command.
3. **Use real preset names everywhere.** `builtin:bugfix`, `ralph init --preset bugfix` — the agent should be able to copy-paste and run.
4. **Explain the *why* behind the hat separation.** Agents that understand motivation make better decisions when things deviate from the happy path.
5. **Keep it under 1500 tokens per prompt.** Agents have limited context. Dense, not comprehensive.
6. **Include the escape hatch.** Every prompt mentions `ralph loops stop` and `ralph run --continue`.
7. **Show the event flow.** E.g., `task.start → repro.complete → fix.complete → verification.passed → LOOP_COMPLETE`. This is the agent's mental model of what's happening.

### Example: `pdd-to-code-assist` Prompt (the end-to-end scenario)

```markdown
# Using Ralph: pdd-to-code-assist

## What This Does
The full autonomous pipeline from rough idea to committed code. Nine specialized hats
handle requirements gathering, architecture, design review, codebase exploration,
planning, task generation, TDD implementation, validation, and committing — in sequence.
No hat does more than one job. This is Ralph's most comprehensive workflow.

## Quick Start
1. `ralph init --preset pdd-to-code-assist` (or use inline: `-c builtin:pdd-to-code-assist`)
2. `ralph run -p "Build a REST API for user management with JWT auth"`
3. Ralph handles everything: requirements Q&A → design → research → plan → tasks → TDD → commit

## Hat Flow
Inquisitor (requirements Q&A) → Architect (design.md) → Design Critic (review) →
Explorer (codebase research) → Planner (implementation plan) → Task Writer (.code-task.md files) →
Builder (TDD: red/green/refactor) → Validator (full test suite) → Committer (atomic commits)

Events: task.start → requirements.complete → design.complete → design.approved →
exploration.complete → plan.complete → tasks.complete → build.done →
verification.passed → LOOP_COMPLETE

## Steering
- Press `:` in TUI to add context ("use Postgres, not SQLite" or "skip the admin endpoints")
- `ralph emit "human.guidance" "the auth module is in src/auth/, follow its patterns"`
- If design is wrong: stop early, edit specs/{task}/design.md, resume with `ralph run --continue`

## Anti-Patterns
- Don't provide an implementation plan in the prompt — let the Inquisitor/Architect discover it
- Don't micro-manage — the 9-hat pipeline has built-in quality gates at every transition
- Don't use this for small fixes — it's heavyweight; use `bugfix` or `code-assist` instead
```

### Example: `bugfix` Prompt

```markdown
# Using Ralph: bugfix

## What This Does
Enforces the scientific method for bug fixing: reproduce with a failing test first,
then fix, then verify. Four hats ensure the Reproducer never fixes, the Fixer never
skips reproduction, and the Verifier catches regressions.

## Quick Start
1. `ralph run -c builtin:bugfix -p "Fix: [describe bug and reproduction steps]"`
2. Ralph creates a failing test, fixes the code, verifies, and commits

## Hat Flow
Reproducer → Fixer → Verifier → Committer

Events: task.start → repro.complete → fix.complete → verification.passed → LOOP_COMPLETE

## Steering
- Press `:` in TUI to add context ("the bug is in the auth middleware, not the route handler")
- `ralph emit "human.guidance" "check error handling in src/api/auth.rs"`
- `ralph loops stop` to pause; `ralph run --continue` to resume

## Anti-Patterns
- Don't write the fix yourself and ask Ralph to "verify it" — let the Reproducer find it
- Don't skip the failing test — it's the proof the bug existed and the regression gate
- Don't provide a multi-bug prompt — one bug per run, always
```

### Example: `steering` Prompt (operational)

```markdown
# Using Ralph: Steering a Running Loop

## What This Does
Ralph loops run autonomously, but you can course-correct mid-flight without
killing and restarting. This is the control plane for a running orchestration.

## Guidance Methods (least to most disruptive)
1. TUI queued: Press `:` → type guidance → queued for next iteration
2. TUI immediate: Press `!` → injected into current iteration
3. Event injection: `ralph emit "human.guidance" "focus on X, skip Y"`
4. Telegram: send message to bot (if RObot configured) for remote steering
5. Graceful stop: `ralph loops stop` → finishes current iteration, then exits

## Monitoring
- TUI shows live agent output, current hat, iteration count
- `ralph loops list` — see all running loops with status
- `ralph loops logs <id> --follow` — tail a specific loop
- `ralph events --last 10` — recent event history

## Recovery
- After `ralph loops stop`: `ralph run --continue` picks up from scratchpad/tasks
- Loop stuck: stop, review `.ralph/agent/scratchpad.md`, add memories, resume
- Wrong direction: stop, refine the prompt, resume

## Anti-Patterns
- Don't kill the process (Ctrl+C twice) — use graceful stop so state is preserved
- Don't edit files while Ralph is running — it will overwrite your changes
- Don't inject guidance every iteration — let the agent work, steer only when off-track
```

### Implementation

#### Data Structure

```rust
struct HelpPrompt {
    /// Scenario name used on CLI — matches preset name where applicable
    name: &'static str,
    /// One-line description for the listing
    summary: &'static str,
    /// The full prompt text (valid markdown, no ANSI)
    body: &'static str,
}
```

All prompts are `const` static data in `main.rs` (same pattern as `TUTORIAL_STEPS` and `HELP_TOPICS`).

#### CLI Integration

Extend the existing `HelpArgs`:

```rust
#[derive(Parser, Debug)]
struct HelpArgs {
    /// Show detailed help with examples and motivation for each tool
    #[arg(short, long)]
    verbose: bool,

    /// Emit a prompt teaching an LLM how to use Ralph for a scenario
    #[arg(short, long)]
    prompt: bool,

    /// Topic (with -v) or scenario (with --prompt)
    topic: Option<String>,
}
```

#### Dispatch Logic

```
ralph help                                → concise command summary (existing)
ralph help -v                             → verbose help topics (existing)
ralph help -v hats                        → single verbose topic (existing)
ralph help --prompt                       → list all 17 scenarios with summaries
ralph help --prompt pdd-to-code-assist    → emit the end-to-end prompt
ralph help --prompt bugfix                → emit the bugfix prompt
ralph help -p steering                    → emit the steering prompt (short flag)
```

#### Output Behavior

- Output is **plain text** (no ANSI colors) when `--prompt <name>` emits a prompt body, since the primary use case is piping to a file or clipboard. Always suppress colors for prompt bodies regardless of `--color` setting or TTY detection.
- When stdout is a TTY, prepend a one-line hint: `# Paste this into your CLAUDE.md or use as a system prompt`
- The listing (`ralph help --prompt` without a name) uses colors normally (follows `--color` setting).
- The prompt body is always valid markdown.

### Files to Modify

| File | Change |
|------|--------|
| `crates/ralph-cli/src/main.rs` | Extend `HelpArgs`, add `HelpPrompt` struct, add `HELP_PROMPTS` const array (17 entries), update `help_command` dispatch, add `print_help_prompts_list` and `print_help_prompt` functions |

### Listing Output

```
$ ralph help --prompt

Available prompts (ralph help --prompt <name>):

  End-to-End:
    pdd-to-code-assist    Full autonomous pipeline from idea to committed code

  Implementation:
    code-assist           Flexible TDD implementation from any starting point
    feature               Build a feature with integrated code review
    spec-driven           Specification-first development pipeline

  Quality:
    review                Code review without modifications
    pr-review             Multi-perspective pull request review
    fresh-eyes            Repeated self-review enforcement (min 3 passes)
    gap-analysis          Deep comparison of specs against implementation

  Fixing & Debugging:
    bugfix                Scientific method: reproduce → fix → verify → commit
    debug                 Hypothesis-driven bug investigation and root cause analysis

  Maintenance:
    refactor              Safe incremental refactoring with verification
    research              Deep exploration without code changes
    docs                  Documentation with writer/reviewer cycle
    deploy                Deployment with validation and rollback

  Operations:
    steering              Course-correct a running Ralph loop
    parallel              Run multiple loops via worktree isolation
    memories              Teach Ralph about your codebase patterns

Usage: ralph help --prompt <name>
       ralph help --prompt <name> | pbcopy
       ralph help --prompt <name> >> CLAUDE.md
```

## Verification

1. `cargo build` — compiles
2. `cargo test` — all pass
3. `ralph help --prompt` — lists all 17 scenarios grouped by category
4. `ralph help --prompt pdd-to-code-assist` — emits the end-to-end prompt
5. `ralph help --prompt bugfix` — emits the bugfix prompt
6. `ralph help --prompt steering` — emits the steering prompt
7. `ralph help --prompt nonexistent` — error with list of available scenarios
8. `ralph help --prompt bugfix | wc -w` — under 1500 words per prompt
9. `ralph help --prompt bugfix | head -1` — starts with `#` (valid markdown, no ANSI)
10. `ralph help --prompt bugfix > /tmp/test.md && grep -c $'\033' /tmp/test.md` — 0 (no escape codes)
11. `ralph help -v` — still works (verbose topics unchanged)
12. `ralph help` — still works (concise summary unchanged)
13. Every preset scenario name works with `ralph run -c builtin:<name>` (names are consistent)

## Non-Goals

- **Dynamic prompt assembly** from the user's ralph.yml — v2 feature. Prompts are static.
- **Prompt for custom presets** — only shipped presets get prompts. Custom presets are self-documenting via their hat instructions.
- **`ralph help --prompt all`** — concatenating 17 prompts would exceed context budgets. Users pick the 1-2 that match their workflow.

## Open Questions

1. **Should operational scenarios (`steering`, `parallel`, `memories`) use the `--prompt` flag or a separate flag?** They're not presets, so they break the 1:1 naming convention. But they're essential for an agent to use Ralph effectively, and a separate flag adds friction. Current design: include them under `--prompt` with a distinct "Operations" group in the listing.

## Future Work: CC-as-Ralph-Expert via CLAUDE.md

### The Relationship Between CC and Ralph

Claude Code and Ralph are not integrated — they have distinct roles:

- **Claude Code (CC)** = general-purpose AI assistant. Runs in a terminal. Handles non-project activities, answers questions, writes one-off code.
- **Ralph** = project orchestrator. Multi-hat, event-driven, autonomous loops. Handles all project work — features, bugs, refactoring, reviews, research.

CC does NOT become Ralph. CC **invokes** Ralph. When CC is running in a project folder with Ralph configured, it should recognize that the user's request is project work and craft the correct `ralph run` command — choosing the right preset, writing the right prompt, and explaining what to expect.

### What `ralph help --prompt` Solves

The `--prompt` feature is the bridge. A user can:

```bash
ralph help --prompt bugfix >> CLAUDE.md
```

Now when they open CC in that project and say "fix the pagination bug," CC knows to run `ralph run -c builtin:bugfix -p "Fix: ..."` instead of trying to fix it directly. CC becomes the expert front-end that translates user intent into the right Ralph workflow.

### What's Missing: A Comprehensive CC Briefing

The per-scenario prompts teach CC about one workflow at a time. What's missing is a **comprehensive briefing** — a single document (or `ralph help --prompt cc-briefing`) that gives CC the full decision tree:

1. **Recognize project work** — User says "add auth" → that's a feature → Ralph handles it
2. **Choose the right preset** — Feature from scratch? `pdd-to-code-assist`. Scoped task? `code-assist`. Bug? `bugfix`. Investigation? `debug`. Review? `pr-review`.
3. **Construct the right command** — Correct `-c builtin:<preset>`, well-written `-p "..."`, appropriate flags (`--max-iterations`, `--continue`)
4. **Explain what to expect** — "Ralph will use 4 hats: Reproducer → Fixer → Verifier → Committer. Watch the TUI for progress."
5. **Know when NOT to use Ralph** — Quick one-liner? CC handles it directly. Question about code? CC reads it. Ralph is for orchestrated multi-step project work.

### Design Questions
- Should this be a single `ralph help --prompt cc-briefing` that emits one comprehensive document? Or should CC's CLAUDE.md be assembled from multiple `--prompt` outputs?
- How large can the briefing be before it hurts CC's context? The per-scenario prompts are ~200 words each. A full decision tree with all 14 presets might be 2000-3000 words — still within budget.
- Should the briefing include the preset selection logic as a decision tree / flowchart that CC can follow?
- Should `ralph help --prompt cc-briefing` be dynamic — reading the project's actual ralph.yml to include custom presets and configuration?
