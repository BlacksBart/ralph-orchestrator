---
status: draft
gap_analysis: null
related:
  - event-system.spec.md
---

# Terminal UI Spec

Ralph Orchestrator's real-time terminal dashboard for monitoring loop execution.

## Overview

A ratatui-based terminal UI that displays Ralph's current state during orchestration runs. The UI observes loop events in real-time using the Observer pattern, showing which hat Ralph is wearing, iteration progress, and timing information.

## Goals

1. **Visibility**: See Ralph's current activity without parsing log output
2. **Progress tracking**: Know iteration count and elapsed time at a glance
3. **Non-intrusive**: UI observes state; it doesn't control execution
4. **Minimal overhead**: Rendering shouldn't slow the orchestration loop

## Non-Goals

- Interactive controls (pause, resume, cancel) - future work
- Historical event browsing - use `ralph events` command
- Log streaming - separate concern
- Configuration editing - use YAML files

## Architecture

### Observer/State Pattern

The UI integrates via the existing `EventBus.set_observer()` callback mechanism. The observer receives all events as they flow through the system.

```
┌─────────────────┐     publishes      ┌─────────────────┐
│   Event Loop    │ ─────────────────▶ │    EventBus     │
└─────────────────┘                    └────────┬────────┘
                                                │
                                         observer callback
                                                │
                                                ▼
                                       ┌─────────────────┐
                                       │   TuiObserver   │
                                       │  (implements    │
                                       │   Fn(Event))    │
                                       └────────┬────────┘
                                                │
                                          updates state
                                                │
                                                ▼
                                       ┌─────────────────┐
                                       │    TuiState     │
                                       │  (Arc<Mutex>)   │
                                       └────────┬────────┘
                                                │
                                           renders to
                                                │
                                                ▼
                                       ┌─────────────────┐
                                       │  Ratatui Frame  │
                                       └─────────────────┘
```

### State Structure

The UI maintains observable state derived from loop events:

**TuiState** holds:
- `current_hat`: Which hat Ralph is wearing (`HatId` + display name)
- `current_hat_started`: When this hat's iteration began
- `iteration`: Current iteration number (1-indexed)
- `loop_started`: Timestamp when the loop began
- `loop_elapsed`: Total elapsed time since loop start
- `iteration_elapsed`: Elapsed time for current iteration
- `last_event`: Most recent event topic for activity indicator
- `termination`: Optional termination reason when loop ends

### Event-to-State Mapping

| Event Topic | State Update |
|-------------|--------------|
| `task.start` | Reset all state, set `loop_started` |
| `task.resume` | Set `loop_started`, preserve iteration from payload |
| `build.task` | Set `current_hat` to builder |
| `build.done` | Set `current_hat` to planner |
| `build.blocked` | Set `current_hat` to planner |
| Any event | Update `last_event`, recalculate elapsed times |

Hat transitions are inferred from event topics and the `source` field on events.

## UI Layout

```
┌─────────────────────────────────────────────────────────┐
│  🎩 RALPH ORCHESTRATOR                          [LIVE]  │
├─────────────────────────────────────────────────────────┤
│                                                         │
│     Current Hat: 📋 Planner                             │
│                                                         │
│     Iteration:   3                                      │
│                                                         │
│     Loop Time:   00:05:23                               │
│     This Run:    00:01:47                               │
│                                                         │
├─────────────────────────────────────────────────────────┤
│  Last: build.done                              ◉ active │
└─────────────────────────────────────────────────────────┘
```

### Layout Regions

1. **Header**: Title bar with live/paused indicator
2. **Status Panel**: Current hat, iteration, timing
3. **Footer**: Last event topic, activity indicator

### Hat Display

Each hat displays with an emoji and name:

| Hat ID | Display |
|--------|---------|
| `planner` | 📋 Planner |
| `builder` | 🔨 Builder |
| Custom | 🎭 {name} |

### Timing Display

- **Loop Time**: `HH:MM:SS` since `task.start` or `task.resume`
- **This Run**: `HH:MM:SS` since current hat began processing

Times update every 100ms via a separate tick mechanism (not blocking on events).

### Activity Indicator

- `◉ active` (green, blinking): Event received in last 2 seconds
- `◯ idle` (dim): No recent events
- `■ done` (blue): Loop terminated

## Integration

### Invocation

```bash
# Run with TUI enabled
ralph run --tui

# TUI-only mode (attach to existing run via event log)
ralph tui
```

### Configuration

```yaml
# ralph.yml
tui:
  enabled: true
  refresh_rate_ms: 100
  show_cost: false  # Optional: display cumulative cost
```

### Implementation Location

All TUI code lives in the `ralph-tui` crate:

- `lib.rs` - Public API: `Tui::new()`, `Tui::run()`
- `state.rs` - `TuiState` and state management
- `observer.rs` - `TuiObserver` implementing the callback
- `widgets/` - Ratatui widget implementations
- `app.rs` - Main application loop and event handling

### Dependencies

Uses workspace dependencies already available:
- `ratatui` - Terminal UI framework
- `crossterm` - Terminal manipulation backend

## Acceptance Criteria

1. **Hat visibility**: UI displays current hat name and emoji within 100ms of hat change
2. **Iteration counter**: Shows correct iteration number, updates on each iteration
3. **Loop timer**: Shows total elapsed time since loop start, updates every 100ms
4. **Iteration timer**: Shows elapsed time for current iteration, resets on hat change
5. **Activity indicator**: Pulses green when events flow, dims after 2s idle
6. **Clean exit**: UI restores terminal state on loop termination or Ctrl+C
7. **No interference**: UI observation doesn't affect loop execution or event routing

## Future Considerations

- Event log panel showing recent events
- Cost tracking display (using existing `LoopState.cumulative_cost`)
- Multiple hat tracking for concurrent execution
- Detachable TUI that can reconnect to running loops
