# API Reference

Technical reference documentation for Ralph's crates.

## Crate Overview

| Crate | Purpose | Documentation |
|-------|---------|---------------|
| [ralph-proto](ralph-proto.md) | Protocol types: Event, Hat, Topic | Core data structures |
| [ralph-core](ralph-core.md) | Orchestration engine | EventLoop, Config |
| [ralph-adapters](ralph-adapters.md) | CLI backends | Backend integrations |
| [ralph-tui](ralph-tui.md) | Terminal UI | TUI components |
| [ralph-cli](ralph-cli.md) | Binary entry point | CLI commands |

## Quick Links

### Core Types

```rust
// Events
use ralph_proto::{Event, Topic, EventBus};

// Hats
use ralph_proto::{Hat, HatId};

// Configuration
use ralph_core::config::{Config, EventLoopConfig, CliConfig};
```

### Common Operations

```rust
// Load configuration
let config = Config::load("ralph.yml")?;

// Create event loop
let event_loop = EventLoop::new(config);

// Run orchestration
event_loop.run().await?;
```

## Rust Documentation

Generate and view Rust docs:

```bash
# Generate docs
cargo doc --no-deps --open

# Generate with dependencies
cargo doc --open
```

## Stability

| Crate | Status |
|-------|--------|
| ralph-proto | Stable |
| ralph-core | Stable |
| ralph-adapters | Stable |
| ralph-tui | Experimental |
| ralph-cli | Stable |
| ralph-e2e | Internal |
| ralph-bench | Internal |

"Stable" means the public API is unlikely to change in breaking ways.
"Experimental" means the API may change.
"Internal" means the crate is not intended for external use.
