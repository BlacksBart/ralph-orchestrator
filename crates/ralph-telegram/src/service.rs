use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tracing::{debug, info, warn};

use crate::error::{TelegramError, TelegramResult};
use crate::handler::MessageHandler;
use crate::state::StateManager;

/// Coordinates the Telegram bot lifecycle with the Ralph event loop.
///
/// Manages startup, shutdown, message sending, and response waiting.
pub struct TelegramService {
    workspace_root: PathBuf,
    bot_token: String,
    timeout_secs: u64,
    loop_id: String,
    state_manager: StateManager,
    handler: MessageHandler,
}

impl TelegramService {
    /// Create a new TelegramService.
    ///
    /// Resolves the bot token from config or `RALPH_TELEGRAM_BOT_TOKEN` env var.
    pub fn new(
        workspace_root: PathBuf,
        bot_token: Option<String>,
        timeout_secs: u64,
        loop_id: String,
    ) -> TelegramResult<Self> {
        let resolved_token = bot_token
            .or_else(|| std::env::var("RALPH_TELEGRAM_BOT_TOKEN").ok())
            .ok_or(TelegramError::MissingBotToken)?;

        let state_path = workspace_root.join(".ralph/telegram-state.json");
        let state_manager = StateManager::new(&state_path);
        let handler_state_manager = StateManager::new(&state_path);
        let handler = MessageHandler::new(handler_state_manager, &workspace_root);

        Ok(Self {
            workspace_root,
            bot_token: resolved_token,
            timeout_secs,
            loop_id,
            state_manager,
            handler,
        })
    }

    /// Get a reference to the workspace root.
    pub fn workspace_root(&self) -> &PathBuf {
        &self.workspace_root
    }

    /// Get the configured timeout in seconds.
    pub fn timeout_secs(&self) -> u64 {
        self.timeout_secs
    }

    /// Get a reference to the bot token (masked for logging).
    pub fn bot_token_masked(&self) -> String {
        if self.bot_token.len() > 8 {
            format!(
                "{}...{}",
                &self.bot_token[..4],
                &self.bot_token[self.bot_token.len() - 4..]
            )
        } else {
            "****".to_string()
        }
    }

    /// Get a reference to the state manager.
    pub fn state_manager(&self) -> &StateManager {
        &self.state_manager
    }

    /// Get a mutable reference to the message handler.
    pub fn handler(&mut self) -> &mut MessageHandler {
        &mut self.handler
    }

    /// Get the loop ID this service is associated with.
    pub fn loop_id(&self) -> &str {
        &self.loop_id
    }

    /// Start the Telegram service.
    ///
    /// Initializes the bot connection and prepares to send/receive messages.
    /// This must be called before sending questions or waiting for responses.
    pub fn start(&self) -> TelegramResult<()> {
        info!(
            bot_token = %self.bot_token_masked(),
            workspace = %self.workspace_root.display(),
            timeout_secs = self.timeout_secs,
            "Telegram service started"
        );
        Ok(())
    }

    /// Stop the Telegram service gracefully.
    ///
    /// Shuts down the bot connection and cleans up resources.
    pub fn stop(&self) {
        info!(
            workspace = %self.workspace_root.display(),
            "Telegram service stopped"
        );
    }

    /// Send a question to the human via Telegram and store it as a pending question.
    ///
    /// The question payload is extracted from the `ask.human` event. A pending
    /// question is stored in the state manager so that incoming replies can be
    /// routed back to the correct loop.
    ///
    /// Returns the message ID of the sent Telegram message, or 0 if no chat ID
    /// is configured (question is logged but not sent).
    pub fn send_question(&self, payload: &str) -> TelegramResult<i32> {
        let mut state = self.state_manager.load_or_default()?;

        let message_id = if let Some(_chat_id) = state.chat_id {
            // TODO: actual Telegram bot send via BotApi when real bot is integrated
            // For now, log the question. The message_id is a placeholder.
            info!(
                loop_id = %self.loop_id,
                "ask.human question sent: {}",
                payload
            );
            0
        } else {
            warn!(
                loop_id = %self.loop_id,
                "No chat ID configured — ask.human question logged but not sent: {}",
                payload
            );
            0
        };

        self.state_manager
            .add_pending_question(&mut state, &self.loop_id, message_id)?;

        debug!(
            loop_id = %self.loop_id,
            message_id = message_id,
            "Stored pending question"
        );

        Ok(message_id)
    }

