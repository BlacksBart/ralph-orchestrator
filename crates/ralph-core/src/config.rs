//! Configuration types for the Ralph Orchestrator.
//!
//! This module supports both v1.x flat configuration format and v2.0 nested format.
//! Users can switch from Python v1.x to Rust v2.0 with zero config changes.

use ralph_proto::Topic;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Top-level configuration for Ralph Orchestrator.
///
/// Supports both v1.x flat format and v2.0 nested format:
/// - v1: `agent: claude`, `max_iterations: 100`
/// - v2: `cli: { backend: claude }`, `event_loop: { max_iterations: 100 }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RalphConfig {
    /// Execution mode: "single" or "multi".
    #[serde(default = "default_mode")]
    pub mode: String,

    /// Event loop configuration (v2 nested style).
    #[serde(default)]
    pub event_loop: EventLoopConfig,

    /// CLI backend configuration (v2 nested style).
    #[serde(default)]
    pub cli: CliConfig,

    /// Hat definitions for multi-hat mode.
    #[serde(default)]
    pub hats: HashMap<String, HatConfig>,

    // ─────────────────────────────────────────────────────────────────────────
    // V1 COMPATIBILITY FIELDS (flat format)
    // These map to nested v2 fields for backwards compatibility.
    // ─────────────────────────────────────────────────────────────────────────

    /// V1 field: Backend CLI (maps to cli.backend).
    /// Values: "claude", "gemini", "codex", "amp", "auto", or "custom".
    #[serde(default)]
    pub agent: Option<String>,

    /// V1 field: Fallback order for auto-detection.
    #[serde(default)]
    pub agent_priority: Vec<String>,

    /// V1 field: Path to prompt file (maps to event_loop.prompt_file).
    #[serde(default)]
    pub prompt_file: Option<String>,

    /// V1 field: Completion detection string (maps to event_loop.completion_promise).
    #[serde(default)]
    pub completion_promise: Option<String>,

    /// V1 field: Maximum loop iterations (maps to event_loop.max_iterations).
    #[serde(default)]
    pub max_iterations: Option<u32>,

    /// V1 field: Maximum runtime in seconds (maps to event_loop.max_runtime_seconds).
    #[serde(default)]
    pub max_runtime: Option<u64>,

    /// V1 field: Maximum cost in USD (maps to event_loop.max_cost_usd).
    #[serde(default)]
    pub max_cost: Option<f64>,

    /// V1 field: Iterations between git checkpoints (maps to event_loop.checkpoint_interval).
    #[serde(default)]
    pub checkpoint_interval: Option<u32>,

    // ─────────────────────────────────────────────────────────────────────────
    // FEATURE FLAGS
    // ─────────────────────────────────────────────────────────────────────────

    /// Enable git checkpointing.
    #[serde(default = "default_true")]
    pub git_checkpoint: bool,

    /// Enable verbose output.
    #[serde(default)]
    pub verbose: bool,

    /// Archive prompts after completion (DEFERRED: warn if enabled).
    #[serde(default)]
    pub archive_prompts: bool,

    /// Enable metrics collection (DEFERRED: warn if enabled).
    #[serde(default)]
    pub enable_metrics: bool,

    // ─────────────────────────────────────────────────────────────────────────
    // DROPPED FIELDS (accepted but ignored with warning)
    // ─────────────────────────────────────────────────────────────────────────

    /// V1 field: Token limits (DROPPED: controlled by CLI tool).
    #[serde(default)]
    pub max_tokens: Option<u32>,

    /// V1 field: Retry delay (DROPPED: handled differently in v2).
    #[serde(default)]
    pub retry_delay: Option<u32>,

    /// V1 adapter settings (partially supported).
    #[serde(default)]
    pub adapters: AdaptersConfig,

    // ─────────────────────────────────────────────────────────────────────────
    // WARNING CONTROL
    // ─────────────────────────────────────────────────────────────────────────

    /// Suppress all warnings (for CI environments).
    #[serde(default, rename = "_suppress_warnings")]
    pub suppress_warnings: bool,
}

fn default_true() -> bool {
    true
}

fn default_mode() -> String {
    "single".to_string()
}

