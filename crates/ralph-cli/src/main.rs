//! # ralph-cli
//!
//! Binary entry point for the Ralph Orchestrator.
//!
//! This crate provides:
//! - CLI argument parsing using `clap`
//! - Application initialization and configuration
//! - Entry point to the headless orchestration loop
//! - Event history viewing via `ralph events`
//! - Project initialization via `ralph init`
//! - SOP-based planning via `ralph plan`
//! - Code task generation via `ralph code-task`
//! - Work item tracking via `ralph task`

mod bot;
mod display;
mod doctor;
mod hats;
mod init;
mod interact;
mod loop_runner;
mod loops;
mod memory;
mod preflight;
mod presets;
mod skill_cli;
mod sop_runner;
mod task_cli;
#[cfg(test)]
mod test_support;
mod tools;
mod web;

use anyhow::{Context, Result};
use clap::{ArgAction, CommandFactory, Parser, Subcommand, ValueEnum};
use ralph_adapters::detect_backend;
use ralph_core::{
    CheckStatus, EventHistory, LockError, LoopContext, LoopEntry, LoopLock, LoopRegistry,
    PreflightReport, PreflightRunner, RalphConfig, TerminationReason,
    worktree::{WorktreeConfig, create_worktree, ensure_gitignore, remove_worktree},
};
use std::fs;
use std::io::{IsTerminal, Write, stdout};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

// Unix-specific process management for process group leadership
#[cfg(unix)]
mod process_management {
    use nix::unistd::{Pid, getpgrp, setpgid, tcgetpgrp};
    use std::io::{IsTerminal, stdin, stdout};
    use tracing::debug;

    /// Sets up process group leadership.
    ///
    /// Per spec: "The orchestrator must run as a process group leader. All spawned
    /// CLI processes (Claude, Kiro, etc.) belong to this group. On termination,
    /// the entire process group receives the signal, preventing orphans."
    pub fn setup_process_group() {
        // Make ourselves the process group leader when safe.
        // If we're launched by a wrapper (e.g., `npx`), moving to a new process
        // group can drop us out of the foreground TTY group and break TUI input.
        let pid = Pid::this();
        let pgrp = getpgrp();
        if pgrp == pid {
            debug!("Already process group leader: PID {}", pid);
            return;
        }

        if is_foreground_tty_group(pgrp) {
            debug!(
                "Skipping setpgid: keeping foreground process group {}",
                pgrp
            );
            return;
        }

        if let Err(e) = setpgid(pid, pid) {
            // EPERM is OK - we're already a process group leader (e.g., started from shell)
            if e != nix::errno::Errno::EPERM {
                debug!(
                    "Note: Could not set process group ({}), continuing anyway",
                    e
                );
            }
        }
        debug!("Process group initialized: PID {}", pid);
    }

    fn is_foreground_tty_group(current_pgrp: Pid) -> bool {
        // Prefer stdin for foreground checks, fall back to stdout.
        if stdin().is_terminal()
            && let Ok(fg) = tcgetpgrp(stdin())
        {
            return fg == current_pgrp;
        }

        if stdout().is_terminal()
            && let Ok(fg) = tcgetpgrp(stdout())
        {
            return fg == current_pgrp;
        }

        false
    }
}

#[cfg(not(unix))]
mod process_management {
    /// No-op on non-Unix platforms.
    pub fn setup_process_group() {}
}

/// Installs a panic hook that restores terminal state before printing panic info.
///
/// When a TUI application panics, the terminal can be left in a broken state:
/// - Raw mode enabled (input not line-buffered)
/// - Alternate screen buffer active (no scrollback)
/// - Cursor hidden
///
/// This hook ensures the terminal is restored so the panic message is visible
/// and the user can scroll/interact normally.
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Restore terminal state before printing panic info
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::LeaveAlternateScreen,
            crossterm::cursor::Show
        );
        // Call the default panic hook to print the panic message
        default_hook(panic_info);
    }));
}

/// Color output mode for terminal display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum ColorMode {
    /// Automatically detect if stdout is a TTY
    #[default]
    Auto,
    /// Always use colors
    Always,
    /// Never use colors
    Never,
}

impl ColorMode {
    /// Returns true if colors should be used based on mode and terminal detection.
    fn should_use_colors(self) -> bool {
        match self {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => stdout().is_terminal(),
        }
    }
}

/// Verbosity level for streaming output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Verbosity {
    /// Suppress all streaming output (for CI/scripting)
    Quiet,
    /// Show assistant text and tool invocations (default)
    #[default]
    Normal,
    /// Show everything including tool results and session summary
    Verbose,
}

impl Verbosity {
    /// Resolves verbosity from CLI args, env vars, and config.
    ///
    /// Precedence (highest to lowest):
    /// 1. CLI flags: `--verbose`/`-v` or `--quiet`/`-q`
    /// 2. Environment variables: `RALPH_VERBOSE=1` or `RALPH_QUIET=1`
    /// 3. Config file: (if supported in future)
    /// 4. Default: Normal
    fn resolve(cli_verbose: bool, cli_quiet: bool) -> Self {
        let env_quiet = std::env::var("RALPH_QUIET").is_ok();
        let env_verbose = std::env::var("RALPH_VERBOSE").is_ok();
        Self::resolve_with_env(cli_verbose, cli_quiet, env_quiet, env_verbose)
    }

    #[allow(clippy::fn_params_excessive_bools)]
    fn resolve_with_env(
        cli_verbose: bool,
        cli_quiet: bool,
        env_quiet: bool,
        env_verbose: bool,
    ) -> Self {
        // CLI flags take precedence
        if cli_quiet {
            return Verbosity::Quiet;
        }
        if cli_verbose {
            return Verbosity::Verbose;
        }

        // Environment variables
        if env_quiet {
            return Verbosity::Quiet;
        }
        if env_verbose {
            return Verbosity::Verbose;
        }

        Verbosity::Normal
    }
}

/// Output format for events command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum OutputFormat {
    /// Human-readable table format
    #[default]
    Table,
    /// JSON format for programmatic access
    Json,
}

// Re-export colors and truncate from display module for use in this file
use display::colors;
use display::truncate;

/// Source for configuration: file path, builtin preset, remote URL, or config override.
#[derive(Debug, Clone)]
pub enum ConfigSource {
    /// Local file path (default behavior)
    File(PathBuf),
    /// Builtin preset name (e.g., "builtin:feature")
    Builtin(String),
    /// Remote URL (e.g., "http://example.com/preset.yml")
    Remote(String),
    /// Config override (e.g., "core.scratchpad=.ralph/feature/scratchpad.md")
    Override { key: String, value: String },
}

impl ConfigSource {
    /// Parse a config source string into its variant.
    ///
    /// Format:
    /// - `core.field=value` → Override (for core.* fields)
    /// - `builtin:preset-name` → Builtin preset
    /// - `http://...` or `https://...` → Remote URL
    /// - Anything else → File path
    fn parse(s: &str) -> Self {
        // Check for core.* override pattern first (prevents false positives on paths with '=')
        // Only treat as override if it starts with "core." AND contains '='
        if s.starts_with("core.")
            && let Some((key, value)) = s.split_once('=')
        {
            return ConfigSource::Override {
                key: key.to_string(),
                value: value.to_string(),
            };
        }
        // Existing logic unchanged
        if let Some(name) = s.strip_prefix("builtin:") {
            ConfigSource::Builtin(name.to_string())
        } else if s.starts_with("http://") || s.starts_with("https://") {
            ConfigSource::Remote(s.to_string())
        } else {
            ConfigSource::File(PathBuf::from(s))
        }
    }
}

/// Known core fields that can be overridden via CLI.
const KNOWN_CORE_FIELDS: &[&str] = &["scratchpad", "specs_dir"];

/// Applies CLI config overrides to the loaded configuration.
///
/// Overrides are in the format `core.field=value` and take precedence
/// over values from the config file.
pub(crate) fn apply_config_overrides(
    config: &mut RalphConfig,
    sources: &[ConfigSource],
) -> anyhow::Result<()> {
    for source in sources {
        if let ConfigSource::Override { key, value } = source {
            match key.as_str() {
                "core.scratchpad" => {
                    config.core.scratchpad = value.clone();
                }
                "core.specs_dir" => {
                    config.core.specs_dir = value.clone();
                }
                other => {
                    // Note: with core.* prefix requirement in parse(), this branch
                    // only handles unknown core.* fields
                    let field = other.strip_prefix("core.").unwrap_or(other);
                    warn!(
                        "Unknown core field '{}'. Known fields: {}",
                        field,
                        KNOWN_CORE_FIELDS.join(", ")
                    );
                }
            }
        }
    }
    Ok(())
}

