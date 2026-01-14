//! Input routing for TUI prefix commands.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Input routing mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    AwaitingCommand,
}

/// Result of routing a key event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteResult {
    Forward(KeyEvent),
    Command(char),
    Consumed,
}

/// Routes input between normal mode and command mode.
pub struct InputRouter {
    mode: InputMode,
}

impl InputRouter {
    pub fn new() -> Self {
        Self {
            mode: InputMode::Normal,
        }
    }

    /// Routes a key event based on current mode.
    pub fn route_key(&mut self, key: KeyEvent) -> RouteResult {
        match self.mode {
            InputMode::Normal => {
                if is_prefix(key) {
                    self.mode = InputMode::AwaitingCommand;
                    RouteResult::Consumed
                } else {
                    RouteResult::Forward(key)
                }
            }
            InputMode::AwaitingCommand => {
                self.mode = InputMode::Normal;
                if let Some(c) = extract_char(key) {
                    RouteResult::Command(c)
                } else {
                    RouteResult::Consumed
                }
            }
        }
    }
}

impl Default for InputRouter {
    fn default() -> Self {
        Self::new()
    }
}

fn is_prefix(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('a')) && key.modifiers.contains(KeyModifiers::CONTROL)
}

fn extract_char(key: KeyEvent) -> Option<char> {
    match key.code {
        KeyCode::Char(c) => Some(c),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_mode_forwards_regular_keys() {
        let mut router = InputRouter::new();
        let key = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        assert_eq!(router.route_key(key), RouteResult::Forward(key));
    }

    #[test]
    fn ctrl_a_switches_to_awaiting_command() {
        let mut router = InputRouter::new();
        let key = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert_eq!(router.route_key(key), RouteResult::Consumed);
    }

    #[test]
    fn next_key_after_ctrl_a_returns_command() {
        let mut router = InputRouter::new();
        let prefix = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        router.route_key(prefix);

        let cmd = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(router.route_key(cmd), RouteResult::Command('q'));
    }

    #[test]
    fn state_resets_to_normal_after_command() {
        let mut router = InputRouter::new();
        let prefix = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
        router.route_key(prefix);

        let cmd = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        router.route_key(cmd);

        let next = KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE);
        assert_eq!(router.route_key(next), RouteResult::Forward(next));
    }
}
