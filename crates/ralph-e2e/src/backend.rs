//! Backend detection and authentication checking.
//!
//! This module provides functionality to detect which AI backends are available
//! and whether they are properly authenticated.

use std::fmt;

/// Supported AI backends for E2E testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Backend {
    /// Claude CLI backend
    Claude,
    /// Kiro CLI backend
    Kiro,
    /// OpenCode CLI backend
    OpenCode,
}

impl Backend {
    /// Returns the CLI command name for this backend.
    pub fn command(&self) -> &'static str {
        match self {
            Backend::Claude => "claude",
            Backend::Kiro => "kiro-cli",
            Backend::OpenCode => "opencode",
        }
    }

    /// Returns all available backends.
    pub fn all() -> &'static [Backend] {
        &[Backend::Claude, Backend::Kiro, Backend::OpenCode]
    }
}

impl fmt::Display for Backend {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Backend::Claude => write!(f, "Claude"),
            Backend::Kiro => write!(f, "Kiro"),
            Backend::OpenCode => write!(f, "OpenCode"),
        }
    }
}