/// Ensures the scratchpad's parent directory exists, creating it if needed.
pub(crate) fn ensure_scratchpad_directory(config: &RalphConfig) -> anyhow::Result<()> {
    let scratchpad_path = config.core.resolve_path(&config.core.scratchpad);
    if let Some(parent) = scratchpad_path.parent()
        && !parent.exists()
    {
        info!("Creating scratchpad directory: {}", parent.display());
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

/// Loads configuration from file sources with override support.
///
/// This is the common sync path used by resume_command and clean_command.
/// For the full async path (including Remote URLs), see run_command.
///
/// Returns the loaded config with overrides applied and workspace_root set.
pub(crate) fn load_config_with_overrides(
    config_sources: &[ConfigSource],
) -> anyhow::Result<RalphConfig> {
    // Partition sources: file sources vs overrides
    let (primary_sources, overrides): (Vec<_>, Vec<_>) = config_sources
        .iter()
        .partition(|s| !matches!(s, ConfigSource::Override { .. }));

    // Load configuration from first file source, or default ralph.yml
    let mut config = if let Some(ConfigSource::File(path)) = primary_sources.first() {
        if path.exists() {
            RalphConfig::from_file(path)
                .with_context(|| format!("Failed to load config from {:?}", path))?
        } else {
            warn!("Config file {:?} not found, using defaults", path);
            RalphConfig::default()
        }
    } else {
        // Only overrides specified - load default ralph.yml as base
        let default_path = PathBuf::from("ralph.yml");
        if default_path.exists() {
            RalphConfig::from_file(&default_path)
                .with_context(|| "Failed to load config from ralph.yml")?
        } else {
            RalphConfig::default()
        }
    };

    config.normalize();

    // Set workspace_root to current directory
    config.core.workspace_root =
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    // Apply CLI config overrides
    let override_sources: Vec<_> = overrides.into_iter().cloned().collect();
    apply_config_overrides(&mut config, &override_sources)?;

    Ok(config)
}

/// Ralph Orchestrator - Multi-agent orchestration framework
#[derive(Parser, Debug)]
#[command(name = "ralph", version, about, disable_help_subcommand = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    // ─────────────────────────────────────────────────────────────────────────
    // Global options (available for all subcommands)
    // ─────────────────────────────────────────────────────────────────────────
    /// Configuration source: file path, builtin:preset, URL, or core.field=value override.
    /// Can be specified multiple times. Overrides are applied after config file loading.
    #[arg(short, long, default_value = "ralph.yml", global = true, action = ArgAction::Append)]
    config: Vec<String>,

    /// Verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Color output mode (auto, always, never)
    #[arg(long, value_enum, default_value_t = ColorMode::Auto, global = true)]
    color: ColorMode,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run the orchestration loop (default if no subcommand given)
    Run(RunArgs),

    /// Run preflight checks to validate configuration and environment
    Preflight(preflight::PreflightArgs),

    /// Run first-run diagnostics and environment checks
    Doctor(doctor::DoctorArgs),

    /// Interactive walkthrough of hats, presets, and workflow
    Tutorial(TutorialArgs),

    /// DEPRECATED: Use `ralph run --continue` instead.
    /// Resume a previously interrupted loop from existing scratchpad.
    #[command(hide = true)]
    Resume(ResumeArgs),

    /// View event history for debugging
    Events(EventsArgs),

    /// Initialize a new ralph.yml configuration file
    Init(InitArgs),

    /// Clean up Ralph artifacts (.agent/ directory)
    Clean(CleanArgs),

    /// Emit an event to the current run's events file with proper JSON formatting
    Emit(EmitArgs),

    /// Start a Prompt-Driven Development planning session
    Plan(PlanArgs),

    /// Generate code task files from descriptions or plans
    CodeTask(CodeTaskArgs),

    /// Create code tasks (alias for code-task)
    Task(CodeTaskArgs),

    /// Ralph's runtime tools (agent-facing)
    Tools(tools::ToolsArgs),

    /// Manage parallel loops
    Loops(loops::LoopsArgs),

    /// Manage configured hats
    Hats(hats::HatsArgs),

    /// Run the web dashboard
    Web(web::WebArgs),

    /// Manage Telegram bot setup and testing
    Bot(bot::BotArgs),

    /// Generate shell completions
    Completions(CompletionsArgs),

    /// Show help. Use -v for verbose help with examples and motivation.
    #[command(name = "help")]
    Help(HelpArgs),
}

/// Arguments for the init subcommand.
#[derive(Parser, Debug)]
struct InitArgs {
    /// Backend to use (claude, kiro, gemini, codex, amp, custom).
    /// When used alone, generates minimal config.
    /// When used with --preset, overrides the preset's backend.
    #[arg(long, conflicts_with = "list_presets")]
    backend: Option<String>,

    /// Copy embedded preset to ralph.yml
    #[arg(long, conflicts_with = "list_presets")]
    preset: Option<String>,

    /// List all available embedded presets
    #[arg(long, conflicts_with = "backend", conflicts_with = "preset")]
    list_presets: bool,

    /// Overwrite existing ralph.yml if present
    #[arg(long)]
    force: bool,
}

/// Arguments for the run subcommand.
#[derive(Parser, Debug)]
struct RunArgs {
    /// Inline prompt text (mutually exclusive with -P/--prompt-file)
    #[arg(short = 'p', long = "prompt", conflicts_with = "prompt_file")]
    prompt_text: Option<String>,

    /// Override backend from config (cli > config > auto-detect)
    #[arg(short = 'b', long = "backend", value_name = "BACKEND")]
    backend: Option<String>,

    /// Prompt file path (mutually exclusive with -p/--prompt)
    #[arg(short = 'P', long = "prompt-file", conflicts_with = "prompt_text")]
    prompt_file: Option<PathBuf>,

    /// Override max iterations
    #[arg(long)]
    max_iterations: Option<u32>,

    /// Override completion promise
    #[arg(long)]
    completion_promise: Option<String>,

    /// Dry run - show what would be executed without running
    #[arg(long)]
    dry_run: bool,

    /// Continue from existing scratchpad (resume interrupted loop).
    /// Use this when a previous run was interrupted and you want to
    /// continue from where it left off.
    #[arg(long = "continue")]
    continue_mode: bool,

    // ─────────────────────────────────────────────────────────────────────────
    // Execution Mode Options
    // ─────────────────────────────────────────────────────────────────────────
    /// Disable TUI observation mode (TUI is enabled by default)
    #[arg(long, conflicts_with = "autonomous")]
    no_tui: bool,

    /// Force autonomous mode (headless, non-interactive).
    /// Overrides default_mode from config.
    #[arg(short, long, conflicts_with = "no_tui")]
    autonomous: bool,

    /// Idle timeout in seconds for interactive mode (default: 30).
    /// Process is terminated after this many seconds of inactivity.
    /// Set to 0 to disable idle timeout.
    #[arg(long)]
    idle_timeout: Option<u32>,

    // ─────────────────────────────────────────────────────────────────────────
    // Multi-Loop Concurrency Options
    // ─────────────────────────────────────────────────────────────────────────
    /// Wait for the primary loop slot instead of spawning into a worktree.
    /// Use this when you want to ensure only one loop runs at a time.
    #[arg(long)]
    exclusive: bool,

    /// Skip automatic merge after loop completes (keep worktree for manual handling).
    /// Only relevant for parallel loops running in worktrees.
    #[arg(long)]
    no_auto_merge: bool,

    // ─────────────────────────────────────────────────────────────────────────
    // Preflight Options
    // ─────────────────────────────────────────────────────────────────────────
    /// Skip preflight checks before loop start.
    /// Overrides features.preflight.enabled from config.
    #[arg(long)]
    skip_preflight: bool,

    // ─────────────────────────────────────────────────────────────────────────
    // Verbosity Options
    // ─────────────────────────────────────────────────────────────────────────
    /// Enable verbose output (show tool results and session summary)
    #[arg(short = 'v', long, conflicts_with = "quiet")]
    verbose: bool,

    /// Suppress streaming output (for CI/scripting)
    #[arg(short = 'q', long, conflicts_with = "verbose")]
    quiet: bool,

    /// Record session to JSONL file for replay testing
    #[arg(long, value_name = "FILE")]
    record_session: Option<PathBuf>,

    /// Custom backend command and arguments (use after --)
    #[arg(last = true)]
    custom_args: Vec<String>,
}

/// Arguments for the resume subcommand.
///
/// Per spec: "When loop terminates due to safeguard (not completion promise),
/// user can run `ralph resume` to restart reading existing scratchpad."
#[derive(Parser, Debug)]
struct ResumeArgs {
    /// Override max iterations (from current position)
    #[arg(long)]
    max_iterations: Option<u32>,

    /// Disable TUI observation mode (TUI is enabled by default)
    #[arg(long, conflicts_with = "autonomous")]
    no_tui: bool,

    /// Force autonomous mode
    #[arg(short, long, conflicts_with = "no_tui")]
    autonomous: bool,

    /// Idle timeout in seconds for TUI mode
    #[arg(long)]
    idle_timeout: Option<u32>,

    /// Enable verbose output (show tool results and session summary)
    #[arg(short = 'v', long, conflicts_with = "quiet")]
    verbose: bool,

    /// Suppress streaming output (for CI/scripting)
    #[arg(short = 'q', long, conflicts_with = "verbose")]
    quiet: bool,

    /// Record session to JSONL file for replay testing
    #[arg(long, value_name = "FILE")]
    record_session: Option<PathBuf>,
}

/// Arguments for the events subcommand.
#[derive(Parser, Debug)]
struct EventsArgs {
    /// Show only the last N events
    #[arg(long)]
    last: Option<usize>,

    /// Filter by topic (e.g., "build.blocked")
    #[arg(long)]
    topic: Option<String>,

    /// Filter by iteration number
    #[arg(long)]
    iteration: Option<u32>,

    /// Output format
    #[arg(long, value_enum, default_value_t = OutputFormat::Table)]
    format: OutputFormat,

    /// Path to events file (default: auto-detects current run)
    #[arg(long)]
    file: Option<PathBuf>,

    /// Clear the event history
    #[arg(long)]
    clear: bool,
}

/// Arguments for the clean subcommand.
#[derive(Parser, Debug)]
struct CleanArgs {
    /// Preview what would be deleted without actually deleting
    #[arg(long)]
    dry_run: bool,

    /// Clean diagnostic logs instead of .agent directory
    #[arg(long)]
    diagnostics: bool,
}

/// Arguments for the emit subcommand.
#[derive(Parser, Debug)]
struct EmitArgs {
    /// Event topic (e.g., "build.done", "review.complete")
    pub topic: String,

    /// Event payload - string or JSON (optional, defaults to empty)
    #[arg(default_value = "")]
    pub payload: String,

    /// Parse payload as JSON object instead of string
    #[arg(long, short)]
    pub json: bool,

    /// Custom ISO 8601 timestamp (defaults to current time)
    #[arg(long)]
    pub ts: Option<String>,

    /// Path to events file (defaults to .ralph/events.jsonl)
    #[arg(long, default_value = ".ralph/events.jsonl")]
    pub file: PathBuf,
}

/// Arguments for the tutorial subcommand.
#[derive(Parser, Debug)]
struct TutorialArgs {
    /// Skip prompts and print the tutorial in one pass
    #[arg(long)]
    no_input: bool,
}

/// Arguments for the help subcommand.
#[derive(Parser, Debug)]
struct HelpArgs {
    /// Show detailed help with examples and motivation for each tool
    #[arg(short, long)]
    verbose: bool,

    /// Emit a prompt teaching an LLM how to use Ralph for a scenario.
    /// Without a name, lists available scenarios.
    #[arg(short, long)]
    prompt: bool,

    /// Topic (with -v) or scenario (with --prompt)
    topic: Option<String>,
}

/// Arguments for the plan subcommand.
///
/// Starts an interactive PDD (Prompt-Driven Development) session.
/// This is a thin wrapper that spawns the AI backend with the bundled
/// PDD SOP, bypassing Ralph's event loop entirely.
#[derive(Parser, Debug)]
struct PlanArgs {
    /// The rough idea to develop (optional - SOP will prompt if not provided)
    #[arg(value_name = "IDEA")]
    idea: Option<String>,

    /// Backend to use (overrides config and auto-detection)
    #[arg(short, long, value_name = "BACKEND")]
    backend: Option<String>,

    /// Enable Claude Code's experimental Agent Teams feature
    #[arg(long)]
    teams: bool,

    /// Custom backend command and arguments (use after --)
    #[arg(last = true)]
    custom_args: Vec<String>,
}

/// Arguments for the task subcommand.
///
/// Starts an interactive code-task-generator session.
/// This is a thin wrapper that spawns the AI backend with the bundled
/// code-task-generator SOP, bypassing Ralph's event loop entirely.
#[derive(Parser, Debug)]
struct CodeTaskArgs {
    /// Input: description text or path to PDD plan file
    #[arg(value_name = "INPUT")]
    input: Option<String>,

    /// Backend to use (overrides config and auto-detection)
    #[arg(short, long, value_name = "BACKEND")]
    backend: Option<String>,

    /// Enable Claude Code's experimental Agent Teams feature
    #[arg(long)]
    teams: bool,

    /// Custom backend command and arguments (use after --)
    #[arg(last = true)]
    custom_args: Vec<String>,
}

/// Arguments for the completions subcommand.
#[derive(Parser, Debug)]
struct CompletionsArgs {
    /// Shell to generate completions for
    #[arg(value_enum)]
    shell: clap_complete::Shell,
}

fn completions_command(args: CompletionsArgs) -> Result<()> {
    use clap_complete::generate;

    let mut cli = Cli::command();
    generate(args.shell, &mut cli, "ralph", &mut std::io::stdout());
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Install panic hook to restore terminal state on crash
    // This prevents the terminal from being left in raw mode or alternate screen
    install_panic_hook();

    let cli = Cli::parse();

    // Detect if TUI mode is requested - TUI owns the terminal, so logs must not go to stdout
    // TUI is enabled by default unless --no-tui is specified or --autonomous is used
    let tui_enabled = match &cli.command {
        Some(Commands::Run(args)) => !args.no_tui && !args.autonomous,
        Some(Commands::Resume(args)) => !args.no_tui && !args.autonomous,
        None => true,
        _ => false,
    };

    // Initialize logging - suppress in TUI mode to avoid corrupting the display
    let filter = if cli.verbose { "debug" } else { "info" };

    // Check if diagnostics are enabled
    let diagnostics_enabled = std::env::var("RALPH_DIAGNOSTICS")
        .map(|v| v == "1")
        .unwrap_or(false);

    if tui_enabled {
        // TUI mode: logs would corrupt the display, so write to a rotating log file
        if let Ok((file, _log_path)) =
            ralph_core::diagnostics::create_log_file(std::path::Path::new("."))
        {
            if diagnostics_enabled {
                use ralph_core::diagnostics::DiagnosticTraceLayer;
                use tracing_subscriber::prelude::*;

                if let Ok(collector) =
                    ralph_core::diagnostics::DiagnosticsCollector::new(std::path::Path::new("."))
                    && let Some(session_dir) = collector.session_dir()
                {
                    if let Ok(trace_layer) = DiagnosticTraceLayer::new(session_dir) {
                        tracing_subscriber::registry()
                            .with(
                                tracing_subscriber::fmt::layer()
                                    .with_writer(std::sync::Mutex::new(file))
                                    .with_ansi(false),
                            )
                            .with(tracing_subscriber::EnvFilter::new(filter))
                            .with(trace_layer)
                            .init();
                    } else {
                        tracing_subscriber::fmt()
                            .with_env_filter(filter)
                            .with_writer(std::sync::Mutex::new(file))
                            .with_ansi(false)
                            .init();
                    }
                }
            } else {
                tracing_subscriber::fmt()
                    .with_env_filter(filter)
                    .with_writer(std::sync::Mutex::new(file))
                    .with_ansi(false)
                    .init();
            }
        }
        // If log file creation fails, silently continue without logging
    } else {
        // Normal mode: logs go to stdout
        if diagnostics_enabled {
            // Normal mode + diagnostics: stdout + trace layer
            use ralph_core::diagnostics::DiagnosticTraceLayer;
            use tracing_subscriber::prelude::*;

            if let Ok(collector) =
                ralph_core::diagnostics::DiagnosticsCollector::new(std::path::Path::new("."))
                && let Some(session_dir) = collector.session_dir()
            {
                if let Ok(trace_layer) = DiagnosticTraceLayer::new(session_dir) {
                    tracing_subscriber::registry()
                        .with(tracing_subscriber::fmt::layer())
                        .with(tracing_subscriber::EnvFilter::new(filter))
                        .with(trace_layer)
                        .init();
                } else {
                    // Fallback: just stdout
                    tracing_subscriber::fmt().with_env_filter(filter).init();
                }
            } else {
                // Fallback: just stdout
                tracing_subscriber::fmt().with_env_filter(filter).init();
            }
        } else {
            // Normal mode without diagnostics: just stdout
            tracing_subscriber::fmt().with_env_filter(filter).init();
        }
    }

    // Parse all config sources from CLI
    let config_sources: Vec<ConfigSource> =
        cli.config.iter().map(|s| ConfigSource::parse(s)).collect();

    match cli.command {
        Some(Commands::Run(args)) => {
            run_command(&config_sources, cli.verbose, cli.color, args).await
        }
        Some(Commands::Preflight(args)) => {
            preflight::execute(&config_sources, args, cli.color.should_use_colors()).await
        }
        Some(Commands::Doctor(args)) => {
            doctor::execute(&config_sources, args, cli.color.should_use_colors()).await
        }
        Some(Commands::Tutorial(args)) => tutorial_command(cli.color, args),
        Some(Commands::Resume(args)) => {
            resume_command(&config_sources, cli.verbose, cli.color, args).await
        }
        Some(Commands::Events(args)) => events_command(cli.color, args),
        Some(Commands::Init(args)) => init_command(cli.color, args),
        Some(Commands::Clean(args)) => clean_command(&config_sources, cli.color, args),
        Some(Commands::Emit(args)) => emit_command(cli.color, args),
        Some(Commands::Plan(args)) => plan_command(&config_sources, cli.color, args),
        Some(Commands::CodeTask(args)) => code_task_command(&config_sources, cli.color, args),
        Some(Commands::Task(args)) => code_task_command(&config_sources, cli.color, args),
        Some(Commands::Tools(args)) => tools::execute(args, cli.color.should_use_colors()).await,
        Some(Commands::Loops(args)) => loops::execute(args, cli.color.should_use_colors()),
        Some(Commands::Hats(args)) => {
            hats::execute(&config_sources, args, cli.color.should_use_colors())
        }
        Some(Commands::Web(args)) => web::execute(args).await,
        Some(Commands::Bot(args)) => {
            bot::execute(args, &config_sources, cli.color.should_use_colors()).await
        }
        Some(Commands::Completions(args)) => completions_command(args),
        Some(Commands::Help(args)) => help_command(cli.color, args),
        None => {
            // Default to run with TUI enabled (new default behavior)
            let args = RunArgs {
                prompt_text: None,
                prompt_file: None,
                backend: None,
                max_iterations: None,
                completion_promise: None,
                dry_run: false,
                continue_mode: false,
                no_tui: false, // TUI enabled by default
                autonomous: false,
                idle_timeout: None,
                exclusive: false,
                no_auto_merge: false,
                skip_preflight: false,
                verbose: false,
                quiet: false,
                record_session: None,
                custom_args: Vec::new(),
            };
            run_command(&config_sources, cli.verbose, cli.color, args).await
        }
    }
}

fn format_preflight_summary(report: &PreflightReport) -> String {
    let icons: Vec<String> = report
        .checks
        .iter()
        .map(|check| {
            let icon = match check.status {
                CheckStatus::Pass => "✓",
                CheckStatus::Warn => "⚠",
                CheckStatus::Fail => "✗",
            };
            format!("{icon} {}", check.name)
        })
        .collect();

    let summary = if icons.is_empty() {
        "no checks".to_string()
    } else {
        icons.join(" ")
    };

    let suffix = if report.failures > 0 {
        format!(
            " ({} failure{})",
            report.failures,
            if report.failures == 1 { "" } else { "s" }
        )
    } else if report.warnings > 0 {
        format!(
            " ({} warning{})",
            report.warnings,
            if report.warnings == 1 { "" } else { "s" }
        )
    } else {
        String::new()
    };

    format!("{summary}{suffix}")
}

enum AutoPreflightMode {
    DryRun,
    Run,
}

fn preflight_failure_detail(report: &PreflightReport, strict: bool) -> String {
    if strict && report.warnings > 0 {
        format!(
            "{} failure{}, {} warning{}",
            report.failures,
            if report.failures == 1 { "" } else { "s" },
            report.warnings,
            if report.warnings == 1 { "" } else { "s" }
        )
    } else {
        format!(
            "{} failure{}",
            report.failures,
            if report.failures == 1 { "" } else { "s" }
        )
    }
}

async fn run_auto_preflight(
    config: &RalphConfig,
    skip_preflight: bool,
    verbose: bool,
    mode: AutoPreflightMode,
) -> Result<Option<PreflightReport>> {
    if skip_preflight || !config.features.preflight.enabled {
        return Ok(None);
    }

    let runner = PreflightRunner::default_checks();
    let mut report = if config.features.preflight.skip.is_empty() {
        runner.run_all(config).await
    } else {
        let skip_lower: std::collections::HashSet<String> = config
            .features
            .preflight
            .skip
            .iter()
            .map(|name| name.to_lowercase())
            .collect();
        let selected: Vec<String> = runner
            .check_names()
            .into_iter()
            .filter(|name| !skip_lower.contains(&name.to_lowercase()))
            .map(|name| name.to_string())
            .collect();
        runner.run_selected(config, &selected).await
    };

    let effective_passed = if config.features.preflight.strict {
        report.failures == 0 && report.warnings == 0
    } else {
        report.failures == 0
    };
    report.passed = effective_passed;

    match mode {
        AutoPreflightMode::DryRun => Ok(Some(report)),
        AutoPreflightMode::Run => {
            print_preflight_summary(&report, verbose, "Preflight: ", false);
            if !effective_passed {
                let detail = preflight_failure_detail(&report, config.features.preflight.strict);
                anyhow::bail!(
                    "Preflight checks failed ({}). Fix the issues above or use --skip-preflight to bypass.",
                    detail
                );
            }
            Ok(None)
        }
    }
}

fn print_preflight_summary(
    report: &PreflightReport,
    verbose: bool,
    prefix: &str,
    use_stdout: bool,
) {
    let summary = format_preflight_summary(report);
    if use_stdout {
        println!("{prefix}{summary}");
    } else {
        eprintln!("{prefix}{summary}");
    }

    let emit = |line: String| {
        if use_stdout {
            println!("{line}");
        } else {
            eprintln!("{line}");
        }
    };

    for check in &report.checks {
        if check.status == CheckStatus::Fail
            && let Some(message) = &check.message
        {
            emit(format!("  ✗ {}: {}", check.name, message));
        }
    }

    if verbose {
        for check in &report.checks {
            if check.status == CheckStatus::Warn
                && let Some(message) = &check.message
            {
                emit(format!("  ⚠ {}: {}", check.name, message));
            }
        }
    }
}

async fn run_command(
    config_sources: &[ConfigSource],
    verbose: bool,
    color_mode: ColorMode,
    args: RunArgs,
) -> Result<()> {
    // Partition sources: file/builtin/remote sources vs overrides
    let (primary_sources, overrides): (Vec<_>, Vec<_>) = config_sources
        .iter()
        .partition(|s| !matches!(s, ConfigSource::Override { .. }));

    // Warn if multiple config sources are specified
    if primary_sources.len() > 1 {
        warn!("Multiple config sources specified, using first one. Others ignored.");
    }

    // Load configuration based on first primary source, or default if only overrides
    let mut config = if let Some(source) = primary_sources.first() {
        match source {
            ConfigSource::File(path) => {
                if path.exists() {
                    RalphConfig::from_file(path)
                        .with_context(|| format!("Failed to load config from {:?}", path))?
                } else {
                    warn!("Config file {:?} not found, using defaults", path);
                    RalphConfig::default()
                }
            }
            ConfigSource::Builtin(name) => {
                let preset = presets::get_preset(name).ok_or_else(|| {
                    let available = presets::preset_names().join(", ");
                    anyhow::anyhow!(
                        "Unknown preset '{}'. Run `ralph run --list-presets` to see available presets.\n\nAvailable: {}",
                        name,
                        available
                    )
                })?;
                RalphConfig::parse_yaml(preset.content)
                    .with_context(|| format!("Failed to parse builtin preset '{}'", name))?
            }
            ConfigSource::Remote(url) => {
                info!("Fetching config from {}", url);
                let response = reqwest::get(url)
                    .await
                    .with_context(|| format!("Failed to fetch config from {}", url))?;

                if !response.status().is_success() {
                    anyhow::bail!(
                        "Failed to fetch config from {}: HTTP {}",
                        url,
                        response.status()
                    );
                }

                let content = response
                    .text()
                    .await
                    .with_context(|| format!("Failed to read config content from {}", url))?;

                RalphConfig::parse_yaml(&content)
                    .with_context(|| format!("Failed to parse config from {}", url))?
            }
            ConfigSource::Override { .. } => unreachable!("Partitioned out overrides"),
        }
    } else {
        // Only overrides specified - load default ralph.yml as base
        let default_path = PathBuf::from("ralph.yml");
        if default_path.exists() {
            RalphConfig::from_file(&default_path)
                .with_context(|| "Failed to load config from ralph.yml")?
        } else {
            warn!("Config file ralph.yml not found, using defaults");
            RalphConfig::default()
        }
    };

    // Normalize v1 flat fields into v2 nested structure
    config.normalize();

    // Set workspace_root to current directory (critical for E2E tests in isolated workspaces).
    // This must happen after config load because workspace_root has #[serde(skip)] and
    // defaults to cwd at deserialize time - but we need it set to the actual runtime cwd.
    config.core.workspace_root =
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    // Apply CLI config overrides (takes precedence over config file values)
    let override_sources: Vec<_> = overrides.into_iter().cloned().collect();
    apply_config_overrides(&mut config, &override_sources)?;

    // Handle --continue mode: check scratchpad exists before proceeding
    let resume = args.continue_mode;
    if resume {
        let scratchpad_path = std::path::Path::new(&config.core.scratchpad);
        if !scratchpad_path.exists() {
            anyhow::bail!(
                "Cannot continue: scratchpad not found at '{}'. \
                 Start a fresh run with `ralph run`.",
                config.core.scratchpad
            );
        }
        info!(
            "Found existing scratchpad at '{}', continuing from previous state",
            config.core.scratchpad
        );
    }

    // Apply CLI overrides (after normalization so they take final precedence)
    // Per spec: CLI -p and -P are mutually exclusive (enforced by clap)
    if let Some(text) = args.prompt_text {
        config.event_loop.prompt = Some(text);
        config.event_loop.prompt_file = String::new(); // Clear file path
    } else if let Some(path) = args.prompt_file {
        config.event_loop.prompt_file = path.to_string_lossy().to_string();
        config.event_loop.prompt = None; // Clear inline
    }
    if let Some(max_iter) = args.max_iterations {
        config.event_loop.max_iterations = max_iter;
    }
    if let Some(promise) = args.completion_promise {
        config.event_loop.completion_promise = promise;
    }
    if verbose {
        config.verbose = true;
    }

    // Apply execution mode overrides per spec
    // TUI is enabled by default (unless --no-tui is specified)
    if args.autonomous {
        config.cli.default_mode = "autonomous".to_string();
    } else if !args.no_tui {
        config.cli.default_mode = "interactive".to_string();
    }

    // Override idle timeout if specified
    if let Some(timeout) = args.idle_timeout {
        config.cli.idle_timeout_secs = timeout;
    }

    // Apply backend override from CLI (takes precedence over config)
    if let Some(backend) = args.backend {
        config.cli.backend = backend;
    }

    // Validate configuration and emit warnings
    let warnings = config
        .validate()
        .context("Configuration validation failed")?;
    for warning in &warnings {
        eprintln!("{warning}");
    }

    // Handle auto-detection if backend is "auto"
    if config.cli.backend == "auto" {
        let priority = config.get_agent_priority();
        let detected = detect_backend(&priority, |backend| {
            config.adapter_settings(backend).enabled
        });

        match detected {
            Ok(backend) => {
                info!("Auto-detected backend: {}", backend);
                config.cli.backend = backend;
            }
            Err(e) => {
                eprintln!("{e}");
                return Err(anyhow::Error::new(e));
            }
        }
    }

    let preflight_verbose = verbose || args.verbose;

    if args.dry_run {
        let preflight_report = run_auto_preflight(
            &config,
            args.skip_preflight,
            preflight_verbose,
            AutoPreflightMode::DryRun,
        )
        .await?;
        println!("Dry run mode - configuration:");
        println!(
            "  Hats: {}",
            if config.hats.is_empty() {
                "planner, builder (default)".to_string()
            } else {
                config.hats.keys().cloned().collect::<Vec<_>>().join(", ")
            }
        );

        // Show prompt source
        if let Some(ref inline) = config.event_loop.prompt {
            let preview = if inline.len() > 60 {
                format!("{}...", &inline[..60].replace('\n', " "))
            } else {
                inline.replace('\n', " ")
            };
            println!("  Prompt: inline text ({})", preview);
        } else {
            println!("  Prompt file: {}", config.event_loop.prompt_file);
        }

        println!(
            "  Completion promise: {}",
            config.event_loop.completion_promise
        );
        println!("  Max iterations: {}", config.event_loop.max_iterations);
        println!("  Max runtime: {}s", config.event_loop.max_runtime_seconds);
        println!("  Scratchpad: {}", config.core.scratchpad);
        println!("  Specs dir: {}", config.core.specs_dir);
        println!("  Backend: {}", config.cli.backend);
        println!("  Verbose: {}", config.verbose);
        // Execution mode info
        println!("  Default mode: {}", config.cli.default_mode);
        if config.cli.default_mode == "interactive" {
            println!("  Idle timeout: {}s", config.cli.idle_timeout_secs);
        }
        if !warnings.is_empty() {
            println!("  Warnings: {}", warnings.len());
        }
        if let Some(report) = preflight_report.as_ref() {
            print_preflight_summary(report, preflight_verbose, "  Preflight: ", true);
        }
        return Ok(());
    }

    // Ensure scratchpad directory exists (auto-create with depth limit)
    // This is done after dry-run check to avoid creating directories during dry-run
    ensure_scratchpad_directory(&config)?;

    // Get the prompt for lock metadata (short version for display)
    // When prompt_file is used, read its content for the summary instead of showing the file path
    let prompt_summary = config
        .event_loop
        .prompt
        .clone()
        .or_else(|| {
            let prompt_file = &config.event_loop.prompt_file;
            if prompt_file.is_empty() {
                None
            } else {
                let path = std::path::Path::new(prompt_file);
                if path.exists() {
                    std::fs::read_to_string(path).ok()
                } else {
                    None
                }
            }
        })
        .map(|p| truncate(&p, 100))
        .unwrap_or_else(|| "[no prompt]".to_string());

    let mut pending_worktree_registration: Option<LoopEntry> = None;

    // Try to acquire the loop lock for multi-loop concurrency support
    // This implements the lock detection flow from the multi-loop spec
    let workspace_root = &config.core.workspace_root;
    let (loop_context, _lock_guard) = match LoopLock::try_acquire(workspace_root, &prompt_summary) {
        Ok(guard) => {
            // We're the primary loop - run in place
            debug!("Acquired loop lock, running as primary loop");
            let context = LoopContext::primary(workspace_root.clone());
            (context, Some(guard))
        }
        Err(LockError::AlreadyLocked(existing)) => {
            // Another loop is running
            if args.exclusive {
                // --exclusive: wait for the lock instead of spawning worktree
                info!(
                    "Loop lock held by PID {} (started {}), waiting for lock (--exclusive mode)...",
                    existing.pid, existing.started
                );
                let guard = LoopLock::acquire_blocking(workspace_root, &prompt_summary)
                    .context("Failed to acquire loop lock in exclusive mode")?;
                debug!("Acquired loop lock after waiting");
                let context = LoopContext::primary(workspace_root.clone());
                (context, Some(guard))
            } else if !config.features.parallel {
                // Parallel loops disabled via config - error out
                anyhow::bail!(
                    "Another loop is already running (PID {}, prompt: \"{}\"). \
                    Parallel loops are disabled in config (features.parallel: false). \
                    Use --exclusive to wait for the lock, or enable parallel loops.",
                    existing.pid,
                    existing.prompt.chars().take(50).collect::<String>()
                );
            } else {
                // Auto-spawn into worktree
                info!(
                    "Loop lock held by PID {} ({}), spawning parallel loop in worktree",
                    existing.pid,
                    existing.prompt.chars().take(50).collect::<String>()
                );

                let worktree_config = WorktreeConfig::default();

                // Generate memorable loop ID (adjective-noun only, no prompt keywords)
                // This ID will be used consistently for: registry ID, worktree path, and branch name
                let name_generator =
                    ralph_core::LoopNameGenerator::from_config(&config.features.loop_naming);
                let loop_id = name_generator.generate_memorable_unique(|name| {
                    ralph_core::worktree_exists(workspace_root, name, &worktree_config)
                });

                // Ensure worktree directory is in .gitignore
                ensure_gitignore(workspace_root, ".worktrees")
                    .context("Failed to update .gitignore for worktrees")?;

                // Create the worktree
                let worktree = create_worktree(workspace_root, &loop_id, &worktree_config)
                    .context("Failed to create worktree for parallel loop")?;

                info!(
                    "Created worktree at {} on branch {}",
                    worktree.path.display(),
                    worktree.branch
                );

                // Create loop context for the worktree
                let context = LoopContext::worktree(
                    loop_id.clone(),
                    worktree.path.clone(),
                    workspace_root.clone(),
                );

                // Set up all worktree symlinks (memories, specs, code tasks)
                context
                    .setup_worktree_symlinks()
                    .context("Failed to create symlinks in worktree")?;

                // Generate context file with worktree metadata
                context
                    .generate_context_file(&worktree.branch, &prompt_summary)
                    .context("Failed to generate context file in worktree")?;

                // Register this loop after preflight succeeds so failed runs
                // don't leave stale registry entries behind.
                let entry = LoopEntry::with_id(
                    &loop_id,
                    &prompt_summary,
                    Some(worktree.path.to_string_lossy().to_string()),
                    worktree.path.to_string_lossy().to_string(),
                );
                pending_worktree_registration = Some(entry);

                // Update config to use worktree paths
                // The scratchpad and other paths should resolve to the worktree
                // Note: We keep the lock guard as None since worktree loops don't hold the primary lock

                (context, None)
            }
        }
        Err(LockError::UnsupportedPlatform) => {
            // Non-Unix: just run without locking (single-loop fallback)
            warn!("Loop locking not supported on this platform, running without lock");
            let context = LoopContext::primary(workspace_root.clone());
            (context, None)
        }
        Err(e) => {
            return Err(anyhow::Error::new(e).context("Failed to acquire loop lock"));
        }
    };

    // Update workspace_root in config if running in worktree
    if !loop_context.is_primary() {
        config.core.workspace_root = loop_context.workspace().to_path_buf();
        // Also update scratchpad path to use worktree location
        config.core.scratchpad = loop_context.scratchpad_path().to_string_lossy().to_string();
        debug!(
            "Running in worktree: workspace={}, scratchpad={}",
            config.core.workspace_root.display(),
            config.core.scratchpad
        );
    }

    // Ensure directories exist in the loop context
    loop_context
        .ensure_directories()
        .context("Failed to create loop directories")?;

    if let Err(err) = run_auto_preflight(
        &config,
        args.skip_preflight,
        preflight_verbose,
        AutoPreflightMode::Run,
    )
    .await
    {
        if !loop_context.is_primary()
            && let Err(clean_err) =
                remove_worktree(loop_context.repo_root(), loop_context.workspace())
        {
            warn!(
                "Preflight failed; unable to remove worktree {}: {}",
                loop_context.workspace().display(),
                clean_err
            );
        }
        return Err(err);
    }

    if let Some(entry) = pending_worktree_registration {
        let registry = LoopRegistry::new(loop_context.repo_root());
        registry
            .register(entry)
            .context("Failed to register loop in registry")?;
    }

    // Run the orchestration loop and exit with proper exit code
    // TUI is enabled by default (unless --no-tui or --autonomous is specified)
    let enable_tui = !args.no_tui && !args.autonomous;
    let verbosity = Verbosity::resolve(verbose || args.verbose, args.quiet);
    let custom_args = args.custom_args;
    // --no-auto-merge CLI flag overrides config.features.auto_merge
    let auto_merge_override = if args.no_auto_merge {
        Some(false)
    } else {
        None
    };
    let workspace_root = config.core.workspace_root.clone();
    let reason = loop_runner::run_loop_impl(
        config,
        color_mode,
        resume,
        enable_tui,
        verbosity,
        args.record_session,
        Some(loop_context),
        custom_args,
        auto_merge_override,
    )
    .await?;

    // Handle restart: exec-replace current process with same CLI args
    if matches!(reason, TerminationReason::RestartRequested) {
        let restart_path = std::path::Path::new(&workspace_root).join(".ralph/restart-requested");
        let _ = std::fs::remove_file(&restart_path);
        info!("Restart requested — exec-replacing process");

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let args: Vec<String> = std::env::args().collect();
            let err = std::process::Command::new(&args[0]).args(&args[1..]).exec();
            // exec() only returns on error
            anyhow::bail!("Failed to exec-replace process: {}", err);
        }

        #[cfg(not(unix))]
        {
            anyhow::bail!("Restart via exec-replace is only supported on Unix");
        }
    }

    let exit_code = reason.exit_code();

    // Use explicit exit for non-zero codes to ensure proper exit status
    if exit_code != 0 {
        std::process::exit(exit_code);
    }

    Ok(())
}