impl Default for RalphConfig {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            event_loop: EventLoopConfig::default(),
            cli: CliConfig::default(),
            hats: HashMap::new(),
            // V1 compatibility fields
            agent: None,
            agent_priority: vec![],
            prompt_file: None,
            completion_promise: None,
            max_iterations: None,
            max_runtime: None,
            max_cost: None,
            checkpoint_interval: None,
            // Feature flags
            git_checkpoint: true,
            verbose: false,
            archive_prompts: false,
            enable_metrics: false,
            // Dropped fields
            max_tokens: None,
            retry_delay: None,
            adapters: AdaptersConfig::default(),
            // Warning control
            suppress_warnings: false,
        }
    }
}

/// V1 adapter settings per backend.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdaptersConfig {
    /// Claude adapter settings.
    #[serde(default)]
    pub claude: AdapterSettings,

    /// Gemini adapter settings.
    #[serde(default)]
    pub gemini: AdapterSettings,

    /// Codex adapter settings.
    #[serde(default)]
    pub codex: AdapterSettings,

    /// Amp adapter settings.
    #[serde(default)]
    pub amp: AdapterSettings,
}

/// Per-adapter settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterSettings {
    /// CLI execution timeout in seconds.
    #[serde(default = "default_timeout")]
    pub timeout: u64,

    /// Include in auto-detection.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Tool permissions (DROPPED: CLI tool manages its own permissions).
    #[serde(default)]
    pub tool_permissions: Option<Vec<String>>,
}

fn default_timeout() -> u64 {
    300 // 5 minutes
}

impl Default for AdapterSettings {
    fn default() -> Self {
        Self {
            timeout: default_timeout(),
            enabled: true,
            tool_permissions: None,
        }
    }
}

impl RalphConfig {
    /// Loads configuration from a YAML file.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    /// Returns true if this is single-hat mode.
    pub fn is_single_mode(&self) -> bool {
        self.mode == "single"
    }

    /// Normalizes v1 flat fields into v2 nested structure.
    ///
    /// V1 flat fields take precedence over v2 nested fields when both are present.
    /// This allows users to use either format or mix them.
    pub fn normalize(&mut self) {
        // Map v1 `agent` to v2 `cli.backend`
        if let Some(ref agent) = self.agent {
            self.cli.backend = agent.clone();
        }

        // Map v1 `prompt_file` to v2 `event_loop.prompt_file`
        if let Some(ref pf) = self.prompt_file {
            self.event_loop.prompt_file = pf.clone();
        }

        // Map v1 `completion_promise` to v2 `event_loop.completion_promise`
        if let Some(ref cp) = self.completion_promise {
            self.event_loop.completion_promise = cp.clone();
        }

        // Map v1 `max_iterations` to v2 `event_loop.max_iterations`
        if let Some(mi) = self.max_iterations {
            self.event_loop.max_iterations = mi;
        }

        // Map v1 `max_runtime` to v2 `event_loop.max_runtime_seconds`
        if let Some(mr) = self.max_runtime {
            self.event_loop.max_runtime_seconds = mr;
        }

        // Map v1 `max_cost` to v2 `event_loop.max_cost_usd`
        if self.max_cost.is_some() {
            self.event_loop.max_cost_usd = self.max_cost;
        }

        // Map v1 `checkpoint_interval` to v2 `event_loop.checkpoint_interval`
        if let Some(ci) = self.checkpoint_interval {
            self.event_loop.checkpoint_interval = ci;
        }
    }

