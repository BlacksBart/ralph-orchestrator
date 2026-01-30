---
name: tasks
description: Runtime work tracking via CLI for managing tasks across iterations
---

# Ralph Tasks

Tasks are your **source of truth** for what work exists and its status.
The scratchpad is for thinking; tasks are for tracking.

## Rules
- One task = one testable unit of work (completable in 1-2 iterations)
- Break large features into smaller tasks BEFORE starting implementation
- On your first iteration, check `ralph tools task ready` — prior iterations may have created tasks
- ONLY close tasks after verification (tests pass, build succeeds)

## Commands
```bash
ralph tools task add 'Title' -p 2           # Create (priority 1-5, 1=highest)
ralph tools task add 'X' --blocked-by Y     # With dependency
ralph tools task list                        # All tasks
ralph tools task ready                       # Unblocked tasks only
ralph tools task close <id>                  # Mark complete (ONLY after verification)
```

## First thing every iteration
```bash
ralph tools task ready    # What's open? Pick one. Don't create duplicates.
```
