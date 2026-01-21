//! Kiro backend test scenarios.
//!
//! This module provides test scenarios specific to the Kiro backend,
//! starting with the basic connectivity test.

use super::{Assertions, ScenarioError, TestScenario};
use crate::Backend;
use crate::executor::{PromptSource, RalphExecutor, ScenarioConfig};
use crate::models::TestResult;
use async_trait::async_trait;
use std::path::Path;
use std::time::Duration;

/// Basic connectivity test for Kiro backend.
///
/// This is the simplest possible test that verifies:
/// - Ralph can spawn with Kiro backend
/// - Kiro CLI responds to a simple prompt
/// - Exit code is 0
/// - No errors in stderr
///
/// # Example
///
/// ```no_run
/// use ralph_e2e::scenarios::{KiroConnectScenario, TestScenario};
/// use ralph_e2e::executor::RalphExecutor;
/// use std::path::Path;
///
/// #[tokio::main]
/// async fn main() {
///     let scenario = KiroConnectScenario::new();
///     let workspace = Path::new(".e2e-tests/kiro-connect");
///
///     let config = scenario.setup(workspace).unwrap();
///     let executor = RalphExecutor::new(workspace.to_path_buf());
///     let result = scenario.run(&executor, &config).await.unwrap();
///
///     assert!(result.passed);
/// }
/// ```
pub struct KiroConnectScenario {
    id: String,
    description: String,
    tier: String,
}

impl KiroConnectScenario {
    /// Creates a new Kiro connectivity scenario.
    pub fn new() -> Self {
        Self {
            id: "kiro-connect".to_string(),
            description: "Basic connectivity test for Kiro backend".to_string(),
            tier: "Tier 1: Connectivity".to_string(),
        }
    }
}

impl Default for KiroConnectScenario {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TestScenario for KiroConnectScenario {
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
        Backend::Kiro
    }

    fn setup(&self, workspace: &Path) -> Result<ScenarioConfig, ScenarioError> {
        // Create the .agent directory
        let agent_dir = workspace.join(".agent");
        std::fs::create_dir_all(&agent_dir).map_err(|e| {
            ScenarioError::SetupError(format!("failed to create .agent directory: {}", e))
        })?;

        // Create minimal ralph.yml for Kiro
        let config_content = r#"# Minimal Kiro config for connectivity test
cli:
  backend: kiro

event_loop:
  max_iterations: 1
  completion_promise: "DONE"
"#;
        let config_path = workspace.join("ralph.yml");
        std::fs::write(&config_path, config_content)
            .map_err(|e| ScenarioError::SetupError(format!("failed to write ralph.yml: {}", e)))?;

        // Create a simple prompt
        let prompt = "Say 'Hello from Kiro!' and then output exactly: DONE";

        Ok(ScenarioConfig {
            config_file: "ralph.yml".into(),
            prompt: PromptSource::Inline(prompt.to_string()),
            max_iterations: 1,
            timeout: Duration::from_secs(300), // 5 minutes - backend iterations can be slow
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
            "ralph-e2e-kiro-{}-{}",
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
    fn test_kiro_connect_scenario_new() {
        let scenario = KiroConnectScenario::new();
        assert_eq!(scenario.id(), "kiro-connect");
        assert_eq!(scenario.backend(), Backend::Kiro);
        assert_eq!(scenario.tier(), "Tier 1: Connectivity");
    }

    #[test]
    fn test_kiro_connect_scenario_default() {
        let scenario = KiroConnectScenario::default();
        assert_eq!(scenario.id(), "kiro-connect");
    }

    #[test]
    fn test_kiro_connect_setup_creates_config() {
        let workspace = test_workspace("setup-creates-config");
        fs::create_dir_all(&workspace).unwrap();

        let scenario = KiroConnectScenario::new();
        let config = scenario.setup(&workspace).unwrap();

        // Verify ralph.yml was created
        let config_path = workspace.join("ralph.yml");
        assert!(config_path.exists(), "ralph.yml should exist");

        // Verify content
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("backend: kiro"));
        assert!(content.contains("max_iterations: 1"));

        // Verify .agent directory was created
        assert!(workspace.join(".agent").exists());

        // Verify config struct
        assert_eq!(config.max_iterations, 1);
        assert!(matches!(config.prompt, PromptSource::Inline(_)));

        cleanup_workspace(&workspace);
    }

    #[test]
    fn test_kiro_connect_setup_fails_if_cannot_create_dir() {
        let workspace = std::path::Path::new("/nonexistent/path/that/does/not/exist");
        let scenario = KiroConnectScenario::new();

        let result = scenario.setup(workspace);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ScenarioError::SetupError(_)));
    }

    #[test]
    fn test_kiro_connect_cleanup_is_noop() {
        let workspace = test_workspace("cleanup-noop");
        fs::create_dir_all(&workspace).unwrap();

        let scenario = KiroConnectScenario::new();
        let result = scenario.cleanup(&workspace);
        assert!(result.is_ok());

        cleanup_workspace(&workspace);
    }

    #[test]
    fn test_kiro_connect_scenario_description() {
        let scenario = KiroConnectScenario::new();
        assert!(scenario.description().contains("connectivity"));
        assert!(scenario.description().contains("Kiro"));
    }

    // Integration test - requires ralph binary and kiro CLI
    #[tokio::test]
    #[ignore = "requires live backend"]
    async fn test_kiro_connect_full_run() {
        let workspace = test_workspace("full-run");
        fs::create_dir_all(&workspace).unwrap();

        let scenario = KiroConnectScenario::new();
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
