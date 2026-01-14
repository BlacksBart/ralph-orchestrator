# Idea Honing

Requirements clarification for the resilient, extensible event loop with hat collections.

---

## Q1: What's the core architectural change you're envisioning?

**Answer:**

The shift is from "Ralph wears different hats" to "Ralph delegates to hat-wearing agents":

**Current design (brittle):**
- Planner and Builder are both "Ralph with a hat"
- Users can override/replace these hats
- This breaks the event graph (events published with no subscriber)
- Ralph can "forget" things

**Proposed design (resilient):**
- Single, irreplaceable "hatless Ralph" — the classic Ralph Wiggum technique
- Hatless Ralph is always present as the orchestrator/manager/scrum master
- Additional hats are optional extensions that Ralph can **delegate to**
- Users ADD hats, they don't REPLACE core Ralph
- Ralph coordinates; hats execute

**Key insight:** Ralph becomes the constant, the orchestrator. Hats become his team.

**Evidence from presets:**
- `review.yml`: `reviewer` triggers on `task.start` — no planner, coordination embedded in reviewer
- `feature.yml`: `planner` is just another replaceable hat
- Each preset rebuilds coordination from scratch
- No safety net for orphaned events

**Root cause:** Coordination is embedded in hats, not separated from them.

---

## Q2: How should hatless Ralph work in practice?

**Answer:**

The existing pub/sub event system stays — hats can still trigger other hats directly (e.g., researcher → reviewer). But hatless Ralph is always **the ruler**.

**Mental model: Constitutional Monarchy**
```
                    ┌─────────────────────────┐
                    │   👑 HATLESS RALPH      │
                    │   (The Ruler)           │
                    │   - Always present      │
                    │   - Ultimate authority  │
                    │   - Oversees everything │
                    └───────────┬─────────────┘
                                │ oversees
        ┌───────────────────────┼───────────────────────┐
        ▼                       ▼                       ▼
   ┌─────────┐             ┌─────────┐            ┌─────────┐
   │ Builder │────event───►│ Reviewer│───event───►│ Deployer│
   └─────────┘             └─────────┘            └─────────┘
        ▲                                              │
        └──────────────────event───────────────────────┘
```

- Hats can still communicate directly via pub/sub
- Users define triggers/publishes as before
- BUT: Ralph is always the sovereign — he rules

---

## Q3: What powers does the ruler have?

**Answer:**

| Power | Has It? | Notes |
|-------|---------|-------|
| **Catches orphaned events** | ✅ Yes | Safety net — no dead ends |
| **Owns completion** | ✅ Yes | Only Ralph can output `LOOP_COMPLETE` |
| **Owns the scratchpad** | ✅ Yes | Ralph creates/maintains; hats read/update |
| **Fallback executor** | ✅ Yes | No hats? Ralph does it himself |
| **Veto power** | ❌ No | Direct hat-to-hat invocation bypasses Ralph |
| **Always runs last** | ✅ Yes | Ralph closes every cycle |

**Key constraints:**
- No veto power — direct hat-to-hat pub/sub bypasses Ralph entirely
- Ralph always runs **last** — he's the closer, not the opener
- Ralph **must** output the completion promise
- Ralph **must** output the final event topic signifying loop complete

**Mental model shift:** Ralph isn't intercepting traffic; he's the final checkpoint.

---

## Q4: When does Ralph run?

**Answer: Option B — When no hat is triggered**

```
hat₁ → hat₂ → hat₃ → (no subscriber for event) → 👑 Ralph runs
```

**Tenet alignment:**
- **Tenet 2 (Backpressure Over Prescription):** Ralph doesn't prescribe when to return; he catches what falls through
- **Tenet 5 (Steer With Signals):** "No subscriber" IS the signal that triggers Ralph
- **Tenet 6 (Let Ralph Ralph):** Hats work autonomously; Ralph only steps in when the chain ends

