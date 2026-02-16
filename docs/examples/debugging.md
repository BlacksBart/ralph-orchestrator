# Debugging Example

!!! note "Documentation In Progress"
    This page is under development. Check back soon for a complete debugging workflow example.

## Overview

This example demonstrates using Ralph to debug issues, with specialized hats for investigation, hypothesis testing, and fix verification.

## Enabling Diagnostics

```bash
RALPH_DIAGNOSTICS=1 ralph run -p "fix the authentication bug"
```

## Reviewing Logs

```bash
# View all agent output
jq 'select(.type == "text")' .ralph/diagnostics/*/agent-output.jsonl

# View hat selection decisions
jq 'select(.event.type == "hat_selected")' .ralph/diagnostics/*/orchestration.jsonl

# View errors
jq '.' .ralph/diagnostics/*/errors.jsonl
```

## See Also

- [Diagnostics](../advanced/diagnostics.md) - Full diagnostics reference
- [Troubleshooting](../reference/troubleshooting.md) - Common issues
- [Simple Task](simple-task.md) - Basic example
