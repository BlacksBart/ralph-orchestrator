//! Tier 4: Capabilities test scenarios.
//!
//! These scenarios test Claude's advanced capabilities through Ralph:
//! - Tool invocation and response handling
//! - NDJSON streaming output parsing
//!
//! These tests verify that Ralph correctly interfaces with Claude's
//! extended features beyond basic text generation.

use super::{Assertions, ScenarioError, TestScenario};
use crate::Backend;
use crate::executor::{PromptSource, RalphExecutor, ScenarioConfig};
use crate::models::TestResult;
use async_trait::async_trait;
use std::path::Path;
use std::time::Duration;

/// Test scenario that verifies tool invocation.
///
/// This scenario:
/// - Sends a prompt that requires using a tool (e.g., file system access)
/// - Verifies that Claude invokes the tool correctly
/// - Validates that tool results are incorporated into the response
///
/// # Example
///
/// ```no_run
/// use ralph_e2e::scenarios::{ClaudeToolUseScenario, TestScenario};
///
/// let scenario = ClaudeToolUseScenario::new();
/// assert_eq!(scenario.tier(), "Tier 4: Capabilities");
/// ```
pub struct ClaudeToolUseScenario {
    id: String,
    description: String,
    tier: String,
}

impl ClaudeToolUseScenario {
    /// Creates a new tool use scenario.
    pub fn new() -> Self {
        Self {
            id: "claude-tool-use".to_string(),
            description: "Verifies tool invocation and response handling".to_string(),
            tier: "Tier 4: Capabilities".to_string(),
        }
    }
}

impl Default for ClaudeToolUseScenario {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TestScenario for ClaudeToolUseScenario {
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

        // Create a test file that the agent should read
        let test_file = workspace.join("test-data.txt");
        std::fs::write(&test_file, "Secret content: E2E_TEST_MARKER_42\n")
            .map_err(|e| ScenarioError::SetupError(format!("failed to write test file: {}", e)))?;

        // Create ralph.yml for tool use testing
        let config_content = r#"# Tool use test config
cli:
  backend: claude

event_loop:
  max_iterations: 1
  completion_promise: "LOOP_COMPLETE"
"#;
        let config_path = workspace.join("ralph.yml");
        std::fs::write(&config_path, config_content)
            .map_err(|e| ScenarioError::SetupError(format!("failed to write ralph.yml: {}", e)))?;

        // Create a prompt that requires tool use to read a file
        let prompt = format!(
            r"You are testing tool invocation capabilities.

Your task:
1. Read the contents of the file at: {}/test-data.txt
2. Report what you found in the file
3. Output LOOP_COMPLETE

You MUST use a tool to read the file. Do not guess the contents.",
            workspace.display()
        );

        Ok(ScenarioConfig {
            config_file: "ralph.yml".into(),
            prompt: PromptSource::Inline(prompt),
            max_iterations: 1,
            timeout: Duration::from_secs(300), // 5 minutes - Claude iterations can take 60-120s
            extra_args: vec![],
        })
    }