    /// Poll the events file for a `human.response` event, blocking until one
    /// arrives or the configured timeout expires.
    ///
    /// Polls the given `events_path` every second for new lines containing
    /// `"human.response"`. On response, removes the pending question and
    /// returns the response message. On timeout, removes the pending question
    /// and returns `None`.
    pub fn wait_for_response(&self, events_path: &Path) -> TelegramResult<Option<String>> {
        let timeout = Duration::from_secs(self.timeout_secs);
        let poll_interval = Duration::from_secs(1);
        let deadline = Instant::now() + timeout;

        // Track file position to only read new lines
        let initial_pos = if events_path.exists() {
            std::fs::metadata(events_path).map(|m| m.len()).unwrap_or(0)
        } else {
            0
        };
        let mut file_pos = initial_pos;

        info!(
            loop_id = %self.loop_id,
            timeout_secs = self.timeout_secs,
            events_path = %events_path.display(),
            "Waiting for human.response"
        );

        loop {
            if Instant::now() >= deadline {
                warn!(
                    loop_id = %self.loop_id,
                    timeout_secs = self.timeout_secs,
                    "Timed out waiting for human.response"
                );

                // Remove pending question on timeout
                if let Ok(mut state) = self.state_manager.load_or_default() {
                    let _ = self
                        .state_manager
                        .remove_pending_question(&mut state, &self.loop_id);
                }

                return Ok(None);
            }

            // Read new lines from the events file
            if let Some(response) = Self::check_for_response(events_path, &mut file_pos)? {
                info!(
                    loop_id = %self.loop_id,
                    "Received human.response: {}",
                    response
                );

                // Remove pending question on response
                if let Ok(mut state) = self.state_manager.load_or_default() {
                    let _ = self
                        .state_manager
                        .remove_pending_question(&mut state, &self.loop_id);
                }

                return Ok(Some(response));
            }

            std::thread::sleep(poll_interval);
        }
    }

    /// Check the events file for a `human.response` event starting from
    /// `file_pos`. Updates `file_pos` to the new end of file.
    fn check_for_response(
        events_path: &Path,
        file_pos: &mut u64,
    ) -> TelegramResult<Option<String>> {
        use std::io::{BufRead, BufReader, Seek, SeekFrom};

        if !events_path.exists() {
            return Ok(None);
        }

        let mut file = std::fs::File::open(events_path)?;
        file.seek(SeekFrom::Start(*file_pos))?;

        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line?;
            let line_bytes = line.len() as u64 + 1; // +1 for newline
            *file_pos += line_bytes;

            if line.trim().is_empty() {
                continue;
            }

            // Try to parse as JSON event
            if let Ok(event) = serde_json::from_str::<serde_json::Value>(&line)
                && event.get("topic").and_then(|t| t.as_str()) == Some("human.response")
            {
                let message = event
                    .get("payload")
                    .and_then(|p| p.as_str())
                    .unwrap_or("")
                    .to_string();
                return Ok(Some(message));
            }

            // Also check pipe-separated format (written by MessageHandler)
            if line.contains("EVENT: human.response") {
                // Extract message from pipe-separated format:
                // EVENT: human.response | message: "..." | timestamp: "..."
                let message = line
                    .split('|')
                    .find(|part| part.trim().starts_with("message:"))
                    .and_then(|part| {
                        let value = part.trim().strip_prefix("message:")?;
                        let trimmed = value.trim().trim_matches('"');
                        Some(trimmed.to_string())
                    })
                    .unwrap_or_default();
                return Ok(Some(message));
            }
        }

        Ok(None)
    }
}

