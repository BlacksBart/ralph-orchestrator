---
status: review
gap_analysis: null
related:
  - event-loop.spec.md
  - cli-adapters.spec.md
  - adapters/kiro.spec.md
  - adapters/claude.spec.md
---

# Interactive Mode

## Overview

Ralph supports two execution modes:

| Mode | Flag | Who Drives | CLI Flags |
|------|------|-----------|-----------|
| **Autonomous** (default) | (none) | Ralph orchestrates | Non-interactive (`--no-interactive`, etc.) |
| **Interactive** | `-i` / `--interactive` | User interacts | Interactive (no headless flags) |

## Problem Statement

The previous design had three execution modes:

| Old Flags | What It Did | Problem |
|-----------|-------------|---------|
| (none) | Headless, piped I/O | ✅ Works |
| `--pty` | PTY mode, but still passed `--no-interactive` to agents | ❌ Broken: users could see TUI but agents ignored their input |
| `--pty --observe` | PTY mode, output-only | ❌ Limited utility |

The core issue: "PTY" is an implementation detail. Users don't care about pseudo-terminals—they care about whether they can interact with the agent.

Additionally, the `--no-interactive` flag was always passed to agents (like kiro-cli), even in PTY mode. This meant:
- User presses Ctrl+C → forwarded to kiro-cli
- kiro-cli (in non-interactive mode) exits immediately
- Ralph's double Ctrl+C logic never triggers

## Solution

Collapse PTY and interactivity into a single flag that expresses user intent:

```bash
ralph run              # Autonomous: Ralph drives, agent runs headless
ralph run -i           # Interactive: User drives, agent runs with TUI
ralph run --interactive
```

The `--interactive` flag changes two things:
1. **Execution method**: Spawns agent in PTY (for TUI support)
2. **Agent flags**: Omits non-interactive flags (agent can prompt user)

## Configuration

### ralph.yml

```yaml
cli:
  backend: "kiro"

  # Execution mode when --interactive not specified
  # Values: "autonomous" (default), "interactive"
  default_mode: "autonomous"

  # Idle timeout in seconds (interactive mode only, 0 = disabled)
  idle_timeout_secs: 30
```

### CLI Flags

| Flag | Description |
|------|-------------|
| `-i`, `--interactive` | Enable interactive mode (PTY + user input forwarding) |
| `-a`, `--autonomous` | Force autonomous mode (overrides `default_mode` config) |
| `--idle-timeout <secs>` | Override idle timeout in interactive mode (0 to disable) |

`-i` and `-a` are mutually exclusive. If neither is specified, `default_mode` from config is used.

No `--observe`, `--pty`, or `--no-pty` flags.

## Behavior

### Autonomous Mode (Default)

Ralph orchestrates the agent headlessly.

| Aspect | Behavior |
|--------|----------|
| **Process spawn** | Standard subprocess with piped I/O |
| **Working directory** | Inherited from Ralph's cwd (see [cli-adapters.spec.md](cli-adapters.spec.md)) |
| **Agent flags** | Non-interactive flags included (see table below) |
| **User input** | Not forwarded (stdin not connected) |
| **Ctrl+C** | SIGTERM to agent → 5s grace → SIGKILL (see [event-loop.spec.md](event-loop.spec.md)) |
| **Output** | Piped to stdout; ANSI intentionally stripped for event parsing |
| **Timeout** | Per-adapter timeout applies (see [cli-adapters.spec.md](cli-adapters.spec.md)) |
| **Use case** | CI, automation, background loops |

### Interactive Mode (`-i`)

User interacts with the agent through Ralph.

| Aspect | Behavior |
|--------|----------|
| **Process spawn** | PTY via `portable-pty` |
| **Working directory** | Explicitly set on `CommandBuilder` (see below) |
| **Agent flags** | Non-interactive flags **omitted** |
| **User input** | Forwarded to agent |
| **Ctrl+C** | Forwarded to agent (see signal handling) |
| **Output** | Real-time with ANSI preserved |
| **Use case** | Development, debugging, manual oversight |

**PTY Working Directory**: Unlike standard subprocesses, PTY-spawned processes may not reliably inherit the parent's working directory on all platforms. The `CommandBuilder` must explicitly call `.cwd(path)` to ensure the agent starts in the correct directory. This is the directory from which `ralph` was invoked. See [cli-adapters.spec.md](cli-adapters.spec.md) for working directory semantics.

### Agent Flags by Mode

Each backend has flags that enable/disable interactive behavior:

