---
status: draft
gap_analysis: null
related:
  - event-loop.spec.md
---

# Interactive Mode

## Overview

Ralph supports two execution modes for running agent CLIs:

1. **Autonomous (default)**: Ralph drives the agent headlessly. The agent runs with non-interactive flags, and user input is not forwarded. Output is piped and parsed for events.

2. **Interactive (`--interactive` / `-i`)**: The user drives the agent through Ralph. The agent runs in a PTY with full TUI support, user input is forwarded, and the agent can prompt for confirmation.

## Motivation

The previous design had three modes (`headless`, `--pty`, `--pty --observe`) which created confusion:

- `--pty` enabled terminal emulation but still passed `--no-interactive` to agents
- `--observe` allowed watching output without interaction (limited utility)
- The mental model was unclear: "PTY" is an implementation detail, not a user intent

The new design collapses PTY and interactivity into a single flag that expresses user intent: "I want to interact with the agent."

## Behavior

### Autonomous Mode (Default)

```bash
ralph run
ralph loop
```

- Agent CLI invoked with non-interactive flags (e.g., `--no-interactive`, `--full-auto`)
- Standard I/O piped, not PTY
- Output parsed for events (ANSI stripped)
- User input NOT forwarded to agent
- Ctrl+C terminates Ralph immediately
- Suitable for: CI, automation, background loops

### Interactive Mode

```bash
ralph run --interactive
ralph run -i
ralph loop -i
```

- Agent CLI invoked WITHOUT non-interactive flags
- Process spawned in PTY for full TUI support (colors, spinners, prompts)
- Output displayed in real-time with ANSI preserved
- User input forwarded to agent (user can respond to prompts)
- Ctrl+C forwarded to agent (agent handles interruption)
- Double Ctrl+C within 1 second: Ralph terminates agent (safety fallback)
- Ctrl+\: Ralph force-kills agent immediately
- Suitable for: Development, debugging, manual oversight

## CLI Backend Configuration

Each backend defines two argument sets:

| Backend | Autonomous Args | Interactive Args |
|---------|-----------------|------------------|
| claude | `--dangerously-skip-permissions` | (none, or `--dangerously-skip-permissions` if user wants auto-approve) |
| kiro | `--no-interactive --trust-all-tools` | `--trust-all-tools` |
| codex | `exec --full-auto` | `exec` |
| amp | `--dangerously-allow-all` | (none) |

The backend receives the execution mode and constructs arguments accordingly.

## Configuration

```toml
[cli]
# Default mode when --interactive not specified
# Options: "autonomous" (default), "interactive"
default_mode = "autonomous"

# Idle timeout in seconds (interactive mode only, 0 = disabled)
idle_timeout_secs = 30
```

## Signal Handling

### Autonomous Mode

| Signal | Behavior |
|--------|----------|
| Ctrl+C | Terminate Ralph and agent immediately |
| Ctrl+\ | Terminate Ralph and agent immediately |

### Interactive Mode

| Signal | Behavior |
|--------|----------|
| Ctrl+C (1st) | Forward to agent, start 1-second window |
| Ctrl+C (2nd within 1s) | Ralph sends SIGTERM to agent |
| Ctrl+\ | Ralph sends SIGKILL to agent |
| Idle timeout | Ralph sends SIGTERM, then SIGKILL after 5s grace |

## Removed Features

- `--pty` flag: Replaced by `--interactive`
- `--observe` flag: Removed (no clear use case)
- `--no-pty` flag: Removed (autonomous mode is the default)
- `pty_mode` config option: Replaced by `default_mode`
- `pty_interactive` config option: Removed

## Migration

Users with existing configuration:

| Old Config | New Config |
|------------|------------|
| `pty_mode = false` | `default_mode = "autonomous"` (or remove, it's default) |
| `pty_mode = true, pty_interactive = true` | `default_mode = "interactive"` |
| `pty_mode = true, pty_interactive = false` | `default_mode = "autonomous"` (observe mode removed) |

## Acceptance Criteria

- [ ] `ralph run` executes agent in autonomous mode (headless, `--no-interactive` flags)
- [ ] `ralph run -i` executes agent in interactive mode (PTY, no `--no-interactive` flags)
- [ ] User input is forwarded to agent only in interactive mode
- [ ] Agent TUI renders correctly in interactive mode (colors, spinners)
- [ ] Ctrl+C in interactive mode is forwarded to agent on first press
- [ ] Double Ctrl+C in interactive mode terminates agent via Ralph
- [ ] Ctrl+\ in interactive mode force-kills agent
- [ ] `--pty` and `--observe` flags are removed
- [ ] Config migration documented in changelog