    /// Validates the configuration and returns warnings.
    ///
    /// This method checks for:
    /// - Deferred features that are enabled (archive_prompts, enable_metrics)
    /// - Dropped fields that are present (max_tokens, retry_delay, tool_permissions)
    /// - Invalid mode values
    /// - Multi-hat mode without hat definitions
    ///
    /// Returns a list of warnings that should be displayed to the user.
    pub fn validate(&self) -> Result<Vec<ConfigWarning>, ConfigError> {
        let mut warnings = Vec::new();

        // Skip all warnings if suppressed
        if self.suppress_warnings {
            return Ok(warnings);
        }

        // Check for deferred features
        if self.archive_prompts {
            warnings.push(ConfigWarning::DeferredFeature {
                field: "archive_prompts".to_string(),
                message: "Feature not yet available in v2".to_string(),
            });
        }

        if self.enable_metrics {
            warnings.push(ConfigWarning::DeferredFeature {
                field: "enable_metrics".to_string(),
                message: "Feature not yet available in v2".to_string(),
            });
        }

        // Check for dropped fields
        if self.max_tokens.is_some() {
            warnings.push(ConfigWarning::DroppedField {
                field: "max_tokens".to_string(),
                reason: "Token limits are controlled by the CLI tool".to_string(),
            });
        }

        if self.retry_delay.is_some() {
            warnings.push(ConfigWarning::DroppedField {
                field: "retry_delay".to_string(),
                reason: "Retry logic handled differently in v2".to_string(),
            });
        }

        // Check adapter tool_permissions (dropped field)
        if self.adapters.claude.tool_permissions.is_some()
            || self.adapters.gemini.tool_permissions.is_some()
            || self.adapters.codex.tool_permissions.is_some()
            || self.adapters.amp.tool_permissions.is_some()
        {
            warnings.push(ConfigWarning::DroppedField {
                field: "adapters.*.tool_permissions".to_string(),
                reason: "CLI tool manages its own permissions".to_string(),
            });
        }

        // Validate mode
        if self.mode != "single" && self.mode != "multi" {
            warnings.push(ConfigWarning::InvalidValue {
                field: "mode".to_string(),
                message: format!(
                    "Invalid mode '{}', expected 'single' or 'multi'. Defaulting to 'single'.",
                    self.mode
                ),
            });
        }

        // Check multi-hat mode without hats
        if self.mode == "multi" && self.hats.is_empty() {
            warnings.push(ConfigWarning::InvalidValue {
                field: "hats".to_string(),
                message: "Multi-hat mode requires at least one hat definition".to_string(),
            });
        }

        Ok(warnings)
    }

    /// Gets the effective backend name, resolving "auto" using the priority list.
    pub fn effective_backend(&self) -> &str {
        &self.cli.backend
    }

    /// Returns the agent priority list for auto-detection.
    /// If empty, returns the default priority order.
    pub fn get_agent_priority(&self) -> Vec<&str> {
        if self.agent_priority.is_empty() {
            vec!["claude", "gemini", "codex", "amp"]
        } else {
            self.agent_priority.iter().map(|s| s.as_str()).collect()
        }
    }

    /// Gets the adapter settings for a specific backend.
    pub fn adapter_settings(&self, backend: &str) -> &AdapterSettings {
        match backend {
            "claude" => &self.adapters.claude,
            "gemini" => &self.adapters.gemini,
            "codex" => &self.adapters.codex,
            "amp" => &self.adapters.amp,
            _ => &self.adapters.claude, // Default fallback
        }
    }
}

/// Configuration warnings emitted during validation.
#[derive(Debug, Clone)]
pub enum ConfigWarning {
    /// Feature is enabled but not yet available in v2.
    DeferredFeature { field: String, message: String },
    /// Field is present but ignored in v2.
    DroppedField { field: String, reason: String },
    /// Field has an invalid value.
    InvalidValue { field: String, message: String },
}

impl std::fmt::Display for ConfigWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigWarning::DeferredFeature { field, message } => {
                write!(f, "Warning [{}]: {}", field, message)
            }
            ConfigWarning::DroppedField { field, reason } => {
                write!(f, "Warning [{}]: Field ignored - {}", field, reason)
            }
            ConfigWarning::InvalidValue { field, message } => {
                write!(f, "Warning [{}]: {}", field, message)
            }
        }
    }
}

/// Event loop configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventLoopConfig {
    /// Path to the prompt file.
    #[serde(default = "default_prompt_file")]
    pub prompt_file: String,

    /// String that signals loop completion.
    #[serde(default = "default_completion_promise")]
    pub completion_promise: String,

    /// Maximum number of iterations before timeout.
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,

    /// Maximum runtime in seconds.
    #[serde(default = "default_max_runtime")]
    pub max_runtime_seconds: u64,

    /// Maximum cost in USD before stopping.
    pub max_cost_usd: Option<f64>,

    /// Stop after this many consecutive failures.
    #[serde(default = "default_max_failures")]
    pub max_consecutive_failures: u32,

    /// Create checkpoint commit every N iterations.
    #[serde(default = "default_checkpoint_interval")]
    pub checkpoint_interval: u32,

    /// Starting hat for multi-hat mode.
    pub starting_hat: Option<String>,
}

