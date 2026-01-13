//! Auto-detection logic for agent backends.
//!
//! When config specifies `agent: auto`, this module handles detecting
//! which backends are available in the system PATH.

use std::process::Command;
use std::sync::OnceLock;

/// Default priority order for backend detection.
pub const DEFAULT_PRIORITY: &[&str] = &["claude", "gemini", "codex", "amp"];

/// Cached detection result for session duration.
static DETECTED_BACKEND: OnceLock<Option<String>> = OnceLock::new();

/// Error returned when no backends are available.
#[derive(Debug, Clone)]
pub struct NoBackendError {
    /// Backends that were checked.
    pub checked: Vec<String>,
}

impl std::fmt::Display for NoBackendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "No supported AI backend found in PATH.")?;
        writeln!(f)?;
        writeln!(f, "Checked backends: {}", self.checked.join(", "))?;
        writeln!(f)?;
        writeln!(f, "Install one of the following:")?;
        writeln!(f, "  • Claude CLI: https://docs.anthropic.com/claude-code")?;
        writeln!(f, "  • Gemini CLI: https://cloud.google.com/gemini")?;
        writeln!(f, "  • Codex CLI:  https://openai.com/codex")?;
        writeln!(f, "  • Amp CLI:    https://amp.dev")?;
        Ok(())
    }
}

impl std::error::Error for NoBackendError {}

/// Checks if a backend is available by running its version command.
///
/// Each backend is detected by running `<command> --version` and checking
/// for exit code 0.
pub fn is_backend_available(backend: &str) -> bool {
    let result = Command::new(backend).arg("--version").output();

    match result {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

/// Detects the first available backend from a priority list.
///
/// # Arguments
/// * `priority` - List of backend names to check in order
/// * `adapter_enabled` - Function that returns whether an adapter is enabled in config
///
/// # Returns
/// * `Ok(backend_name)` - First available backend
/// * `Err(NoBackendError)` - No backends available
pub fn detect_backend<F>(priority: &[&str], adapter_enabled: F) -> Result<String, NoBackendError>
where
    F: Fn(&str) -> bool,
{
    // Check cache first
    if let Some(cached) = DETECTED_BACKEND.get() {
        if let Some(backend) = cached {
            return Ok(backend.clone());
        }
    }

    let mut checked = Vec::new();

    for &backend in priority {
        // Skip if adapter is disabled in config
        if !adapter_enabled(backend) {
            continue;
        }

        checked.push(backend.to_string());

        if is_backend_available(backend) {
            // Cache the result (ignore if already set)
            let _ = DETECTED_BACKEND.set(Some(backend.to_string()));
            return Ok(backend.to_string());
        }
    }

    // Cache the failure too
    let _ = DETECTED_BACKEND.set(None);

    Err(NoBackendError { checked })
}

/// Detects a backend using default priority and all adapters enabled.
pub fn detect_backend_default() -> Result<String, NoBackendError> {
    detect_backend(DEFAULT_PRIORITY, |_| true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_backend_available_echo() {
        // 'echo' command should always be available
        let result = Command::new("echo").arg("--version").output();
        // Just verify the command runs without panic
        assert!(result.is_ok());
    }

    #[test]
    fn test_is_backend_available_nonexistent() {
        // Nonexistent command should return false
        assert!(!is_backend_available("definitely_not_a_real_command_xyz123"));
    }

    #[test]
    fn test_detect_backend_with_disabled_adapters() {
        // All adapters disabled should fail
        let result = detect_backend(&["claude", "gemini"], |_| false);
        // Should return error since all are disabled (empty checked list)
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.checked.is_empty());
        }
    }

    #[test]
    fn test_no_backend_error_display() {
        let err = NoBackendError {
            checked: vec!["claude".to_string(), "gemini".to_string()],
        };
        let msg = format!("{}", err);
        assert!(msg.contains("No supported AI backend found"));
        assert!(msg.contains("claude, gemini"));
    }
}
