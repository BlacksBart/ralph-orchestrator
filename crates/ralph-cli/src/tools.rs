//! CLI commands for the `ralph tools` namespace.
//!
//! Ralph's runtime tools - things Ralph uses during orchestration.
//! This namespace contains agent-facing tools, while top-level commands
//! are user-facing.
//!
//! Subcommands:
//! - `memory`: Persistent memories for accumulated learning
//! - `task`: Work item tracking (beads-lite)
//! - `skill`: Load skill content on demand
//! - `interact`: Human-in-the-loop communication (progress updates, notifications)

use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::interact;
use crate::memory;
use crate::skill_cli;
use crate::task_cli;

/// Ralph's runtime tools (agent-facing).
#[derive(Parser, Debug)]
pub struct ToolsArgs {
    #[command(subcommand)]
    pub command: ToolsCommands,
}

#[derive(Subcommand, Debug)]
pub enum ToolsCommands {
    /// Manage persistent memories for accumulated learning
    Memory(memory::MemoryArgs),

    /// Manage work items (task tracking)
    Task(task_cli::TaskArgs),

    /// Load and manage skills
    Skill(skill_cli::SkillArgs),

    /// Interact with human via Telegram (progress updates, notifications)
    Interact(interact::InteractArgs),
}

/// Execute a tools command.
///
/// `workspace_root` is the resolved git root (from `find_workspace_root()`).
/// It's injected into memory/task args when no explicit `--root` was provided,
/// so the `PathBuf::from(".")` fallback is never reached from a subdirectory.
pub async fn execute(
    args: ToolsArgs,
    use_colors: bool,
    workspace_root: &std::path::Path,
) -> Result<()> {
    match args.command {
        ToolsCommands::Memory(mut memory_args) => {
            if memory_args.root.is_none() {
                memory_args.root = Some(workspace_root.to_path_buf());
            }
            memory::execute(memory_args, use_colors)
        }
        ToolsCommands::Task(mut task_args) => {
            if task_args.root.is_none() {
                task_args.root = Some(workspace_root.to_path_buf());
            }
            task_cli::execute(task_args, use_colors)
        }
        ToolsCommands::Skill(skill_args) => skill_cli::execute(skill_args),
        ToolsCommands::Interact(interact_args) => interact::execute(interact_args).await,
    }
}