/// Resume a previously interrupted loop from existing scratchpad.
///
/// DEPRECATED: Use `ralph run --continue` instead.
///
/// Per spec: "When loop terminates due to safeguard (not completion promise),
/// user can run `ralph run --continue` to restart reading existing scratchpad,
/// continuing from where it left off."
async fn resume_command(
    config_sources: &[ConfigSource],
    verbose: bool,
    color_mode: ColorMode,
    args: ResumeArgs,
) -> Result<()> {
    // Show deprecation warning
    eprintln!(
        "{}warning:{} `ralph resume` is deprecated. Use `ralph run --continue` instead.",
        colors::YELLOW,
        colors::RESET
    );

    // Load config with overrides applied
    let mut config = load_config_with_overrides(config_sources)?;

    // Check that scratchpad exists (required for resume)
    let scratchpad_path = std::path::Path::new(&config.core.scratchpad);
    if !scratchpad_path.exists() {
        anyhow::bail!(
            "Cannot continue: scratchpad not found at '{}'. \
             Start a fresh run with `ralph run`.",
            config.core.scratchpad
        );
    }

    info!(
        "Found existing scratchpad at '{}', continuing from previous state",
        config.core.scratchpad
    );

    // Apply CLI overrides
    if let Some(max_iter) = args.max_iterations {
        config.event_loop.max_iterations = max_iter;
    }
    if verbose {
        config.verbose = true;
    }

    // Apply execution mode overrides
    // TUI is enabled by default (unless --no-tui is specified)
    if args.autonomous {
        config.cli.default_mode = "autonomous".to_string();
    } else if !args.no_tui {
        config.cli.default_mode = "interactive".to_string();
    }

    // Override idle timeout if specified
    if let Some(timeout) = args.idle_timeout {
        config.cli.idle_timeout_secs = timeout;
    }

    // Validate configuration
    let warnings = config
        .validate()
        .context("Configuration validation failed")?;
    for warning in &warnings {
        eprintln!("{warning}");
    }

    // Handle auto-detection if backend is "auto"
    if config.cli.backend == "auto" {
        let priority = config.get_agent_priority();
        let detected = detect_backend(&priority, |backend| {
            config.adapter_settings(backend).enabled
        });

        match detected {
            Ok(backend) => {
                info!("Auto-detected backend: {}", backend);
                config.cli.backend = backend;
            }
            Err(e) => {
                eprintln!("{e}");
                return Err(anyhow::Error::new(e));
            }
        }
    }

    // Run the orchestration loop in resume mode
    // The key difference: we publish task.resume instead of task.start,
    // signaling the planner to read the existing scratchpad
    // TUI is enabled by default (unless --no-tui or --autonomous is specified)
    let enable_tui = !args.no_tui && !args.autonomous;
    let verbosity = Verbosity::resolve(verbose || args.verbose, args.quiet);
    let reason = loop_runner::run_loop_impl(
        config,
        color_mode,
        true,
        enable_tui,
        verbosity,
        args.record_session,
        None,       // Deprecated resume command doesn't have loop_context
        Vec::new(), // Resume command doesn't support custom args
        None,       // Use config.features.auto_merge (deprecated command)
    )
    .await?;
    let exit_code = reason.exit_code();

    if exit_code != 0 {
        std::process::exit(exit_code);
    }

    Ok(())
}