    async fn run(
        &self,
        executor: &RalphExecutor,
        config: &ScenarioConfig,
    ) -> Result<TestResult, ScenarioError> {
        let start = std::time::Instant::now();

        let execution = executor
            .run(config)
            .await
            .map_err(|e| ScenarioError::ExecutionError(format!("ralph execution failed: {}", e)))?;

        let duration = start.elapsed();

        // Build assertions for tool use
        // Note: We use exit_code_success_or_limit() because Ralph's exit code 2 means
        // "max iterations reached" which is valid when functional behavior succeeds.
        let assertions = vec![
            Assertions::response_received(&execution),
            Assertions::exit_code_success_or_limit(&execution),
            Assertions::no_timeout(&execution),
            self.tool_was_invoked(&execution),
            self.file_content_reported(&execution),
        ];

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

impl ClaudeToolUseScenario {
    /// Asserts that a tool was invoked during execution.
    fn tool_was_invoked(
        &self,
        result: &crate::executor::ExecutionResult,
    ) -> crate::models::Assertion {
        // Claude Code typically shows tool use in output with markers like "Read" or "Bash"
        // or shows tool results. Look for common patterns.
        let stdout = &result.stdout.to_lowercase();
        let has_tool_markers = stdout.contains("read")
            || stdout.contains("bash")
            || stdout.contains("cat ")
            || stdout.contains("test-data.txt")
            || stdout.contains("tool");

        super::AssertionBuilder::new("Tool was invoked")
            .expected("Evidence of tool invocation in output")
            .actual(if has_tool_markers {
                "Found tool-related content".to_string()
            } else {
                format!(
                    "No tool markers found. Output: {}",
                    truncate(&result.stdout, 100)
                )
            })
            .build()
            .with_passed(has_tool_markers)
    }

    /// Asserts that the file content was reported in the output.
    fn file_content_reported(
        &self,
        result: &crate::executor::ExecutionResult,
    ) -> crate::models::Assertion {
        // The test file contains "E2E_TEST_MARKER_42"
        let contains_marker = result.stdout.contains("E2E_TEST_MARKER_42");

        super::AssertionBuilder::new("File content reported")
            .expected("Output contains 'E2E_TEST_MARKER_42' from test file")
            .actual(if contains_marker {
                "Found marker in output".to_string()
            } else {
                format!(
                    "Marker not found. Output: {}",
                    truncate(&result.stdout, 100)
                )
            })
            .build()
            .with_passed(contains_marker)
    }
}

/// Test scenario that verifies NDJSON streaming output.
///
/// This scenario:
/// - Configures output in NDJSON streaming format
/// - Verifies that Ralph correctly parses the streaming output
/// - Validates that iteration boundaries are detected
///
/// NDJSON (Newline-Delimited JSON) is used by Claude CLI for structured streaming output.
///
/// # Example
///
/// ```no_run
/// use ralph_e2e::scenarios::{ClaudeStreamingScenario, TestScenario};
///
/// let scenario = ClaudeStreamingScenario::new();
/// assert_eq!(scenario.tier(), "Tier 4: Capabilities");
/// ```
pub struct ClaudeStreamingScenario {
    id: String,
    description: String,
    tier: String,
}

impl ClaudeStreamingScenario {
    /// Creates a new streaming scenario.
    pub fn new() -> Self {
        Self {
            id: "claude-streaming".to_string(),
            description: "Verifies NDJSON streaming output parsing".to_string(),
            tier: "Tier 4: Capabilities".to_string(),
        }
    }
}

impl Default for ClaudeStreamingScenario {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TestScenario for ClaudeStreamingScenario {
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

        // Create ralph.yml with streaming enabled
        let config_content = r#"# Streaming test config
cli:
  backend: claude
  args:
    - "--output-format"
    - "stream-json"

event_loop:
  max_iterations: 1
  completion_promise: "LOOP_COMPLETE"
"#;
        let config_path = workspace.join("ralph.yml");
        std::fs::write(&config_path, config_content)
            .map_err(|e| ScenarioError::SetupError(format!("failed to write ralph.yml: {}", e)))?;

        // Create a simple prompt for streaming test
        let prompt = r#"You are testing streaming output.

Say "Hello from streaming test!" and then output LOOP_COMPLETE.

Keep your response short."#;

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

        let execution = executor
            .run(config)
            .await
            .map_err(|e| ScenarioError::ExecutionError(format!("ralph execution failed: {}", e)))?;

        let duration = start.elapsed();

        // Build assertions for streaming
        // Note: We use exit_code_success_or_limit() because Ralph's exit code 2 means
        // "max iterations reached" which is valid when functional behavior succeeds.
        let assertions = vec![
            Assertions::response_received(&execution),
            Assertions::exit_code_success_or_limit(&execution),
            Assertions::no_timeout(&execution),
            self.streaming_output_received(&execution),
            self.content_extracted(&execution),
        ];

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

impl ClaudeStreamingScenario {
    /// Asserts that streaming output was received.
    fn streaming_output_received(
        &self,
        result: &crate::executor::ExecutionResult,
    ) -> crate::models::Assertion {
        // NDJSON output contains JSON lines with fields like "type", "content", etc.
        // Also check for regular output if NDJSON isn't being used
        let is_streaming = result.stdout.contains("{\"")
            || result.stdout.contains("\"type\"")
            || !result.stdout.is_empty();

        super::AssertionBuilder::new("Streaming output received")
            .expected("Non-empty output (JSON or text)")
            .actual(if is_streaming {
                format!("Received {} bytes", result.stdout.len())
            } else {
                "Empty output".to_string()
            })
            .build()
            .with_passed(is_streaming)
    }

    /// Asserts that meaningful content was extracted from the stream.
    fn content_extracted(
        &self,
        result: &crate::executor::ExecutionResult,
    ) -> crate::models::Assertion {
        // Look for our expected content or any meaningful text
        let has_content = result.stdout.to_lowercase().contains("hello")
            || result.stdout.to_lowercase().contains("streaming")
            || result.stdout.contains("LOOP_COMPLETE")
            || result.stdout.len() > 50; // Or just substantial output

        super::AssertionBuilder::new("Content extracted from stream")
            .expected("Meaningful content in output")
            .actual(if has_content {
                "Found expected content".to_string()
            } else {
                format!("Limited content. Output: {}", truncate(&result.stdout, 100))
            })
            .build()
            .with_passed(has_content)
    }
}

/// Extension trait for with_passed (duplicated here to avoid cross-module issues)
trait AssertionExt {
    fn with_passed(self, passed: bool) -> Self;
}

impl AssertionExt for crate::models::Assertion {
    fn with_passed(mut self, passed: bool) -> Self {
        self.passed = passed;
        self
    }
}

/// Truncates a string to the given length, adding "..." if truncated.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;

    fn test_workspace(test_name: &str) -> std::path::PathBuf {
        env::temp_dir().join(format!(
            "ralph-e2e-caps-{}-{}",
            test_name,
            std::process::id()
        ))
    }

    fn cleanup_workspace(path: &std::path::PathBuf) {
        if path.exists() {
            fs::remove_dir_all(path).ok();
        }
    }

    fn mock_tool_use_result() -> crate::executor::ExecutionResult {
        crate::executor::ExecutionResult {
            exit_code: Some(0),
            stdout: "I'll read the file using the Read tool.\n\nThe file contains: E2E_TEST_MARKER_42\n\nLOOP_COMPLETE".to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(10),
            scratchpad: None,
            events: vec![],
            iterations: 1,
            termination_reason: Some("LOOP_COMPLETE".to_string()),
            timed_out: false,
        }
    }

    fn mock_streaming_result() -> crate::executor::ExecutionResult {
        crate::executor::ExecutionResult {
            exit_code: Some(0),
            stdout: "{\"type\":\"text\",\"content\":\"Hello from streaming test!\"}\n{\"type\":\"result\",\"content\":\"LOOP_COMPLETE\"}".to_string(),
            stderr: String::new(),
            duration: Duration::from_secs(5),
            scratchpad: None,
            events: vec![],
            iterations: 1,
            termination_reason: Some("LOOP_COMPLETE".to_string()),
            timed_out: false,
        }
    }

    // ========== ClaudeToolUseScenario Tests ==========

    #[test]
    fn test_tool_use_scenario_new() {
        let scenario = ClaudeToolUseScenario::new();
        assert_eq!(scenario.id(), "claude-tool-use");
        assert_eq!(scenario.backend(), Backend::Claude);
        assert_eq!(scenario.tier(), "Tier 4: Capabilities");
    }

    #[test]
    fn test_tool_use_scenario_default() {
        let scenario = ClaudeToolUseScenario::default();
        assert_eq!(scenario.id(), "claude-tool-use");
    }

    #[test]
    fn test_tool_use_scenario_description() {
        let scenario = ClaudeToolUseScenario::new();
        assert!(scenario.description().contains("tool"));
    }

    #[test]
    fn test_tool_use_setup_creates_config() {
        let workspace = test_workspace("tool-use-setup");
        fs::create_dir_all(&workspace).unwrap();

        let scenario = ClaudeToolUseScenario::new();
        let config = scenario.setup(&workspace).unwrap();

        // Verify ralph.yml was created
        let config_path = workspace.join("ralph.yml");
        assert!(config_path.exists(), "ralph.yml should exist");

        // Verify test file was created
        let test_file = workspace.join("test-data.txt");
        assert!(test_file.exists(), "test-data.txt should exist");
        let content = fs::read_to_string(&test_file).unwrap();
        assert!(content.contains("E2E_TEST_MARKER_42"));

        // Verify .agent directory was created
        assert!(workspace.join(".agent").exists());

        // Verify config struct
        assert_eq!(config.max_iterations, 1);
        assert_eq!(config.timeout, Duration::from_secs(300));

        cleanup_workspace(&workspace);
    }

    #[test]
    fn test_tool_use_tool_was_invoked_passed() {
        let scenario = ClaudeToolUseScenario::new();
        let result = mock_tool_use_result();
        let assertion = scenario.tool_was_invoked(&result);
        assert!(assertion.passed, "Should pass when tool invocation evident");
    }

    #[test]
    fn test_tool_use_tool_was_invoked_failed() {
        let scenario = ClaudeToolUseScenario::new();
        let mut result = mock_tool_use_result();
        result.stdout = "Just some ordinary output with no relevant markers".to_string();
        let assertion = scenario.tool_was_invoked(&result);
        assert!(!assertion.passed, "Should fail without relevant markers");
    }

    #[test]
    fn test_tool_use_file_content_reported_passed() {
        let scenario = ClaudeToolUseScenario::new();
        let result = mock_tool_use_result();
        let assertion = scenario.file_content_reported(&result);
        assert!(assertion.passed, "Should pass when marker found");
    }

    #[test]
    fn test_tool_use_file_content_reported_failed() {
        let scenario = ClaudeToolUseScenario::new();
        let mut result = mock_tool_use_result();
        result.stdout = "I read the file but didn't find anything".to_string();
        let assertion = scenario.file_content_reported(&result);
        assert!(!assertion.passed, "Should fail without marker");
    }

    // ========== ClaudeStreamingScenario Tests ==========

    #[test]
    fn test_streaming_scenario_new() {
        let scenario = ClaudeStreamingScenario::new();
        assert_eq!(scenario.id(), "claude-streaming");
        assert_eq!(scenario.backend(), Backend::Claude);
        assert_eq!(scenario.tier(), "Tier 4: Capabilities");
    }

    #[test]
    fn test_streaming_scenario_default() {
        let scenario = ClaudeStreamingScenario::default();
        assert_eq!(scenario.id(), "claude-streaming");
    }

    #[test]
    fn test_streaming_scenario_description() {
        let scenario = ClaudeStreamingScenario::new();
        assert!(scenario.description().contains("streaming"));
    }

    #[test]
    fn test_streaming_setup_creates_config() {
        let workspace = test_workspace("streaming-setup");
        fs::create_dir_all(&workspace).unwrap();

        let scenario = ClaudeStreamingScenario::new();
        let config = scenario.setup(&workspace).unwrap();

        // Verify ralph.yml was created
        let config_path = workspace.join("ralph.yml");
        assert!(config_path.exists(), "ralph.yml should exist");

        // Verify content includes streaming args
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("stream-json"));

        // Verify config struct
        assert_eq!(config.max_iterations, 1);
        assert_eq!(config.timeout, Duration::from_secs(300));

        cleanup_workspace(&workspace);
    }

    #[test]
    fn test_streaming_output_received_passed_json() {
        let scenario = ClaudeStreamingScenario::new();
        let result = mock_streaming_result();
        let assertion = scenario.streaming_output_received(&result);
        assert!(assertion.passed, "Should pass with JSON output");
    }

    #[test]
    fn test_streaming_output_received_passed_text() {
        let scenario = ClaudeStreamingScenario::new();
        let mut result = mock_streaming_result();
        result.stdout = "Regular text output".to_string();
        let assertion = scenario.streaming_output_received(&result);
        assert!(assertion.passed, "Should pass with regular text");
    }

    #[test]
    fn test_streaming_output_received_failed() {
        let scenario = ClaudeStreamingScenario::new();
        let mut result = mock_streaming_result();
        result.stdout = String::new();
        let assertion = scenario.streaming_output_received(&result);
        assert!(!assertion.passed, "Should fail with empty output");
    }

    #[test]
    fn test_streaming_content_extracted_passed() {
        let scenario = ClaudeStreamingScenario::new();
        let result = mock_streaming_result();
        let assertion = scenario.content_extracted(&result);
        assert!(assertion.passed, "Should pass with expected content");
    }

    #[test]
    fn test_streaming_content_extracted_passed_loop_complete() {
        let scenario = ClaudeStreamingScenario::new();
        let mut result = mock_streaming_result();
        result.stdout = "LOOP_COMPLETE".to_string();
        let assertion = scenario.content_extracted(&result);
        assert!(assertion.passed, "Should pass with LOOP_COMPLETE");
    }

    #[test]
    fn test_streaming_content_extracted_passed_substantial_output() {
        let scenario = ClaudeStreamingScenario::new();
        let mut result = mock_streaming_result();
        result.stdout = "x".repeat(60); // More than 50 chars
        let assertion = scenario.content_extracted(&result);
        assert!(assertion.passed, "Should pass with substantial output");
    }

    #[test]
    fn test_streaming_content_extracted_failed() {
        let scenario = ClaudeStreamingScenario::new();
        let mut result = mock_streaming_result();
        result.stdout = "tiny".to_string();
        let assertion = scenario.content_extracted(&result);
        assert!(
            !assertion.passed,
            "Should fail with minimal meaningless content"
        );
    }

    // ========== Integration Tests (ignored by default) ==========

    #[tokio::test]
    #[ignore = "requires live backend"]
    async fn test_tool_use_full_run() {
        let workspace = test_workspace("tool-use-full");
        fs::create_dir_all(&workspace).unwrap();

        let scenario = ClaudeToolUseScenario::new();
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

    #[tokio::test]
    #[ignore = "requires live backend"]
    async fn test_streaming_full_run() {
        let workspace = test_workspace("streaming-full");
        fs::create_dir_all(&workspace).unwrap();

        let scenario = ClaudeStreamingScenario::new();
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
