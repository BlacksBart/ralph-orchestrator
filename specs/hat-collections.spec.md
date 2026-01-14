---
status: draft
gap_analysis: 2026-01-14
related:
  - event-loop.spec.md
---

# Hat Collections Specification

## Overview

Hat collections are pre-configured sets of hats (agent personas) designed for specific workflows. This spec defines the contract for valid hat collections, ensuring users cannot create faulty configurations that fail silently or produce undefined behavior.

## Problem Statement

The current implementation allows users to create hat collections that:

1. **Have orphan events** — Hats publish events that no other hat subscribes to, causing the loop to terminate unexpectedly
2. **Have unreachable hats** — Hats that can never be triggered because no event path leads to them
3. **Missing required fields** — Configurations that omit critical fields, leading to runtime failures
4. **Invalid event flow graphs** — Circular dependencies, dead ends, or disconnected subgraphs that cause stuck states
5. **No recovery path** — Custom workflows that cannot recover from blocked states

Users currently receive warnings for some of these issues, but warnings are easily ignored. The system should reject invalid configurations upfront with clear, actionable error messages.

## Goals

1. **Fail fast** — Invalid hat collections are rejected at configuration load time, not during iteration 47
2. **Clear errors** — Every rejection includes what's wrong, why it matters, and how to fix it
3. **Valid by construction** — The type system and validation rules make it hard to create broken collections
4. **Escape hatches** — Advanced users can override validations when they know what they're doing

## Hat Collection Contract

A valid hat collection MUST satisfy all of the following constraints:

### Required Properties

| Property | Requirement |
|----------|-------------|
| **At least one hat** | Collection cannot be empty |
| **Entry point** | At least one hat must trigger on `task.start` or `task.resume` |
| **Exit point** | At least one hat must be capable of emitting the completion promise |
| **Unique triggers** | Each trigger pattern maps to exactly one hat (no ambiguous routing) |
| **Reachable hats** | Every hat must be reachable from the entry point via event publishing |
| **No dead ends** | Every published event (except terminal events) must have a subscriber |

### Hat Definition Schema

Each hat in a collection must conform to:

```yaml
<hat_id>:
  name: string              # Human-readable name (required)
  triggers: [string]        # Events that activate this hat (required, non-empty)
  publishes: [string]       # Events this hat can emit (optional, defaults to [])
  instructions: string      # Custom instructions for this hat (optional)
```

**Note on self-routing:** A hat MAY trigger on events it also publishes (self-routing). This is allowed and is NOT considered "ambiguous routing." Ambiguous routing only occurs when two DIFFERENT hats trigger on the same event. See `event-loop.spec.md` section "Self-Routing Is Allowed" for rationale.

### Terminal Events

Terminal events are events that intentionally have no subscriber. They signal workflow completion or hand-off to external systems.

**Currently implemented:** The following events are hardcoded as terminal:
- `LOOP_COMPLETE` (built-in default)
- The configured `completion_promise` value

**To be implemented:** User-declared terminal events via config:

```yaml
event_loop:
  terminal_events:
    - "deploy.complete"     # Custom terminal event
```

Events listed in `terminal_events` would be exempt from the "no dead ends" validation.

### Recovery Hat Requirement

Collections with more than one hat MUST have a recovery hat that:

1. Subscribes to `task.resume` (to handle loop resumption)
2. Can receive blocked events from other hats (to handle stuck states)

**Currently implemented:**
- The fallback injection logic in `event_loop.rs` hardcodes `planner` as the recovery hat
- No validation exists to verify the recovery hat subscribes to required events

**To be implemented:**
- Configurable recovery hat via `event_loop.recovery_hat`
- Validation that the recovery hat subscribes to `task.resume`

```yaml
event_loop:
  recovery_hat: "coordinator"  # Default: "planner"
```

If no recovery hat exists or the designated hat doesn't subscribe to `task.resume`, the collection should be invalid.

## Validation Rules

### Rule 1: Non-Empty Collection

```
ERROR: Hat collection is empty.

A hat collection must contain at least one hat. Without hats, Ralph has
nothing to execute.

Fix: Add at least one hat to the 'hats:' section of your configuration.
```

### Rule 2: Entry Point Exists

```
ERROR: No hat triggers on 'task.start' or 'task.resume'.

The loop publishes 'task.start' to begin execution. Without a handler,
the loop terminates immediately.

Fix: Add 'task.start' to the triggers of your entry hat:

  hats:
    planner:
      triggers: ["task.start", ...]
```

### Rule 3: Unique Triggers