**Why this is least brittle:**
- Orphaned events don't dead-end — they fall through to Ralph
- No prescription for hats to "hand back" (which they might forget)
- Ralph is the universal fallback, not a micromanager
- The safety net is implicit in the architecture, not explicit in instructions

**Key insight:** Ralph subscribes to `*` (everything), but hat subscriptions take priority. Ralph only activates when no hat claims the event.

---

## Q5: What happens when Ralph runs?

**Answer:**

```
Ralph receives unclaimed event (or no event on first run)
    │
    ├─► "Is there a hat that SHOULD handle this?"
    │       │
    │       ├─► YES: Delegate to that hat
    │       │        (dispatch event that triggers the hat)
    │       │
    │       └─► NO: Handle it myself
    │
    ├─► Update scratchpad with status
    │
    └─► "Is all work complete?"
            │
            ├─► YES: Output LOOP_COMPLETE + final event
            │
            └─► NO: Dispatch next priority task (to hat or self)
```

**Key requirement:** Ralph must know what hats are available and what they do — hat topology must be injected into Ralph's context.

**Two modes:**
1. **Delegate** — There's a hat for this, dispatch to it
2. **Do it himself** — No suitable hat, Ralph handles it directly (classic single-agent mode)

---

## Q6: How does Ralph know what hats are available?

**Answer:**

Hat topology is loaded from the YAML config and injected into Ralph's prompt when hats are configured.

**Flow:**
```
ralph.yml (or preset)
    │
    ├─► hats:
    │     builder: { triggers: [...], publishes: [...], ... }
    │     reviewer: { triggers: [...], publishes: [...], ... }
    │
    ▼
Orchestrator reads config
    │
    ▼
Builds hat topology table
    │
    ▼
Injects into Ralph's prompt:

    ## Available Hats

    | Hat | Triggers On | Publishes | Description |
    |-----|-------------|-----------|-------------|
    | builder | `build.task` | `build.done`, `build.blocked` | Implements code |
    | reviewer | `review.request` | `review.approved`, `review.changes_requested` | Reviews code |

    ## To Delegate
    Publish an event that triggers the hat you want.
```

**Key points:**
- Configuration-driven, not dynamic discovery
- Ralph knows exactly what's available based on what user defined
- No hats configured = no table injected = Ralph does everything himself

---

## Q7: What does Ralph's default prompt look like?

**Answer:**

Ralph's prompt should reflect the Ralph Wiggum philosophy:
- Simple, not clever
- Trust iteration over prescription
- Backpressure enforces correctness
- The plan on disk is memory; fresh context is reliability

**Core prompt (always present):**

```markdown
I'm Ralph. Fresh context, fresh start. The scratchpad is my memory.

## ALWAYS
- Read `.agent/scratchpad.md` — it's the plan, it's the state, it's the truth
- Search before assuming — the codebase IS the instruction manual
- Backpressure is law — tests, typecheck, lint must pass
- One task, one commit — keep it atomic

## DONE?
All tasks `[x]` or `[~]`? Output: LOOP_COMPLETE
```

**Conditional injection — Solo mode (no hats):**

```markdown
## SOLO MODE
No team today. I do the work myself.
Pick the highest priority `[ ]` task and get it done.
```

**Conditional injection — Multi-hat mode (hats configured):**

```markdown
## MY TEAM
I've got hats to delegate to. Use them.

| Hat | Triggers On | Publishes | What They Do |
|-----|-------------|-----------|--------------|
| builder | `build.task` | `build.done`, `build.blocked` | Implements code |
| reviewer | `review.request` | `review.approved`, `review.changes_requested` | Reviews code |

To delegate: publish an event that triggers the hat.
If no hat fits: do it myself.
```

**Key changes from previous draft:**
- Simpler, more Ralph-like tone ("I'm Ralph" not "You are Ralph")
- Solo/multi-hat sections are conditional, not always present
- Removed verbose "YOUR JOB" section — Ralph knows what to do
- Trust the iteration, don't over-explain

---

## Q8: How do presets change under this model?