impl fmt::Debug for TelegramService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TelegramService")
            .field("workspace_root", &self.workspace_root)
            .field("bot_token", &self.bot_token_masked())
            .field("timeout_secs", &self.timeout_secs)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn test_service(dir: &TempDir) -> TelegramService {
        TelegramService::new(
            dir.path().to_path_buf(),
            Some("test-token-12345".to_string()),
            300,
            "main".to_string(),
        )
        .unwrap()
    }

    #[test]
    fn new_with_explicit_token() {
        let dir = TempDir::new().unwrap();
        let service = TelegramService::new(
            dir.path().to_path_buf(),
            Some("test-token-12345".to_string()),
            300,
            "main".to_string(),
        );
        assert!(service.is_ok());
    }

    #[test]
    fn new_without_token_fails() {
        // Only run this test when the env var is not set,
        // to avoid needing unsafe remove_var
        if std::env::var("RALPH_TELEGRAM_BOT_TOKEN").is_ok() {
            return;
        }

        let dir = TempDir::new().unwrap();
        let service = TelegramService::new(dir.path().to_path_buf(), None, 300, "main".to_string());
        assert!(service.is_err());
        assert!(matches!(
            service.unwrap_err(),
            TelegramError::MissingBotToken
        ));
    }

    #[test]
    fn bot_token_masked_works() {
        let dir = TempDir::new().unwrap();
        let service = TelegramService::new(
            dir.path().to_path_buf(),
            Some("abcd1234efgh5678".to_string()),
            300,
            "main".to_string(),
        )
        .unwrap();
        let masked = service.bot_token_masked();
        assert_eq!(masked, "abcd...5678");
    }

    #[test]
    fn loop_id_accessor() {
        let dir = TempDir::new().unwrap();
        let service = TelegramService::new(
            dir.path().to_path_buf(),
            Some("token".to_string()),
            60,
            "feature-auth".to_string(),
        )
        .unwrap();
        assert_eq!(service.loop_id(), "feature-auth");
    }

    #[test]
    fn send_question_stores_pending_question() {
        let dir = TempDir::new().unwrap();
        let service = test_service(&dir);

        service.send_question("Which DB to use?").unwrap();

        // Verify pending question is stored
        let state = service.state_manager().load_or_default().unwrap();
        assert!(
            state.pending_questions.contains_key("main"),
            "pending question should be stored for loop_id 'main'"
        );
    }

    #[test]
    fn send_question_returns_message_id() {
        let dir = TempDir::new().unwrap();
        let service = test_service(&dir);

        let msg_id = service.send_question("async or sync?").unwrap();
        // Without a real bot, message_id is 0
        assert_eq!(msg_id, 0);
    }

    #[test]
    fn check_for_response_json_format() {
        let dir = TempDir::new().unwrap();
        let events_path = dir.path().join("events.jsonl");

        // Write a non-response event first
        let mut file = std::fs::File::create(&events_path).unwrap();
        writeln!(
            file,
            r#"{{"topic":"build.done","payload":"tests: pass","ts":"2026-01-30T00:00:00Z"}}"#
        )
        .unwrap();
        // Write a human.response event
        writeln!(
            file,
            r#"{{"topic":"human.response","payload":"Use async","ts":"2026-01-30T00:01:00Z"}}"#
        )
        .unwrap();
        file.flush().unwrap();

        let mut pos = 0;
        let result = TelegramService::check_for_response(&events_path, &mut pos).unwrap();
        assert_eq!(result, Some("Use async".to_string()));
    }

    #[test]
    fn check_for_response_pipe_format() {
        let dir = TempDir::new().unwrap();
        let events_path = dir.path().join("events.jsonl");

        let mut file = std::fs::File::create(&events_path).unwrap();
        writeln!(
            file,
            r#"EVENT: human.response | message: "Use sync" | timestamp: "2026-01-30T00:01:00Z""#
        )
        .unwrap();
        file.flush().unwrap();

        let mut pos = 0;
        let result = TelegramService::check_for_response(&events_path, &mut pos).unwrap();
        assert_eq!(result, Some("Use sync".to_string()));
    }

    #[test]
    fn check_for_response_skips_non_response_events() {
        let dir = TempDir::new().unwrap();
        let events_path = dir.path().join("events.jsonl");

        let mut file = std::fs::File::create(&events_path).unwrap();
        writeln!(
            file,
            r#"{{"topic":"build.done","payload":"done","ts":"2026-01-30T00:00:00Z"}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"topic":"human.guidance","payload":"check errors","ts":"2026-01-30T00:01:00Z"}}"#
        )
        .unwrap();
        file.flush().unwrap();

        let mut pos = 0;
        let result = TelegramService::check_for_response(&events_path, &mut pos).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn check_for_response_missing_file() {
        let dir = TempDir::new().unwrap();
        let events_path = dir.path().join("does-not-exist.jsonl");

        let mut pos = 0;
        let result = TelegramService::check_for_response(&events_path, &mut pos).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn check_for_response_tracks_position() {
        let dir = TempDir::new().unwrap();
        let events_path = dir.path().join("events.jsonl");

        // Write one event
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&events_path)
            .unwrap();
        writeln!(
            file,
            r#"{{"topic":"build.done","payload":"done","ts":"2026-01-30T00:00:00Z"}}"#
        )
        .unwrap();
        file.flush().unwrap();

        let mut pos = 0;
        let result = TelegramService::check_for_response(&events_path, &mut pos).unwrap();
        assert_eq!(result, None);
        assert!(pos > 0, "position should advance after reading");

        let pos_after_first = pos;

        // Append a human.response
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&events_path)
            .unwrap();
        writeln!(
            file,
            r#"{{"topic":"human.response","payload":"yes","ts":"2026-01-30T00:02:00Z"}}"#
        )
        .unwrap();
        file.flush().unwrap();

        // Should find the response starting from where we left off
        let result = TelegramService::check_for_response(&events_path, &mut pos).unwrap();
        assert_eq!(result, Some("yes".to_string()));
        assert!(pos > pos_after_first, "position should advance further");
    }

    #[test]
    fn wait_for_response_returns_on_response() {
        let dir = TempDir::new().unwrap();
        let service = TelegramService::new(
            dir.path().to_path_buf(),
            Some("token".to_string()),
            5, // enough time for the writer thread
            "main".to_string(),
        )
        .unwrap();

        let events_path = dir.path().join("events.jsonl");
        // Create an empty events file so wait_for_response records position 0
        std::fs::File::create(&events_path).unwrap();

        // Store a pending question first
        service.send_question("Which plan?").unwrap();

        // Spawn a thread to write the response after a brief delay
        let writer_path = events_path.clone();
        let writer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(&writer_path)
                .unwrap();
            writeln!(
                file,
                r#"{{"topic":"human.response","payload":"Go with plan A","ts":"2026-01-30T00:00:00Z"}}"#
            )
            .unwrap();
            file.flush().unwrap();
        });

        let result = service.wait_for_response(&events_path).unwrap();
        writer.join().unwrap();

        assert_eq!(result, Some("Go with plan A".to_string()));

        // Pending question should be removed
        let state = service.state_manager().load_or_default().unwrap();
        assert!(
            !state.pending_questions.contains_key("main"),
            "pending question should be removed after response"
        );
    }

    #[test]
    fn wait_for_response_returns_none_on_timeout() {
        let dir = TempDir::new().unwrap();
        let service = TelegramService::new(
            dir.path().to_path_buf(),
            Some("token".to_string()),
            1, // 1 second timeout
            "main".to_string(),
        )
        .unwrap();

        let events_path = dir.path().join("events.jsonl");
        // Create an empty events file with no human.response
        std::fs::File::create(&events_path).unwrap();

        // Store a pending question
        service.send_question("Will this timeout?").unwrap();

        let result = service.wait_for_response(&events_path).unwrap();
        assert_eq!(result, None, "should return None on timeout");

        // Pending question should be removed even on timeout
        let state = service.state_manager().load_or_default().unwrap();
        assert!(
            !state.pending_questions.contains_key("main"),
            "pending question should be removed on timeout"
        );
    }
}
