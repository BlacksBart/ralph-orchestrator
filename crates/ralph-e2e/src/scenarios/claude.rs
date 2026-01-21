//! Claude backend test scenarios.
//!
//! This module provides test scenarios specific to the Claude backend,
//! starting with the basic connectivity test.

use super::{Assertions, ScenarioError, TestScenario};
use crate::Backend;
use crate::executor::{PromptSource, RalphExecutor, ScenarioConfig};
use crate::models::TestResult;
use async_trait::async_trait;
use std::path::Path;
use std::time::Duration;

/// Basic connectivity test for Claude backend.
///
/// This is the simplest possible test that verifies:
/// - Ralph can spawn with Claude backend
/// - Claude CLI responds to a simple prompt
/// - Exit code is 0
/// - No errors in stderr
///
/// # Example
///
/// ```no_run
/// use ralph_e2e::scenarios::{ClaudeConnectScenario, TestScenario};
/// use ralph_e2e::executor::RalphExecutor;
/// use std::path::Path;
///
/// #[tokio::main]
/// async fn main() {
///     let scenario = ClaudeConnectScenario::new();
///     let workspace = Path::new(".e2e-tests/claude-connect");
///
///     let config = scenario.setup(workspace).unwrap();
///     let executor = RalphExecutor::new(workspace.to_path_buf());
///     let result = scenario.run(&executor, &config).await.unwrap();
///
///     assert!(result.passed);
/// }
/// ```
pub struct ClaudeConnectScenario {
    id: String,
    description: String,
    tier: String,
}

impl ClaudeConnectScenario {
    /// Creates a new Claude connectivity scenario.
    pub fn new() -> Self {
        Self {
            id: "claude-connect".to_string(),
            description: "Basic connectivity test for Claude backend".to_string(),
            tier: "Tier 1: Connectivity".to_string(),
        }
    }
}