fn default_prompt_file() -> String {
    "PROMPT.md".to_string()
}

fn default_completion_promise() -> String {
    "LOOP_COMPLETE".to_string()
}

fn default_max_iterations() -> u32 {
    100
}

fn default_max_runtime() -> u64 {
    14400 // 4 hours
}

fn default_max_failures() -> u32 {
    5
}

fn default_checkpoint_interval() -> u32 {
    5
}

impl Default for EventLoopConfig {
    fn default() -> Self {
        Self {
            prompt_file: default_prompt_file(),
            completion_promise: default_completion_promise(),
            max_iterations: default_max_iterations(),
            max_runtime_seconds: default_max_runtime(),
            max_cost_usd: None,
            max_consecutive_failures: default_max_failures(),
            checkpoint_interval: default_checkpoint_interval(),
            starting_hat: None,
        }
    }
}

/// CLI backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    /// Backend to use: "claude", "gemini", "codex", "amp", or "custom".
    #[serde(default = "default_backend")]
    pub backend: String,

    /// Custom command (for backend: "custom").
    pub command: Option<String>,

    /// How to pass prompts: "arg" or "stdin".
    #[serde(default = "default_prompt_mode")]
    pub prompt_mode: String,
}

fn default_backend() -> String {
    "claude".to_string()
}

fn default_prompt_mode() -> String {
    "arg".to_string()
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            backend: default_backend(),
            command: None,
            prompt_mode: default_prompt_mode(),
        }
    }
}

/// Configuration for a single hat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HatConfig {
    /// Human-readable name for the hat.
    pub name: String,

    /// Topic patterns this hat subscribes to.
    #[serde(default)]
    pub subscriptions: Vec<String>,

    /// Topics this hat publishes.
    #[serde(default)]
    pub publishes: Vec<String>,

    /// Instructions prepended to prompts.
    #[serde(default)]
    pub instructions: String,
}

impl HatConfig {
    /// Converts subscription strings to Topic objects.
    pub fn subscription_topics(&self) -> Vec<Topic> {
        self.subscriptions.iter().map(|s| Topic::new(s)).collect()
    }

    /// Converts publish strings to Topic objects.
    pub fn publish_topics(&self) -> Vec<Topic> {
        self.publishes.iter().map(|s| Topic::new(s)).collect()
    }
}

