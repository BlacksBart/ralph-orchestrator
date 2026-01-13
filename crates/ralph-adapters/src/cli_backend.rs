//! CLI backend definitions for different AI tools.

use ralph_core::CliConfig;

/// How to pass prompts to the CLI tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptMode {
    /// Pass prompt as a command-line argument.
    Arg,
    /// Write prompt to stdin.
    Stdin,
}

/// A CLI backend configuration for executing prompts.
#[derive(Debug, Clone)]
pub struct CliBackend {
    /// The command to execute.
    pub command: String,
    /// Additional arguments before the prompt.
    pub args: Vec<String>,
    /// How to pass the prompt.
    pub prompt_mode: PromptMode,
    /// Argument flag for prompt (if prompt_mode is Arg).
    pub prompt_flag: Option<String>,
}

impl CliBackend {
    /// Creates a backend from configuration.
    pub fn from_config(config: &CliConfig) -> Self {
        match config.backend.as_str() {
            "claude" => Self::claude(),
            "gemini" => Self::gemini(),
            "codex" => Self::codex(),
            "amp" => Self::amp(),
            "custom" => Self::custom(config),
            _ => Self::claude(), // Default to claude
        }
    }

    /// Creates the Claude backend.
    pub fn claude() -> Self {
        Self {
            command: "claude".to_string(),
            args: vec!["--dangerously-skip-permissions".to_string()],
            prompt_mode: PromptMode::Arg,
            prompt_flag: Some("-p".to_string()),
        }
    }

    /// Creates the Gemini backend.
    pub fn gemini() -> Self {
        Self {
            command: "gemini".to_string(),
            args: vec![],
            prompt_mode: PromptMode::Stdin,
            prompt_flag: None,
        }
    }

    /// Creates the Codex backend.
    pub fn codex() -> Self {
        Self {
            command: "codex".to_string(),
            args: vec![],
            prompt_mode: PromptMode::Arg,
            prompt_flag: Some("--prompt".to_string()),
        }
    }

    /// Creates the Amp backend.
    pub fn amp() -> Self {
        Self {
            command: "amp".to_string(),
            args: vec![],
            prompt_mode: PromptMode::Stdin,
            prompt_flag: None,
        }
    }

    /// Creates a custom backend from configuration.
    pub fn custom(config: &CliConfig) -> Self {
        let command = config.command.clone().unwrap_or_else(|| "echo".to_string());
        let prompt_mode = if config.prompt_mode == "stdin" {
            PromptMode::Stdin
        } else {
            PromptMode::Arg
        };

        Self {
            command,
            args: vec![],
            prompt_mode,
            prompt_flag: None,
        }
    }

    /// Builds the full command with arguments for execution.
    pub fn build_command(&self, prompt: &str) -> (String, Vec<String>, Option<String>) {
        let mut args = self.args.clone();

        let stdin_input = match self.prompt_mode {
            PromptMode::Arg => {
                if let Some(ref flag) = self.prompt_flag {
                    args.push(flag.clone());
                }
                args.push(prompt.to_string());
                None
            }
            PromptMode::Stdin => Some(prompt.to_string()),
        };

        (self.command.clone(), args, stdin_input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_backend() {
        let backend = CliBackend::claude();
        let (cmd, args, stdin) = backend.build_command("test prompt");

        assert_eq!(cmd, "claude");
        assert_eq!(
            args,
            vec!["--dangerously-skip-permissions", "-p", "test prompt"]
        );
        assert!(stdin.is_none());
    }

    #[test]
    fn test_gemini_backend() {
        let backend = CliBackend::gemini();
        let (cmd, args, stdin) = backend.build_command("test prompt");

        assert_eq!(cmd, "gemini");
        assert!(args.is_empty());
        assert_eq!(stdin, Some("test prompt".to_string()));
    }

    #[test]
    fn test_from_config() {
        let config = CliConfig {
            backend: "claude".to_string(),
            command: None,
            prompt_mode: "arg".to_string(),
        };
        let backend = CliBackend::from_config(&config);

        assert_eq!(backend.command, "claude");
        assert_eq!(backend.prompt_mode, PromptMode::Arg);
    }
}