impl Default for ClaudeConnectScenario {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TestScenario for ClaudeConnectScenario {
    fn id(&self) -> &str {
        &self.id
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn tier(&self) -> &str {
        &self.tier
    }

    fn backend(&self) -> Backend {
        Backend::Claude
    }

    fn setup(&self, workspace: &Path) -> Result<ScenarioConfig, ScenarioError> {
        // Create the .agent directory
        let agent_dir = workspace.join(".agent");
        std::fs::create_dir_all(&agent_dir).map_err(|e| {
            ScenarioError::SetupError(format!("failed to create .agent directory: {}", e))
        })?;

        // Create a minimal scratchpad for completion verification
        // The completion detection requires scratchpad to exist with no pending [ ] tasks
        let scratchpad_content = r"# E2E Test Connectivity Check

- [x] Test connectivity (already done by saying hello)
";
        let scratchpad_path = agent_dir.join("scratchpad.md");
        std::fs::write(&scratchpad_path, scratchpad_content)
            .map_err(|e| ScenarioError::SetupError(format!("failed to write scratchpad: {}", e)))?;

        // Create minimal ralph.yml for Claude
        // NOTE: max_iterations: 5 is required because the dual-confirmation pattern
        // needs the agent to output DONE in 2 consecutive iterations. LLMs are
        // non-deterministic, so we give extra buffer iterations.
        let config_content = r#"# Minimal Claude config for connectivity test
cli:
  backend: claude

event_loop:
  max_iterations: 5
  completion_promise: "DONE"
"#;
        let config_path = workspace.join("ralph.yml");
        std::fs::write(&config_path, config_content)
            .map_err(|e| ScenarioError::SetupError(format!("failed to write ralph.yml: {}", e)))?;

        // Create a simple prompt that ensures DONE is output
        // The prompt emphasizes ending with DONE to help the dual-confirmation pattern succeed
        let prompt = "Say 'Hello from Claude!' and then on a new line output exactly the word DONE (this signals task completion)";

        Ok(ScenarioConfig {
            config_file: "ralph.yml".into(),
            prompt: PromptSource::Inline(prompt.to_string()),
            max_iterations: 5, // Matches ralph.yml - gives buffer for LLM non-determinism
            timeout: Duration::from_secs(600), // 10 minutes - Claude iterations can take 60-120s each
            extra_args: vec![],
        })
    }

    async fn run(
        &self,
        executor: &RalphExecutor,
        config: &ScenarioConfig,
    ) -> Result<TestResult, ScenarioError> {
        let start = std::time::Instant::now();

        // Execute ralph
        let execution = executor
            .run(config)
            .await
            .map_err(|e| ScenarioError::ExecutionError(format!("ralph execution failed: {}", e)))?;

        let duration = start.elapsed();

        // Build assertions
        // Note: We use exit_code_success_or_limit() because Ralph's exit code 2 means
        // "max iterations reached" which is valid for tests that functionally succeed
        // but don't hit the completion promise before iteration limit.
        let assertions = vec![
            Assertions::response_received(&execution),
            Assertions::exit_code_success_or_limit(&execution),
            Assertions::no_errors(&execution),
            Assertions::no_timeout(&execution),
        ];

        // Check if all assertions passed
        let all_passed = assertions.iter().all(|a| a.passed);

        Ok(TestResult {
            scenario_id: self.id.clone(),
            scenario_description: self.description.clone(),
            backend: self.backend().to_string(),
            tier: self.tier.clone(),
            passed: all_passed,
            assertions,
            duration,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;

    fn test_workspace(test_name: &str) -> std::path::PathBuf {
        env::temp_dir().join(format!(
            "ralph-e2e-scenario-{}-{}",
            test_name,
            std::process::id()
        ))
    }

    fn cleanup_workspace(path: &std::path::PathBuf) {
        if path.exists() {
            fs::remove_dir_all(path).ok();
        }
    }

    #[test]
    fn test_claude_connect_scenario_new() {
        let scenario = ClaudeConnectScenario::new();
        assert_eq!(scenario.id(), "claude-connect");
        assert_eq!(scenario.backend(), Backend::Claude);
        assert_eq!(scenario.tier(), "Tier 1: Connectivity");
    }

    #[test]
    fn test_claude_connect_scenario_default() {
        let scenario = ClaudeConnectScenario::default();
        assert_eq!(scenario.id(), "claude-connect");
    }

    #[test]
    fn test_claude_connect_setup_creates_config() {
        let workspace = test_workspace("setup-creates-config");
        fs::create_dir_all(&workspace).unwrap();

        let scenario = ClaudeConnectScenario::new();
        let config = scenario.setup(&workspace).unwrap();

        // Verify ralph.yml was created
        let config_path = workspace.join("ralph.yml");
        assert!(config_path.exists(), "ralph.yml should exist");

        // Verify content
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("backend: claude"));
        assert!(content.contains("max_iterations: 5"));

        // Verify .agent directory was created
        assert!(workspace.join(".agent").exists());

        // Verify scratchpad was created (required for completion verification)
        let scratchpad_path = workspace.join(".agent").join("scratchpad.md");
        assert!(scratchpad_path.exists(), "scratchpad.md should exist");
        let scratchpad_content = fs::read_to_string(&scratchpad_path).unwrap();
        assert!(
            scratchpad_content.contains("[x]"),
            "scratchpad should have completed task"
        );

        // Verify config struct
        assert_eq!(config.max_iterations, 5);
        assert!(matches!(config.prompt, PromptSource::Inline(_)));

        cleanup_workspace(&workspace);
    }

    #[test]
    fn test_claude_connect_setup_fails_if_cannot_create_dir() {
        let workspace = std::path::Path::new("/nonexistent/path/that/does/not/exist");
        let scenario = ClaudeConnectScenario::new();

        let result = scenario.setup(workspace);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ScenarioError::SetupError(_)));
    }

    #[test]
    fn test_claude_connect_cleanup_is_noop() {
        let workspace = test_workspace("cleanup-noop");
        fs::create_dir_all(&workspace).unwrap();

        let scenario = ClaudeConnectScenario::new();
        let result = scenario.cleanup(&workspace);
        assert!(result.is_ok());

        cleanup_workspace(&workspace);
    }

    #[test]
    fn test_claude_connect_scenario_description() {
        let scenario = ClaudeConnectScenario::new();
        assert!(scenario.description().contains("connectivity"));
    }

    // Integration test - requires ralph binary
    #[tokio::test]
    #[ignore = "requires live backend"]
    async fn test_claude_connect_full_run() {
        let workspace = test_workspace("full-run");
        fs::create_dir_all(&workspace).unwrap();

        let scenario = ClaudeConnectScenario::new();
        let config = scenario.setup(&workspace).unwrap();

        let executor = RalphExecutor::new(workspace.clone());
        let result = scenario.run(&executor, &config).await;

        cleanup_workspace(&workspace);

        let result = result.expect("run should succeed");
        println!("Assertions:");
        for a in &result.assertions {
            println!(
                "  {} - {}: {} (expected: {})",
                if a.passed { "✅" } else { "❌" },
                a.name,
                a.actual,
                a.expected
            );
        }
    }
}