| Backend | Autonomous Mode | Interactive Mode | Notes |
|---------|-----------------|------------------|-------|
| **claude** | `--dangerously-skip-permissions` | `--dangerously-skip-permissions` | Permission flag is about tool approval, not interactivity |
| **kiro** | `--no-interactive --trust-all-tools` | `--trust-all-tools` | `--no-interactive` controls prompting behavior |
| **codex** | `exec --full-auto` | `exec` | `--full-auto` disables confirmation prompts |
| **amp** | `--dangerously-allow-all` | *(no flags)* | Amp is interactive by default; flag disables prompts |
| **gemini** | *(no flags)* | *(no flags)* | Gemini has no interactive/headless distinction |

*(no flags)* means no additional arguments are passed—the backend runs with its default behavior.

### Signal Handling

#### Autonomous Mode

| Signal | Behavior |
|--------|----------|
| Ctrl+C (SIGINT) | Send SIGTERM to agent, wait up to 5s, SIGKILL if needed, then exit Ralph |
| Ctrl+\ (SIGQUIT) | Send SIGKILL to agent immediately, then exit Ralph |

This matches the process management behavior in [event-loop.spec.md](event-loop.spec.md).

#### Interactive Mode

| Signal | Behavior |
|--------|----------|
| Ctrl+C (1st) | Forward to agent, start 1-second window |
| Ctrl+C (2nd within 1s) | Ralph sends SIGTERM to agent |
| Ctrl+\ | Ralph sends SIGKILL immediately |
| Idle timeout | Ralph sends SIGTERM, then SIGKILL after 5s grace |

This now actually works because the agent runs in interactive mode and handles Ctrl+C gracefully instead of exiting immediately.

### Idle Timeout

In interactive mode, the idle timeout resets on:
- Agent output (any bytes from PTY)
- User input (any key forwarded to agent)

The timeout triggers when both agent and user are idle for `idle_timeout_secs`.

Set `idle_timeout_secs: 0` to disable (wait indefinitely).

### Terminal Requirements

Interactive mode requires Ralph's stdout to be a TTY. If not:

```
⚠ Interactive mode requested but stdout is not a TTY, falling back to autonomous mode
```

This ensures Ralph works correctly in CI/pipeline environments.

## Implementation

### CliBackend Changes

`CliBackend` gains a mode parameter:

```rust
impl CliBackend {
    /// Creates backend for the given execution mode.
    pub fn kiro(interactive: bool) -> Self {
        let mut args = vec!["chat".to_string(), "--trust-all-tools".to_string()];
        if !interactive {
            args.insert(1, "--no-interactive".to_string());
        }
        Self {
            command: "kiro-cli".to_string(),
            args,
            prompt_mode: PromptMode::Arg,
            prompt_flag: None,
        }
    }
}
```

### Executor Selection

```rust
if config.interactive_mode() && stdout_is_tty() {
    PtyExecutor::new(backend, pty_config).run_interactive(prompt)
} else {
    CliExecutor::new(backend).execute(prompt, writer)
}
```

### Config Migration

| Old Config | New Config |
|------------|------------|
| `pty_mode: false` | `default_mode: "autonomous"` (or omit) |
| `pty_mode: true, pty_interactive: true` | `default_mode: "interactive"` |
| `pty_mode: true, pty_interactive: false` | `default_mode: "autonomous"` |

## Removed Features

| Removed | Reason |
|---------|--------|
| `--pty` flag | Replaced by `--interactive` |
| `--observe` flag | No clear use case |
| `--no-pty` flag | Autonomous is default |
| `pty_mode` config | Replaced by `default_mode` |
| `pty_interactive` config | Collapsed into `default_mode` |

## Specs Requiring Updates

After this spec is approved, update:

| Spec | Changes Needed |
|------|----------------|
| [event-loop.spec.md](event-loop.spec.md) | Update CLI flags table |
| [cli-adapters.spec.md](cli-adapters.spec.md) | Update config schema and acceptance criteria |
| [adapters/kiro.spec.md](adapters/kiro.spec.md) | Add section on interactive mode flags |
| [adapters/claude.spec.md](adapters/claude.spec.md) | Add section on interactive mode (if applicable) |

## Acceptance Criteria

### Mode Selection

- **Given** no flags specified and default config
- **When** Ralph starts
- **Then** autonomous mode is used (headless, non-interactive agent flags)

- **Given** `-i` or `--interactive` flag
- **When** Ralph starts with TTY stdout
- **Then** interactive mode is used (PTY, interactive agent flags)

- **Given** `--interactive` flag
- **When** Ralph starts without TTY stdout (piped)
- **Then** warning logged, falls back to autonomous mode