/// Configuration errors.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = RalphConfig::default();
        assert_eq!(config.mode, "single");
        assert!(config.is_single_mode());
        assert_eq!(config.event_loop.max_iterations, 100);
        assert!(config.git_checkpoint);
        assert!(!config.verbose);
    }

    #[test]
    fn test_parse_yaml_v2_format() {
        let yaml = r#"
mode: "multi"
event_loop:
  prompt_file: "TASK.md"
  completion_promise: "DONE"
  max_iterations: 50
cli:
  backend: "claude"
hats:
  implementer:
    name: "Implementer"
    subscriptions: ["task.*", "review.done"]
    publishes: ["impl.done"]
    instructions: "You are the implementation agent."
"#;
        let config: RalphConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.mode, "multi");
        assert!(!config.is_single_mode());
        assert_eq!(config.event_loop.prompt_file, "TASK.md");
        assert_eq!(config.hats.len(), 1);

        let hat = config.hats.get("implementer").unwrap();
        assert_eq!(hat.subscriptions.len(), 2);
    }

    #[test]
    fn test_parse_yaml_v1_format() {
        // V1 flat format - identical to Python v1.x config
        let yaml = r#"
agent: gemini
prompt_file: "TASK.md"
completion_promise: "RALPH_DONE"
max_iterations: 75
max_runtime: 7200
max_cost: 10.0
checkpoint_interval: 10
git_checkpoint: true
verbose: true
"#;
        let mut config: RalphConfig = serde_yaml::from_str(yaml).unwrap();

        // Before normalization, v2 fields have defaults
        assert_eq!(config.cli.backend, "claude"); // default
        assert_eq!(config.event_loop.max_iterations, 100); // default

        // Normalize v1 -> v2
        config.normalize();

        // After normalization, v2 fields have v1 values
        assert_eq!(config.cli.backend, "gemini");
        assert_eq!(config.event_loop.prompt_file, "TASK.md");
        assert_eq!(config.event_loop.completion_promise, "RALPH_DONE");
        assert_eq!(config.event_loop.max_iterations, 75);
        assert_eq!(config.event_loop.max_runtime_seconds, 7200);
        assert_eq!(config.event_loop.max_cost_usd, Some(10.0));
        assert_eq!(config.event_loop.checkpoint_interval, 10);
        assert!(config.git_checkpoint);
        assert!(config.verbose);
    }

    #[test]
    fn test_agent_priority() {
        let yaml = r#"
agent: auto
agent_priority: [gemini, claude, codex]
"#;
        let config: RalphConfig = serde_yaml::from_str(yaml).unwrap();
        let priority = config.get_agent_priority();
        assert_eq!(priority, vec!["gemini", "claude", "codex"]);
    }

    #[test]
    fn test_default_agent_priority() {
        let config = RalphConfig::default();
        let priority = config.get_agent_priority();
        assert_eq!(priority, vec!["claude", "gemini", "codex", "amp"]);
    }

    #[test]
    fn test_validate_deferred_features() {
        let yaml = r#"
archive_prompts: true
enable_metrics: true
"#;
        let config: RalphConfig = serde_yaml::from_str(yaml).unwrap();
        let warnings = config.validate().unwrap();

        assert_eq!(warnings.len(), 2);
        assert!(warnings
            .iter()
            .any(|w| matches!(w, ConfigWarning::DeferredFeature { field, .. } if field == "archive_prompts")));
        assert!(warnings
            .iter()
            .any(|w| matches!(w, ConfigWarning::DeferredFeature { field, .. } if field == "enable_metrics")));
    }

    #[test]
    fn test_validate_dropped_fields() {
        let yaml = r#"
max_tokens: 4096
retry_delay: 5
adapters:
  claude:
    tool_permissions: ["read", "write"]
"#;
        let config: RalphConfig = serde_yaml::from_str(yaml).unwrap();
        let warnings = config.validate().unwrap();

        assert_eq!(warnings.len(), 3);
        assert!(warnings
            .iter()
            .any(|w| matches!(w, ConfigWarning::DroppedField { field, .. } if field == "max_tokens")));
        assert!(warnings
            .iter()
            .any(|w| matches!(w, ConfigWarning::DroppedField { field, .. } if field == "retry_delay")));
        assert!(warnings
            .iter()
            .any(|w| matches!(w, ConfigWarning::DroppedField { field, .. } if field == "adapters.*.tool_permissions")));
    }

    #[test]
    fn test_suppress_warnings() {
        let yaml = r#"
_suppress_warnings: true
archive_prompts: true
max_tokens: 4096
"#;
        let config: RalphConfig = serde_yaml::from_str(yaml).unwrap();
        let warnings = config.validate().unwrap();

        // All warnings should be suppressed
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_validate_multi_hat_without_hats() {
        let yaml = r#"
mode: "multi"
"#;
        let config: RalphConfig = serde_yaml::from_str(yaml).unwrap();
        let warnings = config.validate().unwrap();

        assert!(warnings
            .iter()
            .any(|w| matches!(w, ConfigWarning::InvalidValue { field, .. } if field == "hats")));
    }

    #[test]
    fn test_adapter_settings() {
        let yaml = r#"
adapters:
  claude:
    timeout: 600
    enabled: true
  gemini:
    timeout: 300
    enabled: false
"#;
        let config: RalphConfig = serde_yaml::from_str(yaml).unwrap();

        let claude = config.adapter_settings("claude");
        assert_eq!(claude.timeout, 600);
        assert!(claude.enabled);

        let gemini = config.adapter_settings("gemini");
        assert_eq!(gemini.timeout, 300);
        assert!(!gemini.enabled);
    }

    #[test]
    fn test_unknown_fields_ignored() {
        // Unknown fields should be silently ignored (forward compatibility)
        let yaml = r#"
agent: claude
unknown_field: "some value"
future_feature: true
"#;
        let result: Result<RalphConfig, _> = serde_yaml::from_str(yaml);
        // Should parse successfully, ignoring unknown fields
        assert!(result.is_ok());
    }
}