fn init_command(color_mode: ColorMode, args: InitArgs) -> Result<()> {
    let use_colors = color_mode.should_use_colors();

    // Handle --list-presets
    if args.list_presets {
        println!("{}", init::format_preset_list());
        return Ok(());
    }

    // Handle --preset (with optional --backend override)
    if let Some(preset) = args.preset {
        let backend_override = args.backend.as_deref();
        match init::init_from_preset(&preset, backend_override, args.force) {
            Ok(()) => {
                let msg = if let Some(backend) = backend_override {
                    format!(
                        "Created ralph.yml from '{}' preset with {} backend",
                        preset, backend
                    )
                } else {
                    format!("Created ralph.yml from '{}' preset", preset)
                };
                if use_colors {
                    println!("{}✓{} {}", colors::GREEN, colors::RESET, msg);
                    println!(
                        "\n{}Next steps:{}\n  1. Create PROMPT.md with your task\n  2. Run: ralph run",
                        colors::DIM,
                        colors::RESET
                    );
                } else {
                    println!("{}", msg);
                    println!(
                        "\nNext steps:\n  1. Create PROMPT.md with your task\n  2. Run: ralph run"
                    );
                }
                return Ok(());
            }
            Err(e) => {
                anyhow::bail!("{}", e);
            }
        }
    }

    // Handle --backend alone (minimal config)
    if let Some(backend) = args.backend {
        match init::init_from_backend(&backend, args.force) {
            Ok(()) => {
                if use_colors {
                    println!(
                        "{}✓{} Created ralph.yml with {} backend",
                        colors::GREEN,
                        colors::RESET,
                        backend
                    );
                    println!(
                        "\n{}Next steps:{}\n  1. Create PROMPT.md with your task\n  2. Run: ralph run",
                        colors::DIM,
                        colors::RESET
                    );
                } else {
                    println!("Created ralph.yml with {} backend", backend);
                    println!(
                        "\nNext steps:\n  1. Create PROMPT.md with your task\n  2. Run: ralph run"
                    );
                }
                return Ok(());
            }
            Err(e) => {
                anyhow::bail!("{}", e);
            }
        }
    }

    // No flag specified - show help
    println!("Initialize a new ralph.yml configuration file.\n");
    println!("Usage:");
    println!("  ralph init --backend <backend>   Generate minimal config for backend");
    println!("  ralph init --preset <preset>     Use an embedded preset");
    println!("  ralph init --list-presets        Show available presets\n");
    println!("Backends: claude, kiro, gemini, codex, amp, custom");
    println!("\nRun 'ralph init --list-presets' to see available presets.");

    Ok(())
}

fn events_command(color_mode: ColorMode, args: EventsArgs) -> Result<()> {
    let use_colors = color_mode.should_use_colors();

    // Read events path from marker file, fall back to default if marker doesn't exist
    // This ensures `ralph events` reads from the same events file as the active run
    let history = match args.file {
        Some(path) => EventHistory::new(path),
        None => fs::read_to_string(".ralph/current-events")
            .map(|s| EventHistory::new(s.trim()))
            .unwrap_or_else(|_| EventHistory::default_path()),
    };

    // Handle clear command
    if args.clear {
        history.clear()?;
        if use_colors {
            println!("{}✓{} Event history cleared", colors::GREEN, colors::RESET);
        } else {
            println!("Event history cleared");
        }
        return Ok(());
    }

    if !history.exists() {
        if use_colors {
            println!(
                "{}No event history found.{} Run `ralph` to generate events.",
                colors::DIM,
                colors::RESET
            );
        } else {
            println!("No event history found. Run `ralph` to generate events.");
        }
        return Ok(());
    }

    // Read and filter events
    let mut records = history.read_all()?;

    // Apply filters in sequence
    if let Some(ref topic) = args.topic {
        records.retain(|r| r.topic == *topic);
    }

    if let Some(iteration) = args.iteration {
        records.retain(|r| r.iteration == iteration);
    }

    // Apply 'last' filter after other filters (to get last N of filtered results)
    if let Some(n) = args.last
        && records.len() > n
    {
        records = records.into_iter().rev().take(n).rev().collect();
    }

    if records.is_empty() {
        if use_colors {
            println!("{}No matching events found.{}", colors::DIM, colors::RESET);
        } else {
            println!("No matching events found.");
        }
        return Ok(());
    }

    match args.format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&records)?;
            println!("{json}");
        }
        OutputFormat::Table => {
            display::print_events_table(&records, use_colors);
        }
    }

    Ok(())
}

fn clean_command(
    config_sources: &[ConfigSource],
    color_mode: ColorMode,
    args: CleanArgs,
) -> Result<()> {
    let use_colors = color_mode.should_use_colors();

    // If --diagnostics flag is set, clean diagnostics directory
    if args.diagnostics {
        let workspace_root = std::env::current_dir().context("Failed to get current directory")?;
        return ralph_cli::clean_diagnostics(&workspace_root, use_colors, args.dry_run);
    }

    // Load config with overrides applied
    let config = load_config_with_overrides(config_sources)?;

    // Extract the .agent directory path from scratchpad path
    let scratchpad_path = Path::new(&config.core.scratchpad);
    let agent_dir = scratchpad_path.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "Could not determine parent directory from scratchpad path: {}",
            config.core.scratchpad
        )
    })?;

    // Check if directory exists
    if !agent_dir.exists() {
        // Not an error - just inform user
        if use_colors {
            println!(
                "{}Nothing to clean:{} Directory '{}' does not exist",
                colors::DIM,
                colors::RESET,
                agent_dir.display()
            );
        } else {
            println!(
                "Nothing to clean: Directory '{}' does not exist",
                agent_dir.display()
            );
        }
        return Ok(());
    }

    // Dry run mode - list what would be deleted
    if args.dry_run {
        if use_colors {
            println!(
                "{}Dry run mode:{} Would delete directory and all contents:",
                colors::CYAN,
                colors::RESET
            );
        } else {
            println!("Dry run mode: Would delete directory and all contents:");
        }
        println!("  {}", agent_dir.display());

        // List directory contents
        list_directory_contents(agent_dir, use_colors, 1)?;

        return Ok(());
    }

    // Perform actual deletion
    fs::remove_dir_all(agent_dir).with_context(|| {
        format!(
            "Failed to delete directory '{}'. Check permissions and try again.",
            agent_dir.display()
        )
    })?;

    // Success message
    if use_colors {
        println!(
            "{}✓{} Cleaned: Deleted '{}' and all contents",
            colors::GREEN,
            colors::RESET,
            agent_dir.display()
        );
    } else {
        println!(
            "Cleaned: Deleted '{}' and all contents",
            agent_dir.display()
        );
    }

    Ok(())
}

/// Emit an event to the current run's events file with proper JSON formatting.
///
/// This command provides a deterministic way for agents to emit events without
/// risking malformed JSONL from manual echo commands. All JSON serialization
/// is handled via serde_json, ensuring proper escaping of payloads.
///
/// Events are written to the path specified in `.ralph/current-events` marker file
/// (created by `ralph run`), or falls back to `.ralph/events.jsonl` if no marker exists.
fn emit_command(color_mode: ColorMode, args: EmitArgs) -> Result<()> {
    let use_colors = color_mode.should_use_colors();

    // Generate timestamp if not provided
    let ts = args.ts.unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    // Validate JSON payload if --json flag is set
    let payload = if args.json && !args.payload.is_empty() {
        // Validate it's valid JSON
        serde_json::from_str::<serde_json::Value>(&args.payload).context("Invalid JSON payload")?;
        args.payload
    } else {
        args.payload
    };

    // Build the event record
    // We use serde_json directly to ensure proper escaping
    let record = serde_json::json!({
        "topic": args.topic,
        "payload": if args.json && !payload.is_empty() {
            // Parse and embed as object
            serde_json::from_str::<serde_json::Value>(&payload)?
        } else if payload.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::String(payload)
        },
        "ts": ts
    });

    // Read events path from marker file, fall back to CLI arg if marker doesn't exist
    // This ensures `ralph emit` writes to the same events file as the active run
    let events_file = fs::read_to_string(".ralph/current-events")
        .map(|s| PathBuf::from(s.trim()))
        .unwrap_or_else(|_| args.file.clone());

    // Ensure parent directory exists
    if let Some(parent) = events_file.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    // Append to file
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&events_file)
        .with_context(|| format!("Failed to open events file: {}", events_file.display()))?;

    // Write as single-line JSON (JSONL format)
    let json_line = serde_json::to_string(&record)?;
    writeln!(file, "{}", json_line)?;

    // Success message
    if use_colors {
        println!(
            "{}✓{} Event emitted: {}",
            colors::GREEN,
            colors::RESET,
            args.topic
        );
    } else {
        println!("Event emitted: {}", args.topic);
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
struct TutorialStep {
    title: &'static str,
    body: &'static [&'static str],
}

const TUTORIAL_STEPS: &[TutorialStep] = &[
    TutorialStep {
        title: "Hats: Event-driven personas",
        body: &[
            "Hats are named personas that subscribe to events and publish new events.",
            "Each hat lists triggers (ex: task.start) and outputs (ex: build.task).",
            "Inspect hats with: ralph hats list",
            "Visualize the flow with: ralph hats graph --format ascii",
        ],
    },
    TutorialStep {
        title: "Presets: Packaged workflows",
        body: &[
            "Presets bundle hats, backend, and defaults into a single config.",
            "List built-ins with: ralph init --list-presets",
            "Create a config: ralph init --preset <name>",
            "Run directly: ralph run -c builtin:<name>",
        ],
    },
    TutorialStep {
        title: "Workflow: The loop lifecycle",
        body: &[
            "Write a prompt file (ex: PROMPT.md) or pass --prompt/--prompt-file.",
            "Run: ralph run -P PROMPT.md or ralph run -p \"...\"",
            "Ralph emits task.start, hats process events, and the loop ends on done events.",
            "Artifacts live in .ralph/agent (scratchpad, tasks, memories).",
            "Check open tasks with: ralph tools task ready",
        ],
    },
];

fn tutorial_steps() -> &'static [TutorialStep] {
    TUTORIAL_STEPS
}

/// Runs the interactive tutorial walkthrough.
fn tutorial_command(color_mode: ColorMode, args: TutorialArgs) -> Result<()> {
    let use_colors = color_mode.should_use_colors();
    let interactive = !args.no_input && std::io::stdin().is_terminal();
    let steps = tutorial_steps();

    print_tutorial_intro(use_colors, interactive);

    for (index, step) in steps.iter().enumerate() {
        print_tutorial_step(index + 1, steps.len(), step, use_colors);
        if interactive && index + 1 < steps.len() {
            prompt_to_continue(use_colors)?;
        } else {
            println!();
        }
    }

    print_tutorial_outro(use_colors);
    Ok(())
}

fn print_tutorial_intro(use_colors: bool, interactive: bool) {
    if use_colors {
        println!(
            "{}{}Ralph Tutorial{}",
            colors::BOLD,
            colors::CYAN,
            colors::RESET
        );
        println!(
            "{}Interactive walkthrough of hats, presets, and workflow.{}",
            colors::DIM,
            colors::RESET
        );
    } else {
        println!("Ralph Tutorial");
        println!("Interactive walkthrough of hats, presets, and workflow.");
    }

    if !interactive {
        println!("Non-interactive mode: printing all steps.");
    }

    println!();
}

