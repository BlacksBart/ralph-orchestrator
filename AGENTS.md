# AGENTS.md

## The Ralph Tenets

> **Core Insight:** The orchestrator is a thin coordination layer, not a platform. Agents are smart; let them do the work.

These tenets guide all design and implementation decisions for Ralph Orchestrator. When in doubt, return to these principles.

---

### 1. Fresh Context Is Your Reliability

Each iteration starts with a cleared context window. The agent re-reads persistent truth (specs, plan, code) every cycle. Forgetting everything paradoxically makes agents *more* reliable—it prevents information drift and compounding hallucinations. We optimize for the "smart zone" (40-60% of usable context) by keeping tasks atomic and exiting after each one.

### 2. Backpressure Over Prescription

Don't prescribe how—create gates that reject bad work. Tests, typechecks, builds, and lints provide immediate, objective feedback. For subjective criteria (tone, aesthetics, UX feel), use LLM-as-judge with binary pass/fail. Backpressure is enforceable; instructions are suggestions.

### 3. The Plan Is Disposable

Plans can be regenerated when wrong or stale. Regeneration costs one planning loop—cheap compared to an agent spiraling on a bad plan. We pursue eventual consistency through iteration, not upfront perfection. Never fight to save a plan; throw it away and start fresh.

### 4. Disk Is State, Git Is Memory

The filesystem persists state between otherwise isolated loop executions. Git history provides memory across context windows. No sophisticated coordination needed—the plan file on disk (`.agent/scratchpad.md`) is the handoff mechanism. The agent reads updated state each iteration and decides what to do next.

### 5. Steer With Signals, Not Scripts

Guide through discoverable artifacts: prompt guardrails, `AGENTS.md` learnings, existing code patterns. The codebase *is* the instruction manual. When Ralph fails a specific way, add a sign for next time. Don't over-specify; let the agent learn from what exists.

### 6. Let Ralph Ralph

Trust agent self-correction and self-prioritization across iterations. Human role: observe failures, add guardrails reactively. Tune like a guitar, don't conduct like an orchestra. Move *outside* the loop, not inside it. Ralph picks "most important"—that's the design.

---

### Anti-Patterns

- ❌ Building features into the orchestrator that agents can handle
- ❌ Complex retry logic (fresh context handles recovery)
- ❌ Detailed step-by-step instructions (use backpressure instead)
- ❌ Trying to maintain state across iterations (use disk)
- ❌ Fighting nondeterminism (embrace iteration)
- ❌ Scoping work at task selection time (scope at plan creation instead)

## Specs

- Create all specs in `<repo-root>/specs/`
- **DO NOT implement any feature or component without an approved spec in ./specs first.**
- Work step-by-step: spec → review → **dogfood spec** → implement → **dogfood implementation** → done.
- Do NOT add code examples to any specs

### Spec Frontmatter

Every spec must include YAML frontmatter tracking its lifecycle. If a spec is missing frontmatter, add it before making other changes.

```yaml
---
status: draft | review | approved | implemented | deprecated
gap_analysis: YYYY-MM-DD
related:
  - other-spec.spec.md
---
```

| Field | Description |
|-------|-------------|
| `status` | Current lifecycle stage (see below) |
| `gap_analysis` | Date of last gap analysis between spec and implementation |
| `related` | List of related spec filenames (optional) |

**Status values:**

| Status | Meaning |
|--------|---------|
| `draft` | Work in progress, not ready for review |
| `review` | Ready for dogfooding and feedback |
| `approved` | Dogfooded, approved for implementation |
| `implemented` | Implementation complete and verified |
| `deprecated` | No longer active, kept for reference |

**Gap analysis:** After implementation, periodically compare the spec against actual behavior. Update `gap_analysis` date when verified. If gaps are found, either update the spec or fix the implementation.

### Dogfooding: Two Gates

Dogfooding happens twice—once for the spec, once for the implementation.

#### Gate 1: Dogfood the Spec (Before Implementation)

**Walk through the spec as a human would.**

1. Read it start to finish as if you're seeing it for the first time
2. Mentally (or actually) execute each flow—happy path, error cases, edge cases
3. Ask: "If I followed these instructions exactly, would I get the expected result?"
4. Ask: "What's missing? What's ambiguous? What would confuse me?"

**YAGNI/KISS check:**
- Is every feature in this spec actually needed right now?
- Could this be simpler and still solve the problem?
- Are we building for hypothetical future requirements?
- What can we cut and still ship something useful?

**Signs a spec needs more work:**
- You have to make assumptions not stated in the spec
- Acceptance criteria don't cover the flows described
- You can imagine a scenario where the spec contradicts itself
- A reasonable person could interpret it two different ways
- **The spec solves problems nobody has yet**
- **There's a simpler approach that would work**

**The bar:** A new team member should be able to implement the feature using only the spec and the codebase. If they'd need to ask clarifying questions, the spec isn't done. If they'd ask "why do we need this part?"—cut it.

#### Gate 2: Dogfood the Implementation (Before Done)

**Use the feature as a human would.**

1. Run through every acceptance criterion manually—don't assume, verify
2. Try the happy path end-to-end
3. Try to break it—edge cases, bad input, unexpected sequences
4. Ask: "Does this behave as the spec describes?"
5. Ask: "Would a user be confused or frustrated by this?"

**YAGNI/KISS check:**
- Did we build exactly what the spec asked for, nothing more?
- Is there dead code, unused parameters, or over-abstraction?
- Are there config options nobody will use?
- Could a junior dev understand this code in 5 minutes?

**Signs implementation needs more work:**
- An acceptance criterion passes technically but feels wrong
- You found a flow the spec didn't cover (update spec AND fix)
- Error messages are unhelpful or missing
- The feature works but is awkward to use
- **There's code "for future extensibility" that isn't needed now**
- **You built a framework when a function would do**

**The bar:** You should be able to demo this to someone and feel confident it works. If you'd caveat with "well, don't do X or it breaks," it's not done. If you'd say "we might need this later"—delete it.

## Meta Prompt (instructions.rs)

The meta prompt is defined in `crates/ralph-core/src/instructions.rs`. This file contains `InstructionBuilder` which prepends orchestration context to agent prompts.

### Updating the Meta Prompt

When modifying `instructions.rs`:
- Keep logs lighthearted: "I'm Ralph. I've got my planner hat on."
- Run tests after changes
- Test manually with a real agent loop to verify behavior

## Running kiro-cli in Headless Mode

Kiro CLI is AWS's agentic coding assistant for the terminal. To run it in fully headless mode (no user interaction):

```bash
kiro-cli chat --no-interactive --trust-all-tools "Your prompt here"
```
### Configuration Paths

| Purpose | Path |
|---------|------|
| MCP Servers | `~/.kiro/settings/mcp.json` |
| Prompts | `~/.kiro/prompts` |
| Project Config | `.kiro/` (in project root) |
| Global Config | `~/.kiro/` |
| Logs | `$TMPDIR/kiro-log` |


# IMPORTANT
- You MUST always run ralph loops in a temp directory so you dont pollute the workspace