- **Given** `default_mode: "interactive"` in config
- **When** Ralph starts without flags
- **Then** interactive mode is used (if TTY available)

- **Given** `default_mode: "interactive"` in config
- **When** Ralph starts with `-a` or `--autonomous` flag
- **Then** autonomous mode is used (flag overrides config)

- **Given** both `-i` and `-a` flags specified
- **When** Ralph parses arguments
- **Then** error is returned (mutually exclusive flags)

### Agent Invocation

- **Given** `backend: "kiro"` in autonomous mode
- **When** Ralph builds command
- **Then** args include `--no-interactive --trust-all-tools`

- **Given** `backend: "kiro"` in interactive mode
- **When** Ralph builds command
- **Then** args include `--trust-all-tools` but NOT `--no-interactive`

- **Given** `backend: "codex"` in autonomous mode
- **When** Ralph builds command
- **Then** args include `exec --full-auto`

- **Given** `backend: "codex"` in interactive mode
- **When** Ralph builds command
- **Then** args include `exec` but NOT `--full-auto`

- **Given** `backend: "claude"` in either mode
- **When** Ralph builds command
- **Then** args include `--dangerously-skip-permissions` (same in both modes)

### Signal Handling

- **Given** interactive mode
- **When** user presses Ctrl+C once
- **Then** Ctrl+C is forwarded to agent (agent handles it)

- **Given** interactive mode, agent in interactive mode
- **When** user presses Ctrl+C once
- **Then** agent may prompt for confirmation or cancel current operation (not exit)

- **Given** interactive mode
- **When** user presses Ctrl+C twice within 1 second
- **Then** Ralph sends SIGTERM to agent

- **Given** interactive mode and SIGTERM sent to agent
- **When** agent does not exit within 5 seconds
- **Then** Ralph sends SIGKILL to force termination

- **Given** interactive mode
- **When** user presses Ctrl+\
- **Then** Ralph sends SIGKILL immediately (no grace period)

- **Given** autonomous mode
- **When** user presses Ctrl+C
- **Then** Ralph sends SIGTERM to agent, waits up to 5s, then SIGKILL if needed

### User Input

- **Given** interactive mode
- **When** agent prompts for input (e.g., confirmation)
- **Then** user's keystrokes are forwarded to agent

- **Given** autonomous mode
- **When** user types
- **Then** input is ignored (not forwarded)

### Output

- **Given** interactive mode
- **When** agent produces TUI output (colors, spinners)
- **Then** output renders correctly (ANSI preserved)

- **Given** autonomous mode
- **When** agent produces output
- **Then** output is piped (ANSI may be lost)

### Idle Timeout

- **Given** interactive mode with `idle_timeout_secs: 30`
- **When** neither agent nor user produces activity for 30 seconds
- **Then** Ralph sends SIGTERM to agent

- **Given** interactive mode with `idle_timeout_secs: 30`
- **When** user presses a key at 29 seconds of inactivity
- **Then** timeout resets

- **Given** interactive mode with `idle_timeout_secs: 0`
- **When** agent and user are idle
- **Then** no timeout (wait indefinitely)

### Terminal State

- **Given** interactive mode completes (success or failure)
- **When** Ralph exits normally
- **Then** terminal is restored to original state (raw mode disabled)

- **Given** interactive mode is active
- **When** Ralph receives SIGTERM or SIGINT
- **Then** terminal is restored before exit (cleanup handler runs)

- **Given** interactive mode is active
- **When** Ralph crashes or is killed with SIGKILL
- **Then** terminal may be in raw mode (user can recover with `reset` command)

**Note:** SIGKILL cannot be caught, so terminal restoration is best-effort. Document `reset` command as recovery option.

### Working Directory (PTY-specific)

- **Given** interactive mode (`-i`)
- **When** Ralph spawns the agent in a PTY
- **Then** `CommandBuilder.cwd()` is called with Ralph's current working directory

- **Given** interactive mode and user runs `ralph run -i` from `/home/user/project/src`
- **When** the agent executes `pwd` or checks its working directory
- **Then** the result is `/home/user/project/src`

- **Given** interactive mode and the working directory becomes inaccessible after Ralph starts
- **When** Ralph attempts to spawn the PTY
- **Then** an error is returned (PTY spawn fails with OS error)

## Non-Goals

- **Observe mode**: Removed. If you're watching, you might as well interact.
- **Per-backend PTY override**: All backends use the same mode. PTY is implementation detail.
- **Terminal resize propagation**: PTY dimensions set at spawn, not updated on SIGWINCH.