fn print_tutorial_step(index: usize, total: usize, step: &TutorialStep, use_colors: bool) {
    if use_colors {
        println!(
            "{}{}Step {}/{}: {}{}",
            colors::BOLD,
            colors::CYAN,
            index,
            total,
            step.title,
            colors::RESET
        );
    } else {
        println!("Step {}/{}: {}", index, total, step.title);
    }

    for line in step.body {
        println!("  - {}", line);
    }
}

fn prompt_to_continue(use_colors: bool) -> Result<()> {
    if use_colors {
        print!("{}Press Enter to continue...{}", colors::DIM, colors::RESET);
    } else {
        print!("Press Enter to continue...");
    }

    stdout().flush()?;
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .context("Failed to read input")?;
    println!();
    Ok(())
}

fn print_tutorial_outro(use_colors: bool) {
    if use_colors {
        println!(
            "{}Tutorial complete. Next: ralph init --list-presets, then ralph run.{}",
            colors::GREEN,
            colors::RESET
        );
    } else {
        println!("Tutorial complete. Next: ralph init --list-presets, then ralph run.");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Help command
// ─────────────────────────────────────────────────────────────────────────────

struct HelpTopic {
    name: &'static str,
    tagline: &'static str,
    why: &'static [&'static str],
    examples: &'static [&'static str],
}

const HELP_TOPICS: &[HelpTopic] = &[
    HelpTopic {
        name: "hats",
        tagline: "Event-driven personas that separate concerns",
        why: &[
            "Without hats, one agent tries to plan, build, review, and commit all at once.",
            "It loses focus, skips steps, and produces mediocre output. Hats force separation",
            "of concerns — the Builder never reviews its own code, the Reviewer never edits files.",
            "Each hat has its own system prompt, triggers (events it listens to), and outputs",
            "(events it publishes). This means hats compose like UNIX pipes.",
        ],
        examples: &[
            "# Minimal 2-hat ralph.yml:",
            "  hats:",
            "    builder:",
            "      triggers: [task.start, review.revision_needed]",
            "      publishes: [build.done]",
            "      instructions: \"Write code. Run tests. Publish build.done when green.\"",
            "    reviewer:",
            "      triggers: [build.done]",
            "      publishes: [review.approved, review.revision_needed]",
            "      instructions: \"Review diffs. Never edit files. Approve or request revision.\"",
            "",
            "# Inspect configured hats:",
            "  ralph hats list",
            "  ralph hats graph --format ascii",
        ],
    },
    HelpTopic {
        name: "presets",
        tagline: "Battle-tested workflow templates",
        why: &[
            "Writing a multi-hat workflow from scratch is error-prone. Presets are",
            "battle-tested YAML files that encode proven patterns — like starter",
            "templates for orchestration. You pick one, customize it, and run.",
        ],
        examples: &[
            "# List available presets:",
            "  ralph init --list-presets",
            "",
            "# Generate config from a preset:",
            "  ralph init --preset bugfix",
            "",
            "# Run directly from a built-in preset (no file needed):",
            "  ralph run -c builtin:code-assist",
        ],
    },
    HelpTopic {
        name: "memories",
        tagline: "Persistent knowledge across sessions",
        why: &[
            "Each loop iteration starts with fresh context — the agent forgets what it",
            "learned last time. Memories persist knowledge across sessions: codebase patterns",
            "it discovered, decisions it made and why, fixes for problems that recur. Without",
            "memories, the agent re-discovers the same things every run.",
        ],
        examples: &[
            "# Add a memory:",
            "  ralph tools memory add \"pattern\" \"Always run cargo fmt before committing\"",
            "",
            "# Search memories:",
            "  ralph tools memory search \"cargo\"",
            "",
            "# Prime memories into context at loop start:",
            "  ralph tools memory prime",
            "",
            "# Memory file: .ralph/agent/memories.md",
            "# Types: pattern, decision, fix, context",
        ],
    },
    HelpTopic {
        name: "events",
        tagline: "Decoupled communication between hats",
        why: &[
            "Events are how hats talk to each other without coupling. A Builder doesn't call",
            "the Reviewer — it publishes build.done and the Reviewer's trigger fires. This",
            "means you can add, remove, or reorder hats without rewriting instructions.",
            "Events also give you a complete audit trail of what happened and when.",
        ],
        examples: &[
            "# Emit an event manually:",
            "  ralph emit \"build.done\" \"tests pass\"",
            "",
            "# View recent events:",
            "  ralph events --last 5",
            "",
            "# Event XML format in agent output:",
            "  <ralph:event topic=\"build.done\">tests pass</ralph:event>",
            "",
            "# Triggers support glob matching:",
            "  triggers: [build.*, review.approved]",
        ],
    },
    HelpTopic {
        name: "loops",
        tagline: "Steering a running orchestration process",
        why: &[
            "Ralph loops run autonomously, but sometimes you need to course-correct",
            "mid-flight — the agent is heading down the wrong path, you want to add",
            "context it doesn't have, or you need to stop it gracefully. Loop interaction",
            "gives you a control plane for a running process without killing and restarting it.",
        ],
        examples: &[
            "# TUI guidance (while ralph is running in TUI mode):",
            "  Press : to queue guidance for the next iteration",
            "  Press ! to inject guidance immediately",
            "",
            "# Inject guidance via event:",
            "  ralph emit \"human.guidance\" \"focus on the API layer, skip the UI\"",
            "",
            "# Monitor loops:",
            "  ralph loops list",
            "  ralph loops logs <id> --follow",
            "",
            "# Graceful stop (terminates at next iteration boundary):",
            "  ralph loops stop",
            "",
            "# Resume after stop:",
            "  ralph run --continue",
            "",
            "# Telegram (remote steering):",
            "  Send messages to the bot; use /status, /stop, /tail",
            "",
            "# Parallel loops (worktree-isolated):",
            "  ralph loops list --all",
        ],
    },
];

// ─────────────────────────────────────────────────────────────────────────────
// Help --prompt: SDLC-aware prompt generator
// ─────────────────────────────────────────────────────────────────────────────

struct HelpPrompt {
    /// Scenario name used on CLI — matches preset name where applicable
    name: &'static str,
    /// Category for grouping in listing output
    category: &'static str,
    /// One-line description for the listing
    summary: &'static str,
    /// The full prompt text (valid markdown, no ANSI)
    body: &'static str,
}

const HELP_PROMPT_CATEGORIES: &[&str] = &[
    "End-to-End",
    "Implementation",
    "Quality",
    "Fixing & Debugging",
    "Maintenance",
    "Operations",
];