```
ERROR: Ambiguous routing for trigger 'build.done'.

Both 'planner' and 'reviewer' trigger on 'build.done'. Ralph cannot
determine which hat should handle the event.

Fix: Ensure each trigger maps to exactly one hat:

  planner:
    triggers: ["task.start", "review.done"]  # Remove build.done
  reviewer:
    triggers: ["build.done"]                 # Unique ownership
```

### Rule 4: No Orphan Events

```
ERROR: Event 'deploy.start' published by 'planner' has no subscriber.

This event would be published but never handled, causing the loop to
terminate unexpectedly.

Fix: Either:
  1. Add a hat that triggers on 'deploy.start':

     deployer:
       triggers: ["deploy.start"]

  2. If this event signals completion, add it to terminal_events:

     event_loop:
       terminal_events: ["deploy.start"]

  3. Remove 'deploy.start' from planner's publishes list.
```

### Rule 5: Reachable Hats

```
ERROR: Hat 'auditor' is unreachable from entry point.

No event path leads from 'task.start' to 'auditor'. This hat will never
execute.

Event flow graph:
  task.start → planner → build.task → builder → build.done → planner
                                                           ↳ (no path to auditor)

Fix: Either:
  1. Add an event that routes to 'auditor':

     planner:
       publishes: ["build.task", "audit.request"]
     auditor:
       triggers: ["audit.request"]

  2. Remove 'auditor' if it's not needed.
```

### Rule 6: Recovery Hat Valid

```
ERROR: Recovery hat 'planner' does not subscribe to 'task.resume'.

When Ralph is interrupted and resumed, it publishes 'task.resume'. The
recovery hat must handle this event to continue execution.

Fix: Add 'task.resume' to the recovery hat's triggers:

  planner:
    triggers: ["task.start", "task.resume", ...]
```

### Rule 7: Exit Point Exists

```
ERROR: No hat can terminate the loop.

The completion promise 'LOOP_COMPLETE' can only be emitted by a hat that:
  1. Has termination capability (recovery hat or explicitly designated)
  2. Can verify all work is complete

Currently, no hat meets these criteria.

Fix: Ensure your recovery/coordinator hat can assess completion:

  planner:
    triggers: ["task.start", "task.resume", "build.done"]
    instructions: |
      When all tasks are complete, output: LOOP_COMPLETE
```

## Event Flow Graph Validation

The validator builds a directed graph of event flow and verifies:

```
                     validates
┌──────────────┐  ────────────▶  ┌─────────────────────────┐
│ Hat Collection│                │   Event Flow Graph      │
└──────────────┘                 └─────────────────────────┘
                                          │
                    ┌─────────────────────┼─────────────────────┐
                    ▼                     ▼                     ▼
            ┌─────────────┐      ┌─────────────────┐    ┌─────────────┐
            │ Reachability│      │ Dead End Check  │    │ Entry/Exit  │
            │   (DFS)     │      │ (subscriber map)│    │   Points    │
            └─────────────┘      └─────────────────┘    └─────────────┘
```

### Algorithm