const HELP_PROMPTS: &[HelpPrompt] = &[
    // ── End-to-End ──────────────────────────────────────────────────────────
    HelpPrompt {
        name: "pdd-to-code-assist",
        category: "End-to-End",
        summary: "Full autonomous pipeline from idea to committed code",
        body: "\
# Using Ralph: pdd-to-code-assist

## What This Does
The full autonomous pipeline from rough idea to committed code. Nine specialized hats
handle requirements gathering, architecture, design review, codebase exploration,
planning, task generation, TDD implementation, validation, and committing — in sequence.
No hat does more than one job. This is Ralph's most comprehensive workflow.

## Quick Start
1. `ralph init --preset pdd-to-code-assist` (or use inline: `-c builtin:pdd-to-code-assist`)
2. `ralph run -p \"Build a REST API for user management with JWT auth\"`
3. Ralph handles everything: requirements Q&A → design → research → plan → tasks → TDD → commit

## Hat Flow
Inquisitor (requirements Q&A) → Architect (design doc) → Design Critic (approve/reject) →
Explorer (codebase research) → Planner (implementation plan) → Task Writer (.code-task.md files) →
Builder (TDD: red/green/refactor) → Validator (full test suite) → Committer (atomic commits)

Events: design.start → requirements.complete → design.drafted → design.approved →
context.ready → plan.ready → tasks.ready → implementation.ready →
validation.passed → commit.complete → LOOP_COMPLETE

## Steering
- Press `:` in TUI to add context (\"use Postgres, not SQLite\" or \"skip the admin endpoints\")
- `ralph emit \"human.guidance\" \"the auth module is in src/auth/, follow its patterns\"`
- If design is wrong: stop early, edit specs/{task}/design.md, resume with `ralph run --continue`

## Anti-Patterns
- Don't provide an implementation plan in the prompt — let the Inquisitor/Architect discover it
- Don't micro-manage — the 9-hat pipeline has built-in quality gates at every transition
- Don't use this for small fixes — it's heavyweight; use `bugfix` or `code-assist` instead",
    },
    // ── Implementation ──────────────────────────────────────────────────────
    HelpPrompt {
        name: "code-assist",
        category: "Implementation",
        summary: "Flexible TDD implementation from any starting point",
        body: "\
# Using Ralph: code-assist

## What This Does
A flexible TDD implementation workflow that auto-detects your starting point — a PDD
output directory, a .code-task.md file, or a plain description. Four hats handle planning,
building (red/green/refactor TDD), validation, and committing.

## Quick Start
1. `ralph run -c builtin:code-assist -p \"Implement the user auth module from specs/user-auth/\"`
2. Or from a task file: `ralph run -c builtin:code-assist -p \"Implement specs/user-auth/tasks/task-01-models.code-task.md\"`
3. Or ad-hoc: `ralph run -c builtin:code-assist -p \"Add input validation to the signup form\"`

## Hat Flow
Planner (detect input, create tasks) → Builder (TDD cycle per task) →
Validator (full test suite) → Committer (atomic commit)

Events: build.start → tasks.ready → implementation.ready → validation.passed → commit.complete → LOOP_COMPLETE

## Steering
- Press `:` in TUI to refine scope (\"skip the edge case tests for now, focus on happy path\")
- `ralph loops stop` to pause; `ralph run --continue` to resume

## Anti-Patterns
- Don't mix multiple unrelated features in one prompt — one logical unit per run
- Don't skip the Planner by providing implementation details — let it decompose the work",
    },
    HelpPrompt {
        name: "feature",
        category: "Implementation",
        summary: "Build a feature with integrated code review",
        body: "\
# Using Ralph: feature

## What This Does
A lightweight two-hat workflow: Builder implements, Reviewer reviews. The Builder can't
review its own code and the Reviewer can't edit files. This separation catches issues
that a single-agent approach misses.

## Quick Start
1. `ralph run -c builtin:feature -p \"Add a /users/:id endpoint that returns user profile data\"`
2. Ralph builds, then reviews, iterating until the Reviewer approves

## Hat Flow
Builder → Reviewer (approve or request changes → Builder again)

Events: build.task → build.done → review.request → review.approved → LOOP_COMPLETE

## Steering
- Press `:` in TUI to add context (\"follow the pattern in src/routes/posts.rs\")
- `ralph emit \"human.guidance\" \"prioritize error handling over edge cases\"`
- `ralph loops stop` to pause; `ralph run --continue` to resume

## Anti-Patterns
- Don't use for large features — use `pdd-to-code-assist` or `code-assist` with task decomposition
- Don't override the Reviewer's findings — they exist to catch what the Builder missed",
    },
    HelpPrompt {
        name: "spec-driven",
        category: "Implementation",
        summary: "Specification-first development pipeline",
        body: "\
# Using Ralph: spec-driven

## What This Does
Contract-first development: write a spec with Given-When-Then acceptance criteria,
critique it for completeness, implement exactly to spec, then verify every criterion.
Four hats ensure the spec is solid before any code is written.

## Quick Start
1. `ralph run -c builtin:spec-driven -p \"Build a rate limiter: 100 req/min per API key, sliding window\"`
2. Ralph writes the spec, reviews it, implements, then verifies against each criterion

## Hat Flow
Spec Writer → Spec Reviewer (approve/reject) → Implementer → Verifier (pass or spec violation → Implementer)

Events: spec.start → spec.ready → spec.approved → implementation.done → task.complete

## Steering
- Press `:` to refine requirements (\"add a criterion for burst handling\")
- If spec is rejected too many times: stop, edit the spec directly, resume

## Anti-Patterns
- Don't provide implementation hints — the spec is the contract, implementation follows
- Don't skip the Spec Reviewer — catching ambiguity before code saves iteration cycles",
    },
    // ── Quality ─────────────────────────────────────────────────────────────
    HelpPrompt {
        name: "review",
        category: "Quality",
        summary: "Code review without modifications",
        body: "\
# Using Ralph: review

## What This Does
Read-only code review. Two hats — Reviewer and Deep Analyzer — examine code without
modifying it. The Reviewer identifies issues section by section, the Analyzer dives
deeper on flagged areas. Output is structured feedback (Critical/Suggestions/Nitpicks).

## Quick Start
1. `ralph run -c builtin:review -p \"Review src/auth/ for security and correctness\"`
2. Ralph reads, analyzes, and produces a review — no files are modified

## Hat Flow
Reviewer (section-by-section review) → Analyzer (deep dive on flagged areas) → Reviewer (synthesize)

Events: review.start → review.section → analysis.complete → review.complete → REVIEW_COMPLETE

## Steering
- Press `:` to focus the review (\"focus on the JWT validation logic, skip the middleware\")

## Anti-Patterns
- Don't expect code changes — this is read-only; use `feature` or `code-assist` for fixes
- Don't review the entire codebase at once — scope to a module or directory",
    },
    HelpPrompt {
        name: "pr-review",
        category: "Quality",
        summary: "Multi-perspective pull request review",
        body: "\
# Using Ralph: pr-review

## What This Does
Reviews a pull request from three independent perspectives — correctness, security, and
architecture — then synthesizes a unified verdict. Each reviewer hat is isolated: the
security reviewer doesn't care about style, the architecture reviewer doesn't nitpick bugs.

## Quick Start
1. `ralph run -c builtin:pr-review -p \"Review PR #42: adds OAuth2 login flow\"`
2. Ralph produces three independent reviews, then a synthesized APPROVE or REQUEST_CHANGES

## Hat Flow
Correctness Reviewer + Security Reviewer + Architecture Reviewer → Synthesizer (unified verdict)

Events: review.correctness → correctness.done, review.security → security.done,
review.architecture → architecture.done → synthesis.request → review.complete → LOOP_COMPLETE

## Steering
- Press `:` to add context (\"this is a security-sensitive endpoint, weight security review higher\")

## Anti-Patterns
- Don't ask it to also fix the issues — this is review-only; file separate bugfix/feature runs
- Don't skip the synthesis — individual reviews may conflict; the Synthesizer resolves them",
    },
    HelpPrompt {
        name: "fresh-eyes",
        category: "Quality",
        summary: "Repeated self-review enforcement (min 3 passes)",
        body: "\
# Using Ralph: fresh-eyes

## What This Does
Builds a feature then forces a minimum of 3 fresh-eyes review passes. Each audit pass
starts with fresh context, reviewing as if seeing the code for the first time. A Gatekeeper
requires 2 consecutive clean passes before approving. Hard stop at 8 passes.

## Quick Start
1. `ralph run -c builtin:fresh-eyes -p \"Implement the payment processing module\"`
2. Ralph builds, then audits repeatedly until 2 consecutive clean passes or 8 total

## Hat Flow
Builder → Fresh Eyes Auditor (min 3 passes) → Gatekeeper (2 consecutive clean? → done or continue)

Events: fresh_eyes.start → build.complete → fresh_eyes.continue (loop) → fresh_eyes.done → LOOP_COMPLETE

## Steering
- Press `:` to guide the auditor (\"pay extra attention to error handling in the webhook path\")
- If stuck in audit loop: `ralph loops stop`, review findings, fix manually, resume

## Anti-Patterns
- Don't use for trivial changes — the 3-pass minimum is overhead for simple fixes
- Don't increase the pass limit beyond 8 — if it hasn't stabilized by then, the scope is too large",
    },
    HelpPrompt {
        name: "gap-analysis",
        category: "Quality",
        summary: "Deep comparison of specs against implementation",
        body: "\
# Using Ralph: gap-analysis

## What This Does
Compares specifications against implementation to find gaps. Three hats coordinate:
Analyzer reads specs and code, Verifier checks specific claims, Reporter produces a
categorized ISSUES.md (Critical/Missing/Undocumented/Improvements).

## Quick Start
1. `ralph run -c builtin:gap-analysis -p \"Compare specs/auth/ against src/auth/ implementation\"`
2. Ralph produces ISSUES.md with categorized gaps

## Hat Flow
Analyzer (coordinate) → Verifier (check specific gaps) → Reporter (categorized output)

Events: gap.start → analyze.spec → verify.complete → report.request → report.complete → GAP_ANALYSIS_COMPLETE

## Steering
- Press `:` to scope (\"focus on the authentication flow, skip authorization for now\")

## Anti-Patterns
- Don't expect fixes — this is analysis-only; use findings to create bugfix or feature runs
- Don't skip providing spec paths — the Analyzer needs both spec and code to compare",
    },
    // ── Fixing & Debugging ──────────────────────────────────────────────────
    HelpPrompt {
        name: "bugfix",
        category: "Fixing & Debugging",
        summary: "Scientific method: reproduce → fix → verify → commit",
        body: "\
# Using Ralph: bugfix

## What This Does
Enforces the scientific method for bug fixing: reproduce with a failing test first,
then fix, then verify. Four hats ensure the Reproducer never fixes, the Fixer never
skips reproduction, and the Verifier catches regressions.

## Quick Start
1. `ralph run -c builtin:bugfix -p \"Fix: [describe bug and reproduction steps]\"`
2. Ralph creates a failing test, fixes the code, verifies, and commits

## Hat Flow
Reproducer → Fixer → Verifier → Committer

Events: repro.start → repro.complete → fix.complete → verification.passed → LOOP_COMPLETE

If verification fails: verification.failed → Reproducer (re-examine)

## Steering
- Press `:` in TUI to add context (\"the bug is in the auth middleware, not the route handler\")
- `ralph emit \"human.guidance\" \"check error handling in src/api/auth.rs\"`
- `ralph loops stop` to pause; `ralph run --continue` to resume

## Anti-Patterns
- Don't write the fix yourself and ask Ralph to \"verify it\" — let the Reproducer find it
- Don't skip the failing test — it's the proof the bug existed and the regression gate
- Don't provide a multi-bug prompt — one bug per run, always",
    },
    HelpPrompt {
        name: "debug",
        category: "Fixing & Debugging",
        summary: "Hypothesis-driven bug investigation and root cause analysis",
        body: "\
# Using Ralph: debug

## What This Does
Systematic debugging through hypothesis testing. The Investigator forms hypotheses,
the Tester designs experiments to confirm or reject them, the Fixer applies the fix
once root cause is confirmed, and the Verifier ensures the fix holds. Prevents
guess-and-check debugging.

## Quick Start
1. `ralph run -c builtin:debug -p \"Debug: API returns 500 on POST /users with valid payload\"`
2. Ralph investigates, forms hypotheses, tests them, fixes the confirmed root cause

## Hat Flow
Investigator → Tester (confirm/reject hypothesis) → Fixer → Verifier

Events: debug.start → hypothesis.test → hypothesis.confirmed → fix.propose →
fix.applied → fix.verified → DEBUG_COMPLETE

If hypothesis rejected: hypothesis.rejected → Investigator (new hypothesis)

## Steering
- Press `:` to share observations (\"I noticed it only happens with Unicode usernames\")
- `ralph emit \"human.guidance\" \"the database logs show a constraint violation\"`

## Anti-Patterns
- Don't provide the fix in the prompt — let the Investigator discover root cause
- Don't use for known bugs with clear reproduction — use `bugfix` instead
- Don't combine with feature work — debug is investigation-only",
    },
    // ── Maintenance ─────────────────────────────────────────────────────────
    HelpPrompt {
        name: "refactor",
        category: "Maintenance",
        summary: "Safe incremental refactoring with verification",
        body: "\
# Using Ralph: refactor

## What This Does
Safe incremental refactoring: each step is atomic and leaves the codebase in a working
state. Two hats — Refactorer makes changes, Verifier confirms nothing broke. Frequent
git checkpoints (every 3 steps) so you can always roll back.

## Quick Start
1. `ralph run -c builtin:refactor -p \"Refactor: extract the validation logic from UserController into a ValidationService\"`
2. Ralph refactors in small verified steps, checkpointing along the way

## Hat Flow
Refactorer → Verifier (pass → next step or done, fail → Refactorer fixes)

Events: refactor.task → refactor.done → verify.passed → REFACTOR_COMPLETE

## Steering
- Press `:` to scope (\"only refactor the public API, leave internals for now\")
- `ralph loops stop` to pause; `ralph run --continue` to resume

## Anti-Patterns
- Don't combine refactoring with new features — refactor = same behavior, different structure
- Don't skip the Verifier — every step must leave tests green",
    },
    HelpPrompt {
        name: "research",
        category: "Maintenance",
        summary: "Deep exploration without code changes",
        body: "\
# Using Ralph: research

## What This Does
Pure exploration — reads code, analyzes patterns, produces findings — without modifying
any files. Two hats: Researcher explores and reports findings, Synthesizer consolidates
into actionable summaries with file:line references.

## Quick Start
1. `ralph run -c builtin:research -p \"Research: how does the event system route messages between hats?\"`
2. Ralph explores, produces findings with citations, then synthesizes

## Hat Flow
Researcher (explore, report findings) → Synthesizer (consolidate, follow up or complete)

Events: research.start → research.finding → research.followup (loop) → synthesis.complete → RESEARCH_COMPLETE

## Steering
- Press `:` to redirect (\"also look at how errors propagate through the event bus\")

## Anti-Patterns
- Don't expect code changes — this is read-only; use findings to plan implementation
- Don't ask for implementation recommendations — research observes, it doesn't prescribe",
    },
    HelpPrompt {
        name: "docs",
        category: "Maintenance",
        summary: "Documentation with writer/reviewer cycle",
        body: "\
# Using Ralph: docs

## What This Does
Documentation writing with a writer/reviewer cycle. The Writer drafts sections, the
Reviewer checks for accuracy, completeness, and clarity. Iterates until the Reviewer
approves. Ensures docs match the actual code.

## Quick Start
1. `ralph run -c builtin:docs -p \"Write API documentation for the /users endpoints in src/routes/users.rs\"`
2. Ralph writes, reviews, revises, and produces final documentation

## Hat Flow
Writer (draft sections) → Reviewer (approve or request revision → Writer again)

Events: write.section → write.done → review.done or review.revision → DOCS_COMPLETE

## Steering
- Press `:` to guide style (\"use the same format as docs/api/posts.md\")

## Anti-Patterns
- Don't provide the documentation text — let the Writer read the code and write from it
- Don't ask for code changes alongside docs — use a separate `feature` or `bugfix` run",
    },
    HelpPrompt {
        name: "deploy",
        category: "Maintenance",
        summary: "Deployment with validation and rollback",
        body: "\
# Using Ralph: deploy

## What This Does
Deployment workflow with built-in validation and automatic rollback. Three hats: Builder
prepares artifacts, Deployer executes deployment, Verifier runs post-deploy checks. If
verification fails, the Deployer can rollback.

## Quick Start
1. `ralph run -c builtin:deploy -p \"Deploy: release v2.1.0 to staging environment\"`
2. Ralph builds, deploys, verifies, and rolls back if needed

## Hat Flow
Builder (prepare) → Deployer (execute) → Verifier (post-deploy checks)

Events: build.task → build.done → deploy.ready → deploy.start → deploy.done →
verify.pass → LOOP_COMPLETE

If verification fails: verify.fail → deploy.rollback → deploy.failed

## Steering
- Press `:` to add context (\"skip the integration tests, just run smoke tests\")
- `ralph loops stop` for emergency stop

## Anti-Patterns
- Don't skip the Verifier — post-deploy validation is the safety net
- Don't combine deploy with feature work — deploy is release-only",
    },
    // ── Operations ──────────────────────────────────────────────────────────
    HelpPrompt {
        name: "steering",
        category: "Operations",
        summary: "Course-correct a running Ralph loop",
        body: "\
# Using Ralph: Steering a Running Loop

## What This Does
Ralph loops run autonomously, but you can course-correct mid-flight without
killing and restarting. This is the control plane for a running orchestration.

## Guidance Methods (least to most disruptive)
1. TUI queued: Press `:` → type guidance → queued for next iteration
2. TUI immediate: Press `!` → injected into current iteration
3. Event injection: `ralph emit \"human.guidance\" \"focus on X, skip Y\"`
4. Telegram: send message to bot (if RObot configured) for remote steering
5. Graceful stop: `ralph loops stop` → finishes current iteration, then exits

## Monitoring
- TUI shows live agent output, current hat, iteration count
- `ralph loops list` — see all running loops with status
- `ralph loops logs <id> --follow` — tail a specific loop
- `ralph events --last 10` — recent event history

## Recovery
- After `ralph loops stop`: `ralph run --continue` picks up from scratchpad/tasks
- Loop stuck: stop, review `.ralph/agent/scratchpad.md`, add memories, resume
- Wrong direction: stop, refine the prompt, resume

## Anti-Patterns
- Don't kill the process (Ctrl+C twice) — use graceful stop so state is preserved
- Don't edit files while Ralph is running — it will overwrite your changes
- Don't inject guidance every iteration — let the agent work, steer only when off-track",
    },
    HelpPrompt {
        name: "parallel",
        category: "Operations",
        summary: "Run multiple loops via worktree isolation",
        body: "\
# Using Ralph: Parallel Loops

## What This Does
Run multiple Ralph orchestration loops simultaneously using git worktrees for filesystem
isolation. Each loop gets its own branch and working directory. When a worktree loop
completes, it queues for merge back to the main workspace.

## Quick Start
1. Start the primary loop: `ralph run -p \"Add user authentication\"`
2. In another terminal: `ralph run -p \"Add logging middleware\"` (auto-creates worktree)
3. Monitor all loops: `ralph loops list --all`
4. Primary loop processes the merge queue when worktree loops complete

## Architecture
Primary Loop (holds .ralph/loop.lock)
├── Runs in main workspace
├── Processes merge queue on completion
└── Spawns merge-ralph for queued loops

Worktree Loops (.worktrees/<loop-id>/)
├── Isolated filesystem via git worktree
├── Symlinked memories, specs, tasks → main repo
├── Queue for merge on completion
└── Exit cleanly (no spawn)

## Steering
- `ralph loops list --all` — see all loops with status
- `ralph loops stop <id>` — stop a specific loop
- Messages default to the primary loop; use `@loop-id` prefix to target a worktree loop

## Anti-Patterns
- Don't run parallel loops that modify the same files — merge conflicts will block
- Don't run more parallel loops than your machine can handle — each spawns an agent process
- Don't skip the merge queue — let merge-ralph handle conflict resolution",
    },
    HelpPrompt {
        name: "memories",
        category: "Operations",
        summary: "Teach Ralph about your codebase patterns",
        body: "\
# Using Ralph: Memories

## What This Does
Each loop iteration starts with fresh context — the agent forgets what it learned.
Memories persist knowledge across sessions so Ralph doesn't re-discover the same things
every run. Four types: pattern, decision, fix, context.

## Memory Types
- **pattern**: Codebase conventions (\"All API handlers return Result<Json<T>, AppError>\")
- **decision**: Architectural choices (\"Chose JSONL over SQLite: simpler, git-friendly\")
- **fix**: Recurring problem solutions (\"ECONNREFUSED on :5432 means run docker-compose up\")
- **context**: Project-specific knowledge (\"The /legacy folder is deprecated, use /v2\")

## Commands
- `ralph tools memory add \"content\" -t pattern --tags api,conventions`
- `ralph tools memory search \"query\" -t pattern`
- `ralph tools memory list -t decision`
- `ralph tools memory prime --budget 2000` (inject into next iteration's context)
- `ralph tools memory show <id>` / `ralph tools memory delete <id>`

## Storage
Memories live in `.ralph/agent/memories.md`. They are automatically injected into
prompts when enabled (default). Each memory has an ID like `mem-1737372000-a1b2`.

## When to Create Memories
- After discovering a codebase pattern the agent keeps missing
- After making an architectural decision you want persisted
- After fixing a recurring problem
- Before a long run, to prime context about unfamiliar code

## Anti-Patterns
- Don't create memories for temporary state — use the scratchpad for that
- Don't create duplicate memories — search first
- Don't over-prime — respect the token budget, prioritize high-value memories",
    },
];

fn help_command(color_mode: ColorMode, args: HelpArgs) -> Result<()> {
    let use_colors = color_mode.should_use_colors();

    // --prompt takes priority
    if args.prompt {
        return match &args.topic {
            Some(name) => print_help_prompt(name),
            None => {
                print_help_prompts_list(use_colors);
                Ok(())
            }
        };
    }

    if args.verbose {
        return match &args.topic {
            Some(topic) => {
                let name = topic.to_lowercase();
                match HELP_TOPICS.iter().find(|t| t.name == name) {
                    Some(t) => {
                        print_help_topic(t, use_colors);
                        Ok(())
                    }
                    None => {
                        eprintln!(
                            "Unknown topic: {}. Available: {}",
                            topic,
                            HELP_TOPICS
                                .iter()
                                .map(|t| t.name)
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                        std::process::exit(1);
                    }
                }
            }
            None => {
                for (i, topic) in HELP_TOPICS.iter().enumerate() {
                    print_help_topic(topic, use_colors);
                    if i + 1 < HELP_TOPICS.len() {
                        println!();
                    }
                }
                Ok(())
            }
        };
    }

    // Default: concise help
    print_help_concise(use_colors);
    Ok(())
}

fn print_help_prompts_list(use_colors: bool) {
    if use_colors {
        println!(
            "{}{}Available prompts{} (ralph help --prompt <name>):\n",
            colors::BOLD,
            colors::CYAN,
            colors::RESET
        );
    } else {
        println!("Available prompts (ralph help --prompt <name>):\n");
    }

    for category in HELP_PROMPT_CATEGORIES {
        if use_colors {
            println!("  {}{}:{}", colors::BOLD, category, colors::RESET);
        } else {
            println!("  {}:", category);
        }

        for prompt in HELP_PROMPTS.iter().filter(|p| p.category == *category) {
            if use_colors {
                println!(
                    "    {}{}{:<24}{}  {}",
                    colors::BOLD,
                    colors::GREEN,
                    prompt.name,
                    colors::RESET,
                    prompt.summary
                );
            } else {
                println!("    {:<24}  {}", prompt.name, prompt.summary);
            }
        }
        println!();
    }

    if use_colors {
        println!("Usage: {}ralph help --prompt <name>{}", colors::BOLD, colors::RESET);
        println!("       {}ralph help --prompt <name> | pbcopy{}", colors::DIM, colors::RESET);
        println!("       {}ralph help --prompt <name> >> CLAUDE.md{}", colors::DIM, colors::RESET);
    } else {
        println!("Usage: ralph help --prompt <name>");
        println!("       ralph help --prompt <name> | pbcopy");
        println!("       ralph help --prompt <name> >> CLAUDE.md");
    }
}

fn print_help_prompt(name: &str) -> Result<()> {
    let name_lower = name.to_lowercase();
    match HELP_PROMPTS.iter().find(|p| p.name == name_lower) {
        Some(prompt) => {
            // TTY hint
            if std::io::stdout().is_terminal() {
                println!("# Paste this into your CLAUDE.md or use as a system prompt\n");
            }
            // Always plain text (no ANSI) for prompt bodies
            println!("{}", prompt.body);
            Ok(())
        }
        None => {
            eprintln!(
                "Unknown scenario: {}. Available: {}",
                name,
                HELP_PROMPTS
                    .iter()
                    .map(|p| p.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            std::process::exit(1);
        }
    }
}

fn print_help_concise(use_colors: bool) {
    if use_colors {
        println!(
            "{}{}Ralph Orchestrator{} — hat-based multi-agent workflows\n",
            colors::BOLD,
            colors::CYAN,
            colors::RESET
        );
    } else {
        println!("Ralph Orchestrator — hat-based multi-agent workflows\n");
    }

    let commands = [
        ("run", "Run the orchestration loop"),
        ("init", "Initialize ralph.yml from a preset"),
        ("hats", "Manage and inspect configured hats"),
        ("events", "View event history"),
        ("loops", "Manage and monitor parallel loops"),
        ("tools", "Runtime tools (memory, task, skill)"),
        ("tutorial", "Interactive walkthrough"),
        ("doctor", "Environment diagnostics"),
    ];

    if use_colors {
        println!("{}Commands:{}", colors::BOLD, colors::RESET);
    } else {
        println!("Commands:");
    }

    for (name, desc) in &commands {
        if use_colors {
            println!(
                "  {}{}  {:<12}{}  {}",
                colors::BOLD,
                colors::GREEN,
                name,
                colors::RESET,
                desc
            );
        } else {
            println!("  {:<12}  {}", name, desc);
        }
    }

    println!();
    if use_colors {
        println!(
            "Use {}ralph help -v{} for detailed help with examples.",
            colors::BOLD, colors::RESET
        );
        println!(
            "Use {}ralph help -v <topic>{} for a specific topic: {}.",
            colors::BOLD,
            colors::RESET,
            HELP_TOPICS
                .iter()
                .map(|t| t.name)
                .collect::<Vec<_>>()
                .join(", ")
        );
        println!(
            "Use {}ralph help --prompt{} for LLM-ready prompts per workflow.",
            colors::BOLD, colors::RESET
        );
    } else {
        println!("Use ralph help -v for detailed help with examples.");
        println!(
            "Use ralph help -v <topic> for a specific topic: {}.",
            HELP_TOPICS
                .iter()
                .map(|t| t.name)
                .collect::<Vec<_>>()
                .join(", ")
        );
        println!("Use ralph help --prompt for LLM-ready prompts per workflow.");
    }
}

fn print_help_topic(topic: &HelpTopic, use_colors: bool) {
    // Header
    if use_colors {
        println!(
            "{}{}━━━ {} ━━━{}",
            colors::BOLD,
            colors::CYAN,
            topic.name.to_uppercase(),
            colors::RESET
        );
        println!(
            "{}{}{}",
            colors::DIM,
            topic.tagline,
            colors::RESET
        );
    } else {
        println!("━━━ {} ━━━", topic.name.to_uppercase());
        println!("{}", topic.tagline);
    }
    println!();

    // Why section
    if use_colors {
        println!("{}Why:{}", colors::BOLD, colors::RESET);
    } else {
        println!("Why:");
    }
    for line in topic.why {
        println!("  {}", line);
    }
    println!();

    // Examples section
    if use_colors {
        println!("{}Examples:{}", colors::BOLD, colors::RESET);
    } else {
        println!("Examples:");
    }
    for line in topic.examples {
        if use_colors && line.starts_with('#') {
            println!(
                "  {}{}{}",
                colors::DIM,
                line,
                colors::RESET
            );
        } else {
            println!("  {}", line);
        }
    }
}

/// Starts a Prompt-Driven Development planning session.
///
/// This is a thin wrapper that bypasses Ralph's event loop entirely.
/// It spawns the AI backend with the bundled PDD SOP for interactive planning.
fn plan_command(
    config_sources: &[ConfigSource],
    color_mode: ColorMode,
    args: PlanArgs,
) -> Result<()> {
    use sop_runner::{Sop, SopRunConfig, SopRunError};

    let use_colors = color_mode.should_use_colors();

    // Show what we're starting
    if use_colors {
        println!(
            "{}🎯{} Starting {} session...",
            colors::CYAN,
            colors::RESET,
            Sop::Pdd.name()
        );
    } else {
        println!("Starting {} session...", Sop::Pdd.name());
    }

    // Extract first file source for config path
    let config_path = config_sources.iter().find_map(|s| match s {
        ConfigSource::File(path) => Some(path.clone()),
        _ => None,
    });

    let config = SopRunConfig {
        sop: Sop::Pdd,
        user_input: args.idea,
        backend_override: args.backend,
        config_path,
        custom_args: if args.custom_args.is_empty() {
            None
        } else {
            Some(args.custom_args)
        },
        agent_teams: args.teams,
    };

    sop_runner::run_sop(config).map_err(|e| match e {
        SopRunError::NoBackend(no_backend) => anyhow::Error::new(no_backend),
        SopRunError::UnknownBackend(name) => anyhow::anyhow!(
            "Unknown backend: {}\n\nValid backends: claude, kiro, gemini, codex, amp",
            name
        ),
        SopRunError::SpawnError(io_err) => anyhow::anyhow!("Failed to spawn backend: {}", io_err),
    })
}

/// Starts a code-task-generator session.
///
/// This is a thin wrapper that bypasses Ralph's event loop entirely.
/// It spawns the AI backend with the bundled code-task-generator SOP.
fn code_task_command(
    config_sources: &[ConfigSource],
    color_mode: ColorMode,
    args: CodeTaskArgs,
) -> Result<()> {
    use sop_runner::{Sop, SopRunConfig, SopRunError};

    let use_colors = color_mode.should_use_colors();

    // Show what we're starting
    if use_colors {
        println!(
            "{}📋{} Starting {} session...",
            colors::CYAN,
            colors::RESET,
            Sop::CodeTaskGenerator.name()
        );
    } else {
        println!("Starting {} session...", Sop::CodeTaskGenerator.name());
    }

    // Extract first file source for config path
    let config_path = config_sources.iter().find_map(|s| match s {
        ConfigSource::File(path) => Some(path.clone()),
        _ => None,
    });

    let config = SopRunConfig {
        sop: Sop::CodeTaskGenerator,
        user_input: args.input,
        backend_override: args.backend,
        config_path,
        custom_args: if args.custom_args.is_empty() {
            None
        } else {
            Some(args.custom_args)
        },
        agent_teams: args.teams,
    };

    sop_runner::run_sop(config).map_err(|e| match e {
        SopRunError::NoBackend(no_backend) => anyhow::Error::new(no_backend),
        SopRunError::UnknownBackend(name) => anyhow::anyhow!(
            "Unknown backend: {}\n\nValid backends: claude, kiro, gemini, codex, amp",
            name
        ),
        SopRunError::SpawnError(io_err) => anyhow::anyhow!("Failed to spawn backend: {}", io_err),
    })
}

/// Lists directory contents recursively for dry-run mode.
fn list_directory_contents(path: &Path, use_colors: bool, indent: usize) -> Result<()> {
    let entries = fs::read_dir(path)?;
    let indent_str = "  ".repeat(indent);

    for entry in entries {
        let entry = entry?;
        let entry_path = entry.path();
        let file_name = entry.file_name();

        if entry_path.is_dir() {
            if use_colors {
                println!(
                    "{}{}{}/{}",
                    indent_str,
                    colors::BLUE,
                    file_name.to_string_lossy(),
                    colors::RESET
                );
            } else {
                println!("{}{}/", indent_str, file_name.to_string_lossy());
            }
            list_directory_contents(&entry_path, use_colors, indent + 1)?;
        } else if use_colors {
            println!(
                "{}{}{}{}",
                indent_str,
                colors::DIM,
                file_name.to_string_lossy(),
                colors::RESET
            );
        } else {
            println!("{}{}", indent_str, file_name.to_string_lossy());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::CwdGuard;
    use std::path::PathBuf;

    #[test]
    fn test_verbosity_cli_quiet() {
        assert_eq!(Verbosity::resolve(false, true), Verbosity::Quiet);
    }

    #[test]
    fn test_verbosity_cli_verbose() {
        assert_eq!(Verbosity::resolve(true, false), Verbosity::Verbose);
    }

    #[test]
    fn test_verbosity_default() {
        assert_eq!(Verbosity::resolve(false, false), Verbosity::Normal);
    }

    #[test]
    fn test_verbosity_env_quiet() {
        assert_eq!(
            Verbosity::resolve_with_env(false, false, true, false),
            Verbosity::Quiet
        );
    }

    #[test]
    fn test_verbosity_env_verbose() {
        assert_eq!(
            Verbosity::resolve_with_env(false, false, false, true),
            Verbosity::Verbose
        );
    }

    #[test]
    fn test_color_mode_should_use_colors() {
        assert!(ColorMode::Always.should_use_colors());
        assert!(!ColorMode::Never.should_use_colors());
    }

    #[test]
    fn test_config_source_parse_builtin() {
        let source = ConfigSource::parse("builtin:feature");
        match source {
            ConfigSource::Builtin(name) => assert_eq!(name, "feature"),
            _ => panic!("Expected Builtin variant"),
        }
    }

    #[test]
    fn test_config_source_parse_remote_https() {
        let source = ConfigSource::parse("https://example.com/preset.yml");
        match source {
            ConfigSource::Remote(url) => assert_eq!(url, "https://example.com/preset.yml"),
            _ => panic!("Expected Remote variant"),
        }
    }

    #[test]
    fn test_config_source_parse_remote_http() {
        let source = ConfigSource::parse("http://example.com/preset.yml");
        match source {
            ConfigSource::Remote(url) => assert_eq!(url, "http://example.com/preset.yml"),
            _ => panic!("Expected Remote variant"),
        }
    }

    #[test]
    fn test_config_source_parse_file() {
        let source = ConfigSource::parse("ralph.yml");
        match source {
            ConfigSource::File(path) => assert_eq!(path, std::path::PathBuf::from("ralph.yml")),
            _ => panic!("Expected File variant"),
        }
    }

    #[test]
    fn test_config_source_parse_override_scratchpad() {
        let source = ConfigSource::parse("core.scratchpad=.ralph/feature/scratchpad.md");
        match source {
            ConfigSource::Override { key, value } => {
                assert_eq!(key, "core.scratchpad");
                assert_eq!(value, ".ralph/feature/scratchpad.md");
            }
            _ => panic!("Expected Override variant"),
        }
    }

    #[test]
    fn test_config_source_parse_override_specs_dir() {
        let source = ConfigSource::parse("core.specs_dir=./my-specs/");
        match source {
            ConfigSource::Override { key, value } => {
                assert_eq!(key, "core.specs_dir");
                assert_eq!(value, "./my-specs/");
            }
            _ => panic!("Expected Override variant"),
        }
    }

    #[test]
    fn test_bot_daemon_parses_global_config_flag() {
        let cli = Cli::try_parse_from(["ralph", "bot", "daemon", "-c", "ralph.bot.yml"])
            .expect("CLI parse failed");

        assert!(cli.config.iter().any(|value| value == "ralph.bot.yml"));
        assert!(matches!(
            cli.command,
            Some(Commands::Bot(crate::bot::BotArgs {
                command: crate::bot::BotCommands::Daemon(_),
            }))
        ));
    }

    #[test]
    fn test_doctor_parses_command() {
        let cli = Cli::try_parse_from(["ralph", "doctor"]).expect("CLI parse failed");

        assert!(matches!(cli.command, Some(Commands::Doctor(_))));
    }

    #[test]
    fn test_tutorial_parses_command() {
        let cli = Cli::try_parse_from(["ralph", "tutorial"]).expect("CLI parse failed");

        assert!(matches!(cli.command, Some(Commands::Tutorial(_))));
    }

    #[test]
    fn test_tutorial_steps_cover_core_topics() {
        let steps = tutorial_steps();
        assert_eq!(steps.len(), 3);
        assert!(steps.iter().any(|step| step.title.contains("Hats")));
        assert!(steps.iter().any(|step| step.title.contains("Presets")));
        assert!(steps.iter().any(|step| step.title.contains("Workflow")));
    }

    #[test]
    fn test_config_source_parse_file_with_equals() {
        // Paths containing '=' but not starting with 'core.' should be treated as files
        let source = ConfigSource::parse("path/with=equals.yml");
        match source {
            ConfigSource::File(path) => {
                assert_eq!(path, std::path::PathBuf::from("path/with=equals.yml"))
            }
            _ => panic!("Expected File variant for path with equals sign"),
        }
    }

    #[test]
    fn test_config_source_parse_core_without_equals() {
        // "core.field" without '=' should be treated as a file path (will fail to load)
        let source = ConfigSource::parse("core.field");
        match source {
            ConfigSource::File(path) => assert_eq!(path, std::path::PathBuf::from("core.field")),
            _ => panic!("Expected File variant for core.field without ="),
        }
    }

    #[test]
    fn test_apply_config_overrides_scratchpad() {
        let mut config = RalphConfig::default();
        let sources = vec![ConfigSource::Override {
            key: "core.scratchpad".to_string(),
            value: ".custom/scratch.md".to_string(),
        }];
        apply_config_overrides(&mut config, &sources).unwrap();
        assert_eq!(config.core.scratchpad, ".custom/scratch.md");
    }

    #[test]
    fn test_apply_config_overrides_specs_dir() {
        let mut config = RalphConfig::default();
        let sources = vec![ConfigSource::Override {
            key: "core.specs_dir".to_string(),
            value: "./specifications/".to_string(),
        }];
        apply_config_overrides(&mut config, &sources).unwrap();
        assert_eq!(config.core.specs_dir, "./specifications/");
    }

    #[test]
    fn test_apply_config_overrides_multiple() {
        let mut config = RalphConfig::default();
        let sources = vec![
            ConfigSource::Override {
                key: "core.scratchpad".to_string(),
                value: ".custom/scratch.md".to_string(),
            },
            ConfigSource::Override {
                key: "core.specs_dir".to_string(),
                value: "./my-specs/".to_string(),
            },
        ];
        apply_config_overrides(&mut config, &sources).unwrap();
        assert_eq!(config.core.scratchpad, ".custom/scratch.md");
        assert_eq!(config.core.specs_dir, "./my-specs/");
    }

    #[test]
    fn test_apply_config_overrides_unknown_field() {
        // Unknown core.* fields should warn but not error
        let mut config = RalphConfig::default();
        let original_scratchpad = config.core.scratchpad.clone();
        let sources = vec![ConfigSource::Override {
            key: "core.unknown_field".to_string(),
            value: "some_value".to_string(),
        }];
        // Should not error
        apply_config_overrides(&mut config, &sources).unwrap();
        // Original values should be unchanged
        assert_eq!(config.core.scratchpad, original_scratchpad);
    }

    #[test]
    fn test_config_source_parse_non_core_with_equals_is_file() {
        // Non-core.* prefix with '=' should be treated as file path per spec
        let source = ConfigSource::parse("event_loop.max_iterations=5");
        match source {
            ConfigSource::File(path) => {
                assert_eq!(
                    path,
                    std::path::PathBuf::from("event_loop.max_iterations=5")
                )
            }
            _ => panic!("Expected File variant, not Override"),
        }
    }

    #[test]
    fn test_ensure_scratchpad_directory_creates_nested() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = RalphConfig::default();
        config.core.workspace_root = temp_dir.path().to_path_buf();

        config.core.scratchpad = "a/b/c/scratchpad.md".to_string();

        let result = ensure_scratchpad_directory(&config);
        assert!(result.is_ok());

        // Verify directory was created
        let expected_dir = temp_dir.path().join("a/b/c");
        assert!(expected_dir.exists());
    }

    #[test]
    fn test_ensure_scratchpad_directory_noop_when_exists() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = RalphConfig::default();
        config.core.workspace_root = temp_dir.path().to_path_buf();

        // Pre-create the directory
        let subdir = temp_dir.path().join("existing");
        std::fs::create_dir_all(&subdir).unwrap();
        config.core.scratchpad = "existing/scratchpad.md".to_string();

        // Should succeed without error (no-op)
        let result = ensure_scratchpad_directory(&config);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_auto_preflight_dry_run_returns_report() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = RalphConfig::default();
        config.core.workspace_root = temp_dir.path().to_path_buf();
        config.features.preflight.enabled = true;
        config.features.preflight.skip = vec!["git".to_string(), "tools".to_string()];
        config.cli.backend = "custom".to_string();
        config.cli.command = Some("definitely-missing-12345".to_string());

        let report = run_auto_preflight(&config, false, false, AutoPreflightMode::DryRun)
            .await
            .unwrap();

        let report = report.expect("expected preflight report in dry-run mode");
        assert!(!report.passed);
        assert!(report.failures >= 1);
    }

    #[tokio::test]
    async fn test_auto_preflight_run_fails_on_check_failure() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut config = RalphConfig::default();
        config.core.workspace_root = temp_dir.path().to_path_buf();
        config.features.preflight.enabled = true;
        config.features.preflight.skip = vec!["git".to_string(), "tools".to_string()];
        config.cli.backend = "custom".to_string();
        config.cli.command = Some("definitely-missing-12345".to_string());

        let err = run_auto_preflight(&config, false, false, AutoPreflightMode::Run)
            .await
            .expect_err("expected preflight failure in run mode");

        assert!(err.to_string().contains("Preflight checks failed"));
    }

    #[test]
    fn test_partition_config_sources_separates_overrides() {
        let sources = [
            ConfigSource::File(PathBuf::from("ralph.yml")),
            ConfigSource::Override {
                key: "core.scratchpad".to_string(),
                value: ".custom/scratchpad.md".to_string(),
            },
            ConfigSource::Builtin("tdd".to_string()),
            ConfigSource::Override {
                key: "core.specs_dir".to_string(),
                value: "./specs/".to_string(),
            },
        ];

        let (primary, overrides): (Vec<_>, Vec<_>) = sources
            .iter()
            .partition(|s| !matches!(s, ConfigSource::Override { .. }));

        assert_eq!(primary.len(), 2); // File + Builtin
        assert_eq!(overrides.len(), 2); // Two overrides
        assert!(matches!(primary[0], ConfigSource::File(_)));
        assert!(matches!(primary[1], ConfigSource::Builtin(_)));
    }

    #[test]
    fn test_partition_config_sources_only_overrides() {
        let sources = [ConfigSource::Override {
            key: "core.scratchpad".to_string(),
            value: ".custom/scratchpad.md".to_string(),
        }];

        let (primary, overrides): (Vec<_>, Vec<_>) = sources
            .iter()
            .partition(|s| !matches!(s, ConfigSource::Override { .. }));

        assert_eq!(primary.len(), 0); // No primary sources
        assert_eq!(overrides.len(), 1); // One override
    }

    #[test]
    fn test_load_config_from_file_with_overrides() {
        // Integration test: load a real config file and apply overrides
        let temp_dir = tempfile::tempdir().unwrap();
        let config_path = temp_dir.path().join("test.yml");
        std::fs::write(
            &config_path,
            r"
cli:
  backend: claude
core:
  scratchpad: .agent/scratchpad.md
  specs_dir: ./specs/
",
        )
        .unwrap();

        let mut config = RalphConfig::from_file(&config_path).unwrap();
        assert_eq!(config.core.scratchpad, ".agent/scratchpad.md");

        // Apply override
        let overrides = vec![ConfigSource::Override {
            key: "core.scratchpad".to_string(),
            value: ".custom/scratch.md".to_string(),
        }];
        apply_config_overrides(&mut config, &overrides).unwrap();

        assert_eq!(config.core.scratchpad, ".custom/scratch.md");
        assert_eq!(config.core.specs_dir, "./specs/"); // Unchanged
    }

    /// Regression test for prompt_summary reading file content instead of path.
    ///
    /// Previously, when prompt_file was used, the prompt_summary would just
    /// return the file path string. This caused confusing error messages like
    /// "Configuration file not found at con..." when the path was displayed.
    ///
    /// The fix ensures prompt_summary reads the actual file content.
    #[test]
    fn test_prompt_summary_reads_file_content_not_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let prompt_path = temp_dir.path().join("PROMPT.md");
        let prompt_content = "Build a feature that does amazing things";

        // Write the prompt file
        std::fs::write(&prompt_path, prompt_content).unwrap();

        // Create config with prompt_file set
        let mut config = RalphConfig::default();
        config.event_loop.prompt_file = prompt_path.to_string_lossy().to_string();
        config.event_loop.prompt = None;

        // Simulate the prompt_summary logic from run_command
        let prompt_summary = config
            .event_loop
            .prompt
            .clone()
            .or_else(|| {
                let prompt_file = &config.event_loop.prompt_file;
                if prompt_file.is_empty() {
                    None
                } else {
                    let path = std::path::Path::new(prompt_file);
                    if path.exists() {
                        std::fs::read_to_string(path).ok()
                    } else {
                        None
                    }
                }
            })
            .map(|p| {
                if p.len() > 100 {
                    format!("{}...", &p[..100])
                } else {
                    p
                }
            })
            .unwrap_or_else(|| "[no prompt]".to_string());

        // Assert: summary contains file content, NOT the file path
        assert_eq!(prompt_summary, prompt_content);
        assert!(!prompt_summary.contains("PROMPT.md"));
        assert!(!prompt_summary.contains(&temp_dir.path().to_string_lossy().to_string()));
    }

    #[test]
    fn test_prompt_summary_truncates_long_content() {
        let temp_dir = tempfile::tempdir().unwrap();
        let prompt_path = temp_dir.path().join("LONG_PROMPT.md");
        let long_content = "X".repeat(150); // 150 chars, exceeds 100 limit

        std::fs::write(&prompt_path, &long_content).unwrap();

        let mut config = RalphConfig::default();
        config.event_loop.prompt_file = prompt_path.to_string_lossy().to_string();
        config.event_loop.prompt = None;

        // Simulate the prompt_summary logic
        let prompt_summary = config
            .event_loop
            .prompt
            .clone()
            .or_else(|| {
                let prompt_file = &config.event_loop.prompt_file;
                if prompt_file.is_empty() {
                    None
                } else {
                    let path = std::path::Path::new(prompt_file);
                    if path.exists() {
                        std::fs::read_to_string(path).ok()
                    } else {
                        None
                    }
                }
            })
            .map(|p| {
                if p.len() > 100 {
                    format!("{}...", &p[..100])
                } else {
                    p
                }
            })
            .unwrap_or_else(|| "[no prompt]".to_string());

        // Assert: truncated to 100 chars + "..."
        assert_eq!(prompt_summary.len(), 103); // 100 + "..."
        assert!(prompt_summary.ends_with("..."));
    }

    #[test]
    fn test_prompt_summary_returns_no_prompt_for_missing_file() {
        let mut config = RalphConfig::default();
        config.event_loop.prompt_file = "/nonexistent/path/PROMPT.md".to_string();
        config.event_loop.prompt = None;

        // Simulate the prompt_summary logic
        let prompt_summary = config
            .event_loop
            .prompt
            .clone()
            .or_else(|| {
                let prompt_file = &config.event_loop.prompt_file;
                if prompt_file.is_empty() {
                    None
                } else {
                    let path = std::path::Path::new(prompt_file);
                    if path.exists() {
                        std::fs::read_to_string(path).ok()
                    } else {
                        None
                    }
                }
            })
            .map(|p| {
                if p.len() > 100 {
                    format!("{}...", &p[..100])
                } else {
                    p
                }
            })
            .unwrap_or_else(|| "[no prompt]".to_string());

        // Assert: returns "[no prompt]" for missing file
        assert_eq!(prompt_summary, "[no prompt]");
    }

    #[test]
    fn test_format_preflight_summary_with_failures() {
        let report = PreflightReport {
            passed: false,
            warnings: 1,
            failures: 1,
            checks: vec![
                ralph_core::CheckResult::pass("config", "Config"),
                ralph_core::CheckResult::warn("backend", "Backend", "Missing"),
                ralph_core::CheckResult::fail("paths", "Paths", "Missing path"),
            ],
        };

        let summary = format_preflight_summary(&report);

        assert!(summary.contains("✓"));
        assert!(summary.contains("⚠"));
        assert!(summary.contains("✗"));
        assert!(summary.contains("(1 failure)"));
    }

    #[test]
    fn test_format_preflight_summary_no_checks() {
        let report = PreflightReport {
            passed: true,
            warnings: 0,
            failures: 0,
            checks: Vec::new(),
        };

        let summary = format_preflight_summary(&report);

        assert_eq!(summary, "no checks");
    }

    #[test]
    fn test_preflight_failure_detail_strict_includes_warnings() {
        let report = PreflightReport {
            passed: false,
            warnings: 2,
            failures: 1,
            checks: Vec::new(),
        };

        assert_eq!(preflight_failure_detail(&report, false), "1 failure");
        assert_eq!(
            preflight_failure_detail(&report, true),
            "1 failure, 2 warnings"
        );
    }

    #[test]
    fn test_load_config_with_overrides_applies_override_sources() {
        let temp_dir = tempfile::tempdir().unwrap();
        let _cwd = CwdGuard::set(temp_dir.path());
        let config_path = temp_dir.path().join("ralph.yml");
        std::fs::write(&config_path, "core:\n  scratchpad: .agent/scratchpad.md\n").unwrap();

        let sources = vec![
            ConfigSource::File(config_path),
            ConfigSource::Override {
                key: "core.scratchpad".to_string(),
                value: ".custom/scratch.md".to_string(),
            },
        ];

        let config = load_config_with_overrides(&sources).unwrap();

        assert_eq!(config.core.scratchpad, ".custom/scratch.md");
        let expected_root = std::fs::canonicalize(temp_dir.path())
            .unwrap_or_else(|_| temp_dir.path().to_path_buf());
        let actual_root = std::fs::canonicalize(&config.core.workspace_root)
            .unwrap_or_else(|_| config.core.workspace_root.clone());
        assert_eq!(actual_root, expected_root);
    }

    #[test]
    fn test_load_config_with_overrides_only_overrides_uses_defaults() {
        let temp_dir = tempfile::tempdir().unwrap();
        let _cwd = CwdGuard::set(temp_dir.path());

        let sources = vec![ConfigSource::Override {
            key: "core.specs_dir".to_string(),
            value: "custom-specs".to_string(),
        }];

        let config = load_config_with_overrides(&sources).unwrap();

        assert_eq!(config.core.specs_dir, "custom-specs");
        let expected_root = std::fs::canonicalize(temp_dir.path())
            .unwrap_or_else(|_| temp_dir.path().to_path_buf());
        let actual_root = std::fs::canonicalize(&config.core.workspace_root)
            .unwrap_or_else(|_| config.core.workspace_root.clone());
        assert_eq!(actual_root, expected_root);
    }

    #[test]
    fn test_load_config_with_overrides_missing_file_falls_back_to_defaults() {
        let temp_dir = tempfile::tempdir().unwrap();
        let _cwd = CwdGuard::set(temp_dir.path());

        let sources = vec![ConfigSource::File(PathBuf::from("missing.yml"))];

        let config = load_config_with_overrides(&sources).unwrap();

        let default = RalphConfig::default();
        assert_eq!(config.core.scratchpad, default.core.scratchpad);
        let expected_root = std::fs::canonicalize(temp_dir.path())
            .unwrap_or_else(|_| temp_dir.path().to_path_buf());
        let actual_root = std::fs::canonicalize(&config.core.workspace_root)
            .unwrap_or_else(|_| config.core.workspace_root.clone());
        assert_eq!(actual_root, expected_root);
    }

    #[test]
    fn test_list_directory_contents_handles_nested_paths() {
        let temp_dir = tempfile::tempdir().unwrap();
        let nested_dir = temp_dir.path().join("one/two");
        std::fs::create_dir_all(&nested_dir).unwrap();
        std::fs::write(temp_dir.path().join("one/file.txt"), "hello").unwrap();

        assert!(list_directory_contents(temp_dir.path(), false, 0).is_ok());
        assert!(list_directory_contents(temp_dir.path(), true, 0).is_ok());
    }

    #[test]
    fn test_list_directory_contents_missing_path_returns_error() {
        let temp_dir = tempfile::tempdir().unwrap();
        let missing = temp_dir.path().join("missing");

        assert!(list_directory_contents(&missing, false, 0).is_err());
    }

    #[test]
    fn test_print_preflight_summary_handles_failures_and_warnings() {
        let report = PreflightReport {
            passed: false,
            warnings: 1,
            failures: 1,
            checks: vec![
                ralph_core::CheckResult::pass("config", "Config"),
                ralph_core::CheckResult::warn("backend", "Backend", "Missing"),
                ralph_core::CheckResult::fail("paths", "Paths", "Missing path"),
            ],
        };

        print_preflight_summary(&report, true, "Preflight: ", true);
        print_preflight_summary(&report, false, "Preflight: ", false);
    }

    fn default_run_args() -> RunArgs {
        RunArgs {
            prompt_text: None,
            backend: Some("claude".to_string()),
            prompt_file: None,
            max_iterations: None,
            completion_promise: None,
            dry_run: false,
            continue_mode: false,
            no_tui: true,
            autonomous: false,
            idle_timeout: None,
            exclusive: false,
            no_auto_merge: false,
            skip_preflight: true,
            verbose: false,
            quiet: false,
            record_session: None,
            custom_args: Vec::new(),
        }
    }

    #[tokio::test]
    async fn test_run_command_continue_missing_scratchpad_returns_error() {
        let temp_dir = tempfile::tempdir().unwrap();
        let _cwd = CwdGuard::set(temp_dir.path());

        let mut args = default_run_args();
        args.continue_mode = true;

        let err = run_command(&[], false, ColorMode::Never, args)
            .await
            .expect_err("expected missing scratchpad error");
        assert!(err.to_string().contains("scratchpad not found"));
    }

    #[tokio::test]
    async fn test_run_command_dry_run_inline_prompt_skips_execution() {
        let temp_dir = tempfile::tempdir().unwrap();
        let _cwd = CwdGuard::set(temp_dir.path());

        let mut args = default_run_args();
        args.dry_run = true;
        args.prompt_text = Some("Test inline prompt".to_string());

        run_command(&[], false, ColorMode::Never, args)
            .await
            .expect("dry run should succeed");
    }
}