1. **Build adjacency list**: For each hat, map its `publishes` to hats that `trigger` on those events
2. **DFS from entry**: Starting at `task.start`, traverse all reachable hats
3. **Check coverage**: Verify all hats are visited; report unreachable hats
4. **Check dead ends**: For each published event, verify a subscriber exists (or it's terminal)
5. **Verify recovery**: Check recovery hat subscribes to `task.resume` and blocked events

## Configuration Examples

### Valid: Minimal Collection (Single Hat)

```yaml
hats:
  worker:
    name: "Worker"
    triggers: ["task.start", "task.resume"]
    # No publishes - single hat runs to completion
    instructions: |
      Implement the requested feature. When done, output: LOOP_COMPLETE
```

**Why valid:**
- Has entry point (`task.start`)
- Single hat is implicitly the recovery hat
- Single hat can emit completion promise

### Valid: Standard Planner/Builder

```yaml
hats:
  planner:
    name: "Planner"
    triggers: ["task.start", "task.resume", "build.done", "build.blocked"]
    publishes: ["build.task"]

  builder:
    name: "Builder"
    triggers: ["build.task"]
    publishes: ["build.done", "build.blocked"]
```

**Why valid:**
- Entry: `planner` triggers on `task.start`
- Recovery: `planner` triggers on `task.resume` and `build.blocked`
- No dead ends: `build.task` → `builder`, `build.done/blocked` → `planner`
- Both hats reachable: `task.start` → `planner` → `build.task` → `builder`

### Valid: Extended Team with Reviewer

```yaml
hats:
  planner:
    name: "Planner"
    triggers: ["task.start", "task.resume", "build.blocked", "review.approved", "review.rejected"]
    publishes: ["build.task", "review.request"]

  builder:
    name: "Builder"
    triggers: ["build.task"]
    publishes: ["build.done", "build.blocked"]

  reviewer:
    name: "Reviewer"
    triggers: ["build.done", "review.request"]
    publishes: ["review.approved", "review.rejected"]

event_loop:
  completion_promise: "LOOP_COMPLETE"
```

**Why valid:**
- Entry: `planner` → `task.start`
- All hats reachable via event flow
- All published events have subscribers
- `planner` handles blocked events (recovery)

### Invalid: Orphan Event

```yaml
hats:
  planner:
    name: "Planner"
    triggers: ["task.start"]
    publishes: ["build.task", "deploy.start"]  # ← deploy.start has no subscriber

  builder:
    name: "Builder"
    triggers: ["build.task"]
    publishes: ["build.done"]
```

**Error:**
```
ERROR: Event 'deploy.start' published by 'planner' has no subscriber.
```

### Invalid: Unreachable Hat

```yaml
hats:
  planner:
    name: "Planner"
    triggers: ["task.start", "build.done"]
    publishes: ["build.task"]

  builder:
    name: "Builder"
    triggers: ["build.task"]
    publishes: ["build.done"]

  auditor:
    name: "Auditor"
    triggers: ["audit.request"]  # ← No hat publishes audit.request
    publishes: ["audit.done"]
```

**Error:**
```
ERROR: Hat 'auditor' is unreachable from entry point.
```

### Invalid: Ambiguous Routing

```yaml
hats:
  planner:
    name: "Planner"
    triggers: ["task.start", "build.done"]  # ← build.done also in reviewer
    publishes: ["build.task"]

  builder:
    name: "Builder"
    triggers: ["build.task"]
    publishes: ["build.done"]

  reviewer:
    name: "Reviewer"
    triggers: ["build.done"]  # ← Conflicts with planner
    publishes: ["review.done"]
```

**Error:**
```
ERROR: Ambiguous routing for trigger 'build.done'.
Both 'planner' and 'reviewer' trigger on 'build.done'.
```

### Invalid: No Recovery Path

```yaml
hats:
  coordinator:
    name: "Coordinator"
    triggers: ["task.start", "impl.done"]  # ← Missing task.resume
    publishes: ["impl.task"]

  implementer:
    name: "Implementer"
    triggers: ["impl.task"]
    publishes: ["impl.done", "impl.blocked"]  # ← blocked has no handler!
```

**Errors:**
```
ERROR: Recovery hat 'coordinator' does not subscribe to 'task.resume'.

ERROR: Event 'impl.blocked' published by 'implementer' has no subscriber.
```

## Validation Bypass

**To be implemented.** For advanced use cases (testing, experimentation), validation can be bypassed with a single flag:

```yaml
event_loop:
  strict_validation: false  # Downgrade errors to warnings (default: true)
```

**Rationale (YAGNI):** Granular bypass flags like `allow_orphan_events` and `allow_unreachable_hats` add complexity without clear use cases. A single `strict_validation` toggle is sufficient—users who need to bypass validation likely need to bypass all of it.

**Warning when bypassed:**
```
WARN: Hat collection validation bypassed (strict_validation: false).
      Errors downgraded to warnings. This may cause undefined behavior.
```

## Preset Collections

Ralph ships with preset collections in the `presets/` directory:

| Preset | Purpose | Hats | Terminal Events |
|--------|---------|------|-----------------|
| `feature.yml` | Feature development | planner, builder, reviewer | — |
| `feature-minimal.yml` | Feature dev (auto-derived instructions) | planner, builder, reviewer | — |
| `research.yml` | Code exploration (no changes) | researcher, synthesizer | `research.question`, `synthesis.complete` |
| `docs.yml` | Documentation writing | planner, writer, reviewer | — |
| `refactor.yml` | Safe code refactoring | planner, refactorer, verifier | — |
| `debug.yml` | Bug investigation | investigator, tester, fixer, verifier | `hypothesis.confirmed`, `fix.blocked`, `fix.failed` |
| `review.yml` | Code review | reviewer, analyzer | `review.complete` |
| `deploy.yml` | Deployment workflow | planner, builder, deployer, verifier | — |
| `gap-analysis.yml` | Spec vs implementation comparison | analyzer, verifier, reporter | — |

**Note:** Some presets intentionally have "orphan" events that represent workflow completion or hand-off points. These should be declared as `terminal_events` once that feature is implemented. Until then, these presets rely on the completion promise mechanism.

## Acceptance Criteria

### Validation Errors

- **Given** a hat collection with no hats
- **When** configuration is loaded
- **Then** error "Hat collection is empty" is returned

- **Given** a hat collection where no hat triggers on `task.start` or `task.resume`
- **When** configuration is loaded
- **Then** error "No hat triggers on 'task.start'" is returned

- **Given** a hat collection where two hats trigger on the same event
- **When** configuration is loaded
- **Then** error "Ambiguous routing for trigger" is returned with both hat names

- **Given** a hat publishes an event with no subscriber (not in terminal_events)
- **When** configuration is loaded
- **Then** error "Event X published by Y has no subscriber" is returned

- **Given** a hat that cannot be reached from task.start via event flow
- **When** configuration is loaded
- **Then** error "Hat X is unreachable from entry point" is returned

- **Given** a multi-hat collection where recovery hat doesn't subscribe to `task.resume`
- **When** configuration is loaded
- **Then** error "Recovery hat does not subscribe to 'task.resume'" is returned

### Valid Collections

- **Given** a single-hat collection that triggers on `task.start`
- **When** configuration is loaded
- **Then** validation passes (single hat is implicitly recovery hat)

- **Given** a standard planner/builder collection
- **When** configuration is loaded
- **Then** validation passes with no warnings

- **Given** an extended collection with all events properly routed
- **When** configuration is loaded
- **Then** validation passes

### Terminal Events

- **Given** a hat publishes an event listed in `terminal_events`
- **When** orphan event validation runs
- **Then** the event is not flagged as orphan

- **Given** custom terminal events configured
- **When** validation runs
- **Then** only those events (plus built-in defaults) are exempt from orphan check

### Validation Bypass

- **Given** `strict_validation: false` in config
- **When** an invalid collection is loaded
- **Then** errors are downgraded to warnings and loop proceeds

- **Given** validation bypass is active
- **When** loop starts
- **Then** a prominent warning is displayed about potential undefined behavior

### Error Messages

- **Given** any validation error
- **When** error is displayed
- **Then** it includes: what's wrong, why it matters, and how to fix it

- **Given** an unreachable hat error
- **When** error is displayed
- **Then** it includes the event flow graph showing the gap

### Preset Validation

- **Given** any preset in `presets/` directory
- **When** loaded with default settings
- **Then** validation passes with no errors or warnings

## Implementation Notes

### Crate Placement

| Component | Crate |
|-----------|-------|
| `HatCollectionValidator` | `ralph-core` |
| `EventFlowGraph` | `ralph-core` |
| Validation error types | `ralph-core` |
| Preset loading | `ralph-cli` |

### Validation Timing

Validation runs at config load time, before any iteration starts. This ensures:
1. Fast feedback (no wasted iterations)
2. Clear separation between config errors and runtime errors
3. Ability to use `ralph validate` command for CI/CD checks

### Migration Path

Existing configurations that would fail new validation rules:
1. First release: New validations emit warnings only (with deprecation notice)
2. Second release: New validations are errors by default, `strict_validation: false` available

## Implementation Status

This section tracks what's currently implemented vs what this spec proposes.

### Currently Implemented (in `config.rs`)

| Validation | Status | Location |
|------------|--------|----------|
| Non-empty collection | ✅ Implemented | `preflight_check()` |
| Entry point exists (`task.start`/`task.resume`) | ✅ Implemented | `preflight_check()` |
| Unique triggers (no ambiguous routing) | ✅ Implemented | `validate()` |
| No orphan events (published events have subscribers) | ✅ Implemented | `preflight_check()` |
| Terminal event exemption for `LOOP_COMPLETE` and `completion_promise` | ✅ Implemented | `preflight_check()` |

### To Be Implemented

| Feature | Priority | Notes |
|---------|----------|-------|
| Reachability check (DFS from entry point) | High | Catches unreachable hats |
| Recovery hat validation (`task.resume` subscription) | Medium | Currently hardcoded to "planner" |
| Configurable `recovery_hat` field | Medium | Enables custom recovery hats |
| Configurable `terminal_events` list | Medium | Enables custom terminal events |
| `strict_validation` bypass flag | Low | For experimentation |
| Rich error messages with fix suggestions | Low | Improves DX |
| Exit point validation (completion capability) | Low | May not be worth the complexity |

### Config Fields Not Yet in Schema

These fields are specified but not present in `EventLoopConfig`:

```rust
// To be added to EventLoopConfig:
pub terminal_events: Vec<String>,      // Default: []
pub recovery_hat: Option<String>,      // Default: None (uses "planner")
pub strict_validation: bool,           // Default: true
```
